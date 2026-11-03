"""End-to-end offline evaluation of a vehicle with the recovered game models (no game required).

mass/COM (definitions) → hydrostatic draft (envelope buoyancy, RE-0004) → steady water drag vs speed (RE-0003 face
bins) → propeller thrust (RE-0007) → top-speed estimate. Y up, +Z forward (RE-0002). Static flooding is not modelled
(see RE-0004 §4). Hull assumed sealed.
"""
from __future__ import annotations
import math, collections
import sw_buoyancy as B, sw_hydro as H, sw_engine as E

VOX = 0.25


def mass_properties(components, defs):
    vox_cache = {}
    M = 0.0; m = [0.0, 0.0, 0.0]
    for d, vp, R in components:
        r = defs.get(d)
        if r is None:
            continue
        mass = float(r.get('mass', 0) or 0)
        if d not in vox_cache:
            vs = [tuple(int(float(p.get(a, 0) or 0)) for a in 'xyz') for p in (v.find('position') for v in r.find('voxels')) if p is not None]
            vox_cache[d] = vs or [(0, 0, 0)]
        vs = vox_cache[d]
        for q in vs:
            w = B.rot_col(R, q)
            for i in range(3):
                m[i] += mass / len(vs) * (vp[i] + w[i]) * VOX
        M += mass
    return M, tuple(x / M for x in m) if M else (0, 0, 0)


def hull_polygons(components, defs, tables):
    """Exterior buoyancy polygons (metres) with outward unit normal and area."""
    polys = []
    for d, vp, R in components:
        r = defs.get(d)
        if r is None or r.find('buoyancy_surfaces') is None:
            continue
        for s in r.find('buoyancy_surfaces'):
            p = s.find('position')
            sp = tuple(int(float(p.get(a, 0) or 0)) for a in 'xyz') if p is not None else (0, 0, 0)
            wpos = B.rot_col(R, sp); centre = tuple(2 * (vp[i] + wpos[i]) for i in range(3))
            for t in tables.types_for(s.get('orientation', '0'), s.get('rotation', '0'), s.get('shape', '0')):
                for tt in (tables.quarters[t] if t < 6 else [t]):
                    polys.append([tuple(centre[i] + q[i] for i in range(3)) for q in (B.rot_col(R, v) for v in tables.poly[tt])])
    groups = collections.defaultdict(list)
    for p in polys:
        groups[frozenset(p)].append(p)
    out = []
    for ps in groups.values():
        fwd = [p for p in ps if B._winding_sign(p, ps[0]) > 0]; rev = [p for p in ps if B._winding_sign(p, ps[0]) < 0]
        k = min(len(fwd), len(rev))
        for p in fwd[k:] + rev[k:]:
            n = B._inward_normal(p); L = math.sqrt(sum(c * c for c in n))
            if L == 0:
                continue
            area = 0.5 * L * B.HALF ** 2
            c = tuple(sum(v[i] for v in p) / len(p) * B.HALF for i in range(3))
            out.append((c, tuple(-x / L for x in n), area))       # outward normal
    return out


def submerged_volume(polys_halfunits_env_columns, waterline):
    cols, = polys_halfunits_env_columns
    return sum(length for (y, length) in cols if y < waterline)


def envelope_columns(components, defs, tables, sub=2):
    """Per-sub-column (centre y in metres, inside length in metres³ per column) from the envelope z-scan."""
    import sw_buoyancy as BB
    shell = BB.body_shell  # reuse polygon generation path
    polys = []
    for d, vp, R in components:
        r = defs.get(d)
        if r is None or r.find('buoyancy_surfaces') is None:
            continue
        for s in r.find('buoyancy_surfaces'):
            p = s.find('position')
            sp = tuple(int(float(p.get(a, 0) or 0)) for a in 'xyz') if p is not None else (0, 0, 0)
            wpos = B.rot_col(R, sp); centre = tuple(2 * (vp[i] + wpos[i]) for i in range(3))
            for t in tables.types_for(s.get('orientation', '0'), s.get('rotation', '0'), s.get('shape', '0')):
                for tt in (tables.quarters[t] if t < 6 else [t]):
                    polys.append([tuple(centre[i] + q[i] for i in range(3)) for q in (B.rot_col(R, v) for v in tables.poly[tt])])
    cols = collections.defaultdict(list); zmax = -1e9
    for p in polys:
        n = B._inward_normal(p); zmax = max(zmax, max(v[2] for v in p))
        if n[2] == 0:
            continue
        xs = [v[0] for v in p]; ys = [v[1] for v in p]
        for sx in range(min(xs) * sub, max(xs) * sub):
            for sy in range(min(ys) * sub, max(ys) * sub):
                px, py = (sx + 0.5) / sub, (sy + 0.5) / sub
                if B._point_in_poly_xy(px, py, p):
                    a = p[0]; z = a[2] - (n[0] * (px - a[0]) + n[1] * (py - a[1])) / n[2]
                    cols[(px, py)].append((z, 1 if n[2] > 0 else -1))
    out = []
    for (px, py), hits in cols.items():
        ins = [z for z, s in hits if s > 0]; outs = [z for z, s in hits if s < 0]
        if not ins:
            continue
        zl = max(outs) if outs and max(outs) > min(ins) else zmax + 2
        out.append((py * B.HALF, (zl - min(ins)) * B.HALF * (B.HALF / sub) ** 2))
    return out


def solve_draft(cols, mass):
    k = H.buoyancy_divisor(mass)
    need = mass * k / H.K.water_density          # m³ to submerge (from M g = ρ g V / k)
    ys = sorted(cols); lo = min(c[0] for c in cols) - 0.25; hi = max(c[0] for c in cols) + 0.25
    vol = lambda wl: sum(v for y, v in cols if y < wl)
    if vol(hi) < need:
        return None, need, vol(hi)
    for _ in range(60):
        mid = (lo + hi) / 2
        lo, hi = (mid, hi) if vol(mid) < need else (lo, mid)
    return hi, need, vol(hi)


def drag_at(polys, waterline, speed, mass):
    """Steady forward drag (N) at +Z speed with the hull at the given waterline (no impulse cap: steady state)."""
    F = 0.0
    k = H.K
    for c, n, area in polys:
        if c[1] >= waterline:
            continue
        u = (0.0, 0.0, speed); s = speed
        uh = (0.0, 0.0, 1.0); cos = H._dot(uh, n)
        visc = k.viscous_resistance_factor * k.water_density * (20.0 / (s * 1.0 * 1e6 + 10.0) + 1.5) * 0.00075 * area * H.water_drag_scale(mass) * s * s
        F += visc
        if cos > 0:
            F += cos * (cos ** k.falloff_power_pressure) * (k.drag_factor_linear_pressure * s + k.drag_factor_quadratic_pressure * s * s) * area
        elif cos < 0:
            F += (-cos) * ((-cos) ** k.falloff_power_suction) * (k.drag_factor_linear_suction * s + k.drag_factor_quadratic_suction * s * s) * area
    return F
