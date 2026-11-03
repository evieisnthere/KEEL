"""Offline performance report for a vehicle (main body). Usage: offline_report.py defs_dir metadata_dir vehicle.xml"""
import sys, os, time
here = os.path.dirname(os.path.abspath(__file__))
sys.path[:0] = [os.path.join(here, 're'), os.path.join(here, '..', 'sim-reference')]
from defs_load import load
import sw_buoyancy as B, sw_offline as O, sw_engine as E, sw_hydro as H
defs, _ = load(sys.argv[1]); T = B.SurfaceTables(sys.argv[2])
bodies = list(B.vehicle_bodies(sys.argv[3]))
mass = lambda cs: sum(float(defs[d].get('mass', 0) or 0) for d, _, _ in cs if d in defs)
bid, comps = max(bodies, key=lambda b: mass(b[1]))
allc = [c for _, cs in bodies for c in cs]
M, com = O.mass_properties(allc, defs)
t = time.time(); cols = O.envelope_columns(comps, defs, T); polys = O.hull_polygons(comps, defs, T)
wl, need, got = O.solve_draft(cols, M)
ymin = min(c[0][1] for c in polys); ymax = max(c[0][1] for c in polys)
zs = [c[0][2] for c in polys]
print(f"vehicle {os.path.basename(os.path.dirname(sys.argv[3]))}: {len(bodies)} bodies, {len(allc)} components, mass {M:.0f} (game units), COM {tuple(round(x,2) for x in com)} m")
print(f"hull body {bid}: length {max(zs)-min(zs):.2f} m, height {ymax-ymin:.2f} m, envelope {sum(v for _,v in cols):.1f} m³, k_buoy {H.buoyancy_divisor(M):.2f}")
if wl is None:
    print(f"  SINKS (static): needs {need:.1f} m³ submerged, hull envelope only {got:.1f} m³"); sys.exit()
print(f"  floats: waterline y = {wl:.2f} m, draft {wl-ymin:.2f} m, freeboard {ymax-wl:.2f} m, displaced {need:.1f} m³")
props = [(d, vp) for d, vp, _ in allc if d.startswith('propeller') or d == 'large_propeller']
print(f"  propellers: {[p[0] for p in props]}")
print("  speed   drag(N)   thrust needed per prop")
for v in (1, 2, 4, 6, 8, 10, 12):
    D = O.drag_at(polys, wl, v, M)
    print(f"  {v:4.0f} m/s ({v*1.944:4.1f} kn) {D:10.0f} {D/max(1,len(props)):10.0f}")
for d, _ in props[:1]:
    r = defs[d]
    kw = dict(max_force=float(r.get('force_emitter_max_force')), rotation_speed=float(r.get('force_emitter_rotation_speed', 1) or 1),
              blade_length=float(r.get('force_emitter_blade_physics_length')), blade_efficiency=float(r.get('force_emitter_blade_efficiency')),
              efficiency=float(r.get('force_emitter_efficiency', 1) or 1))
    for w in (5, 10, 15, 20):
        _, thr, ab = E.propeller_tick(w, 1.0, **kw)
        D = lambda v: O.drag_at(polys, wl, v, M) - thr * len(props)
        lo, hi = 0.0, 40.0
        if D(hi) < 0: vmax = '>40'
        else:
            for _ in range(50):
                mid = (lo + hi) / 2; lo, hi = (mid, hi) if D(mid) < 0 else (lo, mid)
            vmax = f"{lo:.1f} m/s ({lo*1.944:.1f} kn)"
        print(f"  {d} at {w} RPS, pitch 1: thrust {thr:.0f} N each (shaft must resupply {ab:.2f} RPS/tick) -> top speed {vmax}")
print(f"  ({time.time()-t:.1f}s)")
