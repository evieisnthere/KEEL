import sys, os, collections
here = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, here); sys.path.insert(0, os.path.join(here, '..', '..', 'sim-reference'))
from defs_load import load
import sw_buoyancy as B, sw_hydro as H
defs, _ = load(sys.argv[1]); T = B.SurfaceTables(sys.argv[2])
mass = {k: float(v.get('mass', 0) or 0) for k, v in defs.items()}
BOAT = ('propeller', 'rudder', 'water_jet', 'jet_water'); LAND = ('train', 'wheel')
for p in sys.argv[3:]:
    bodies = list(B.vehicle_bodies(p))
    names = collections.Counter(d for _, cs in bodies for d, _, _ in cs)
    kind = 'BOAT' if any(any(b in n for b in BOAT) for n in names) else ('LAND' if any(any(l in n for l in LAND) for n in names) else '?')
    bid, comps = max(bodies, key=lambda b: sum(mass.get(d, 0) for d, _, _ in b[1]))
    M = sum(mass.get(d, 0) for d, _, _ in comps); k = H.buoyancy_divisor(M); r = B.body_shell(comps, defs, T)
    f = lambda V: round(M * k / (1000 * V), 2) if V > 0 else 'inf'
    print(f"{os.path.basename(os.path.dirname(p))} {kind:4s} M={round(M):6d} ratio depth={f(r['volume_m3'])} envelope={f(r['volume_envelope_m3'])}")
