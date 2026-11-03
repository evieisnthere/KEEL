"""Sealed-volume float ratio per vehicle (main body). Usage: sealed_check.py defs_dir metadata_dir vehicle.xml..."""
import sys, os, collections, functools
here = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, here); sys.path.insert(0, os.path.join(here, '..', '..', 'sim-reference'))
from defs_load import load
import sw_buoyancy as B, sw_hydro as H
defs, _ = load(sys.argv[1]); T = B.SurfaceTables(sys.argv[2])
mass = {k: float(v.get('mass', 0) or 0) for k, v in defs.items()}
I = (1, 0, 0, 0, 1, 0, 0, 0, 1)
class SolidVol(dict):
    def __missing__(self, d):
        r = defs.get(d); n = max(1, len(r.find('voxels'))) if r is not None else 1
        v = B.body_shell([(d, (0, 0, 0), I)], defs, T)['volume_depth_m3'] if r is not None else 0.0
        self[d] = max(0.0, v) / n; return self[d]
SV = SolidVol()
BOAT = ('propeller', 'rudder', 'water_jet')
for p in sys.argv[3:]:
    bodies = list(B.vehicle_bodies(p))
    names = {d for _, cs in bodies for d, _, _ in cs}
    kind = 'BOAT' if any(any(b in n for b in BOAT) for n in names) else 'other'
    bid, comps = max(bodies, key=lambda b: sum(mass.get(d, 0) for d, _, _ in b[1]))
    M = sum(mass.get(d, 0) for d, _, _ in comps); k = H.buoyancy_divisor(M)
    import time; t0 = time.time()
    r = B.game_buoyancy(comps, defs, T)
    V = r['buoyant_m3']; ratio = M * k / (1000 * V) if V else float('inf')
    print(f"{os.path.basename(os.path.dirname(p))} {kind:5s} M={M:8.0f} sealed={V:8.2f} m3 sealed_air={r.get('sealed_air_m3', 0):.1f} flooded={r.get('flooded_m3',0):.1f} open/sealed comps={r.get('open_compartments')}/{r.get('sealed_compartments')} ({time.time()-t0:.0f}s) ratio={ratio:.2f} {'floats' if ratio < 1 else 'SINKS'}")
