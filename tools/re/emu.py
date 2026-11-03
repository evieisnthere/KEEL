"""Minimal unicorn harness: run a self-contained initializer from the game EXE and read back memory.
Stubs: MSVC thread-safe-static guards, atexit, operator new/delete (bump allocator)."""
import sys, struct, pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_64, UC_HOOK_CODE, UC_HOOK_MEM_UNMAPPED, UcError
from unicorn.x86_const import *

class Emu:
    def __init__(self, path, stubs_ret0=(), allocators=(), frees=()):
        pe = pefile.PE(path, fast_load=True); self.pe = pe
        self.base = pe.OPTIONAL_HEADER.ImageBase
        img = pe.get_memory_mapped_image(); size = (len(img) + 0xFFFF) & ~0xFFFF
        u = Uc(UC_ARCH_X86, UC_MODE_64); self.u = u
        u.mem_map(self.base, size + 0x100000); u.mem_write(self.base, img)
        self.STACK = 0x7f0000000000; u.mem_map(self.STACK - 0x200000, 0x200000)
        self.HEAP = 0x600000000000; self.hp = self.HEAP; u.mem_map(self.HEAP, 0x10000000)
        self.TEB = 0x500000000000; u.mem_map(self.TEB, 0x10000)
        # gs:[0x58] -> TLS array -> [0] -> TLS block (zeroed)
        u.mem_write(self.TEB + 0x58, struct.pack('<Q', self.TEB + 0x1000))
        u.mem_write(self.TEB + 0x1000, struct.pack('<Q', self.TEB + 0x2000))
        u.reg_write(UC_X86_REG_GS_BASE, self.TEB)
        self.RET = 0x400000000000; u.mem_map(self.RET, 0x1000); u.mem_write(self.RET, b'\xf4')  # hlt
        self.stubs = {a: 'ret0' for a in stubs_ret0}
        self.stubs.update({a: 'alloc' for a in allocators}); self.stubs.update({a: 'free' for a in frees})
        self.trace = []
        # bind every import to a stub address; hook dispatches by name
        self.IMP = 0x410000000000; u.mem_map(self.IMP, 0x100000); u.mem_write(self.IMP, b'\xc3' * 0x100000)
        self.impnames = {}
        pe.parse_data_directories()
        k = 0
        for d in getattr(pe, 'DIRECTORY_ENTRY_IMPORT', []):
            for imp in d.imports:
                a = self.IMP + 16 * k; k += 1
                self.impnames[a] = (imp.name or b'ord').decode()
                u.mem_write(imp.address, struct.pack('<Q', a))
        self.unknown_imports = set()
        u.hook_add(UC_HOOK_CODE, self._code, begin=self.IMP, end=self.IMP + 0x100000)
        for a in list(self.stubs):
            u.hook_add(UC_HOOK_CODE, self._code, begin=a, end=a)
        self._imp_hook_pending = True
        u.hook_add(UC_HOOK_MEM_UNMAPPED, self._bad)
    def alloc(self, n):
        p = (self.hp + 15) & ~15; self.hp = p + max(n, 16); return p
    def _ret(self, val):
        u = self.u; rsp = u.reg_read(UC_X86_REG_RSP)
        ra = struct.unpack('<Q', u.mem_read(rsp, 8))[0]
        u.reg_write(UC_X86_REG_RSP, rsp + 8); u.reg_write(UC_X86_REG_RIP, ra); u.reg_write(UC_X86_REG_RAX, val)
    def _code(self, u, addr, size, _):
        if self.IMP <= addr < self.IMP + 0x100000:
            n = self.impnames.get(addr, '?')
            if n in ('HeapAlloc', 'RtlAllocateHeap'):
                sz = u.reg_read(UC_X86_REG_R8); p = self.alloc(sz); u.mem_write(p, b'\0' * sz); self._ret(p)
            elif n in ('HeapReAlloc',):
                old = u.reg_read(UC_X86_REG_R8); sz = u.reg_read(UC_X86_REG_R9); p = self.alloc(sz)
                u.mem_write(p, self.read(old, sz)); self._ret(p)
            elif n == 'HeapSize': self._ret(0x10000)
            elif n in ('GetProcessHeap', 'HeapFree', 'TlsGetValue', 'FlsGetValue'): self._ret(1 if n != 'TlsGetValue' else 0)
            else: self.unknown_imports.add(n); self._ret(0)
            return
        k = self.stubs.get(addr)
        if k == 'ret0': self._ret(0)
        elif k == 'alloc':
            n = u.reg_read(UC_X86_REG_RCX); p = self.alloc(n); u.mem_write(p, b'\0' * n); self._ret(p)
        elif k == 'free': self._ret(0)

    def _bad(self, u, access, addr, size, value, _):
        print(f"unmapped access {addr:#x} at rip {u.reg_read(UC_X86_REG_RIP):#x}"); return False
    def call(self, fn, *args, limit=5_000_000):
        u = self.u; rsp = self.STACK - 0x10000
        extra = list(args[4:])
        rsp -= 8 * (len(extra) + 4 + 1); rsp &= ~0xF; rsp -= 8
        u.mem_write(rsp, struct.pack('<Q', self.RET))
        for i, v in enumerate(extra): u.mem_write(rsp + 0x28 + 8 * i, struct.pack('<Q', v & 0xFFFFFFFFFFFFFFFF))
        u.reg_write(UC_X86_REG_RSP, rsp)
        for r, v in zip((UC_X86_REG_RCX, UC_X86_REG_RDX, UC_X86_REG_R8, UC_X86_REG_R9), args): u.reg_write(r, v)
        try: u.emu_start(fn, self.RET, count=limit)
        except UcError as e:
            print('emu error', e, hex(u.reg_read(UC_X86_REG_RIP)), [hex(a) for a in self.trace[-12:]]); raise
        return u.reg_read(UC_X86_REG_RAX)
    def read(self, a, n): return bytes(self.u.mem_read(a, n))
