"""Float check with the ported shell walk (sw_shellwalk). Usage: shell_check.py defs_dir metadata_dir vehicle.xml..."""
import sys, os, time
here = os.path.dirname(os.path.abspath(__file__)); sys.path[:0] = [here, os.path.join(here, '..', '..', 'sim-reference')]
from defs_load import load
import sw_buoyancy as B, sw_hydro as H, sw_shellwalk as W
defs, _ = load(sys.argv[1]); tab = W.Tables(sys.argv[2])
mass = {k: float(v.get('mass', 0) or 0) for k, v in defs.items()}
for p in sys.argv[3:]:
    bodies = list(B.vehicle_bodies(p)); t = time.time()
    tot_M = sum(mass.get(d, 0) for _, cs in bodies for d, _, _ in cs)
    V = 0.0; rows = []
    for bid, comps in bodies:
        r = W.buoyancy(comps, defs, tab); V += r['buoyant_m3']; rows.append((bid, round(r['buoyant_m3'], 1)))
    k = H.buoyancy_divisor(tot_M); ratio = tot_M * k / (1000 * V) if V else float('inf')
    print(f"{os.path.basename(os.path.dirname(p))}: bodies={len(bodies)} M={tot_M:.0f} V={V:.1f} m3 ratio={ratio:.2f} {'floats' if ratio < 1 else 'SINKS'} ({time.time()-t:.0f}s) top bodies {sorted(rows, key=lambda x: -x[1])[:3]}")
