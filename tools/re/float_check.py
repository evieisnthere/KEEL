"""Per-body enclosed buoyancy volume, closure and float ratio for vehicle files. Usage: float_check.py defs_dir metadata_dir vehicle.xml..."""
import sys, os
here = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, here); sys.path.insert(0, os.path.join(here, '..', '..', 'sim-reference'))
from defs_load import load
import sw_buoyancy as B, sw_hydro as H
defs, _ = load(sys.argv[1]); T = B.SurfaceTables(sys.argv[2])
mass = {k: float(v.get('mass', 0) or 0) for k, v in defs.items()}
for p in sys.argv[3:]:
    rows = []
    for bid, comps in B.vehicle_bodies(p):
        M = sum(mass.get(d, 0) for d, _, _ in comps)
        r = B.body_shell(comps, defs, T)
        k = H.buoyancy_divisor(M)
        ratio = (M * k / (1000 * r['volume_m3'])) if r['volume_m3'] > 0 else float('inf')
        rows.append((bid, len(comps), round(M), round(r['volume_m3'], 2), r['unmatched_edges'], round(ratio, 2)))
    tot_m = sum(x[2] for x in rows); main = max(rows, key=lambda x: x[2])
    print(f"{os.path.basename(os.path.dirname(p))}: bodies={len(rows)} mass={tot_m} main(body {main[0]}: comps={main[1]} M={main[2]} V={main[3]} open_edges={main[4]} float_ratio={main[5]})")
