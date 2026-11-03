"""Emulate the game's surface converter for every (orientation, rotation, definition shape) with identity component rotation.
Usage: python extract_surface_conversion.py <stormworks64.exe> <out.json>"""
import sys, struct, json, collections, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__))); from emu import Emu
exe, outp = sys.argv[1], sys.argv[2]
e = Emu(exe, stubs_ret0=[0x14095c4a8, 0x14095c448, 0x14095c974], allocators=[0x14095c6f8], frees=[0x14095c2ec])
e.u.mem_write(0x140d03b70, struct.pack('<i', -1)); e.u.mem_write(e.TEB + 0x2000 + 4, struct.pack('<i', -0x7fffffff))
e.call(0x1402f2970)
e.u.mem_write(e.TEB + 0x2000 + 4, struct.pack('<i', 0x7fffffff))  # mark static as initialised
B = e.alloc(0x1000)
ints = lambda a, n: list(struct.unpack('<%di' % n, e.read(a, 4 * n)))
out = {"orient_rot": {}, "conv": {}}; seen = collections.Counter(); errors = []
for o in range(6):
    for r in range(4):
        M = B; e.u.mem_write(M, b'\0' * 36); e.call(0x1402fa0c0, o, r, M); m = ints(M, 9)
        S = B + 0x100; e.u.mem_write(S, b'\0' * 0x40)
        e.u.mem_write(S + 0xc, struct.pack('<ii', o, r)); e.u.mem_write(S + 0x14, struct.pack('<9i', *m))
        out["orient_rot"][f"{o},{r}"] = {"matrix": m}
        for s in range(0, 80):
            e.u.mem_write(S + 0x38, struct.pack('<i', s))
            p = e.call(0x1402f8d40, S, B + 0x200); f = list(struct.unpack('<3f', e.read(p, 12)))
            for c in f:                      # game: divide by first |c| in (0,1), then round(x + 0.5) via int()
                if 0.0 < abs(c) < 1.0: f = [x / abs(c) for x in f]; break
            import math
            n = [int(math.floor(x + 0.5)) for x in f]
            e.u.mem_write(B + 0x300, struct.pack('<3i', *n))
            e.u.mem_write(B + 0x340, struct.pack('<ii', -1, -1))
            try: cnt = e.call(0x140483460, M, s, B + 0x300, B + 0x340, B + 0x344) & 0xffffffff
            except Exception as x: errors.append((o, r, s)); continue
            a3, a4 = struct.unpack('<ii', e.read(B + 0x340, 8))
            if 0 < cnt < 10:
                out["conv"][f"{o},{r},{s}"] = {"normal": n, "types": [a3] if cnt == 1 else [a3, a4]}; seen[s] += 1
out.update(source="emulated sub_1402fa0c0 / sub_1402f8d40 / sub_140483460 (stormworks64.exe v1.15.26)",
           evidence="E1, component rotation = identity", errors=errors)
json.dump(out, open(outp, 'w'), indent=0)
print("shape ids converted:", sorted(seen.items())); print("errors:", len(errors))
