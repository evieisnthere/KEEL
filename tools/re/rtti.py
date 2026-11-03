"""Find MSVC x64 vtables for a class name via RTTI. Usage: rtti.py exe ClassName"""
import sys, struct, pefile
pe = pefile.PE(sys.argv[1], fast_load=True); base = pe.OPTIONAL_HEADER.ImageBase; img = bytes(pe.get_memory_mapped_image())
def vtables(cls):
    s = f'.?AU{cls}@@'.encode(); i = img.find(s + b'\0')
    if i < 0: s = f'.?AV{cls}@@'.encode(); i = img.find(s + b'\0')
    td_rva = i - 0x10
    cols = []
    for j in range(0, len(img) - 24, 4):
        if struct.unpack_from('<I', img, j + 12)[0] == td_rva and struct.unpack_from('<I', img, j)[0] == 1:
            if struct.unpack_from('<I', img, j + 20)[0] == j: cols.append(j)
    out = []
    for c in cols:
        needle = struct.pack('<Q', base + c); k = img.find(needle)
        while k >= 0:
            vt = k + 8; offs = struct.unpack_from('<I', img, c + 4)[0]
            fns = []
            for n in range(int(__import__("os").environ.get("VSLOTS","80"))):
                f = struct.unpack_from('<Q', img, vt + 8 * n)[0]
                if not (base + 0x1000 <= f < base + 0xa96000): break
                fns.append(f)
            out.append((base + vt, offs, fns)); k = img.find(needle, k + 1)
    return out
if __name__ == '__main__':
    for vt, off, fns in vtables(sys.argv[2]):
        print(f'vtable {vt:#x} (offset {off}) {len(fns)} slots'); print(' '.join(f'{i}:{f:#x}' for i, f in enumerate(fns)))
