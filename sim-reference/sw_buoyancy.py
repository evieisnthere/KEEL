"""Vehicle-level buoyancy shell and enclosed volume, a game-faithful approximation of RE-0004.

Pipeline (per body):
  1. For every component, take its definition's buoyancy_surfaces, convert (orientation, rotation, shape) to the game's
     internal surface types (surface_conversion.json, E1), then place each type's polygon in the body grid using the
     component's vp and r (column-major convention, E1-inferred).
  2. Full faces are split into their four quarter triangles (mirrors sub_1402f5450 normalisation), and all edges are split
     into unit half-voxel segments so that polygons from different shapes share edges exactly.
  3. Coincident back-to-back polygons cancel (an internal wall between two blocks).
  4. The shell is closed iff every directed edge segment has a reverse partner. Enclosed volume uses the divergence theorem.

The game's own builder walks edge loops from +Z-facing seeds. This module computes the same closed region where the
shell is valid. It is not yet bit-exact for invalid or partially open shells. Units: positions in half-voxel lattice units
(1 unit = 0.125 m).
"""
from __future__ import annotations
import json, math, os, collections
import xml.etree.ElementTree as ET
from fractions import Fraction

HALF = 0.125  # metres per half-voxel unit


def _load_json(md, name):
    return json.load(open(os.path.join(md, name)))


class SurfaceTables:
    def __init__(self, metadata_dir):
        self.conv = _load_json(metadata_dir, 'surface_conversion.json')['conv']
        shapes = _load_json(metadata_dir, 'surface_shapes.json')['shapes']
        self.poly = {s['shape']: [tuple(v) for v in s['vertices_halfvox']] for s in shapes}
        # quarter triangles per full face: the 4 triangle types sharing the face's plane & orientation
        self.quarters = {}
        for f in range(6):
            n = shapes[f]['normal']
            self.quarters[f] = [s['shape'] for s in shapes[6:30] if s['normal'] == n]

    def types_for(self, orientation, rotation, shape):
        c = self.conv.get(f"{orientation},{rotation},{shape}")
        return c['types'] if c else []


def rot_col(R, v):
    """Component rotation, column-major: world = R^T v (E1-inferred from voxel overlap minimisation)."""
    return tuple(R[0 + i] * v[0] + R[3 + i] * v[1] + R[6 + i] * v[2] for i in range(3))


def _split_edges(poly):
    out = []
    n = len(poly)
    for k in range(n):
        a, b = poly[k], poly[(k + 1) % n]
        d = [b[i] - a[i] for i in range(3)]
        g = math.gcd(math.gcd(abs(d[0]), abs(d[1])), abs(d[2])) or 1
        step = [d[i] // g for i in range(3)]
        for s in range(g):
            out.append(tuple(a[i] + step[i] * s for i in range(3)))
    return out


def _poly_key(poly):
    return frozenset(poly)


def body_shell(components, defs, tables: SurfaceTables):
    """components: iterable of (definition_name, vp(x,y,z), r[9]). Returns dict with volume, closure stats."""
    polys = []
    for d, vp, R in components:
        r = defs.get(d)
        if r is None:
            continue
        bs = r.find('buoyancy_surfaces')
        if bs is None:
            continue
        for s in bs:
            p = s.find('position')
            sp = tuple(int(float(p.get(a, 0) or 0)) for a in 'xyz') if p is not None else (0, 0, 0)
            types = tables.types_for(s.get('orientation', '0'), s.get('rotation', '0'), s.get('shape', '0'))
            expanded = []
            for t in types:
                expanded += tables.quarters[t] if t < 6 else [t]
            wpos = rot_col(R, sp)
            centre = tuple(2 * (vp[i] + wpos[i]) for i in range(3))
            for t in expanded:
                poly = [tuple(centre[i] + q[i] for i in range(3)) for q in (rot_col(R, v) for v in tables.poly[t])]
                polys.append(poly)
    # cancel back-to-back coincident polygons (same vertex set, opposite winding)
    groups = collections.defaultdict(list)
    for p in polys:
        groups[_poly_key(p)].append(p)
    kept = []
    cancelled = 0
    for key, ps in groups.items():
        fwd = [p for p in ps if _winding_sign(p, ps[0]) > 0]
        rev = [p for p in ps if _winding_sign(p, ps[0]) < 0]
        m = min(len(fwd), len(rev)); cancelled += 2 * m
        kept += fwd[m:] + rev[m:]
    # closure: directed unit edges must pair up
    edges = collections.Counter()
    for p in kept:
        q = _split_edges(p)
        for k in range(len(q)):
            edges[(q[k], q[(k + 1) % len(q)])] += 1
    unmatched = 0
    for (a, b), n in edges.items():
        unmatched += max(0, n - edges.get((b, a), 0))
    # volume (polygon normals from game edge order point into the solid; flip sign)
    vol6 = Fraction(0)
    for p in kept:
        for k in range(1, len(p) - 1):
            a, b, c = p[0], p[k], p[k + 1]
            vol6 += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]))
    volume = -float(vol6) / 6.0 * HALF ** 3
    zvol = zscan_volume(kept)
    env = zscan_volume(kept, envelope=True)
    axis_vols = [zscan_volume(kept, axis=a) for a in (0, 1)]
    return {"polygons": len(kept), "cancelled": cancelled, "unmatched_edges": unmatched,
            "edge_segments": sum(edges.values()), "volume_closed_m3": volume, "volume_depth_m3": zvol, "volume_m3": env, "volume_xscan_m3": axis_vols[0], "volume_yscan_m3": axis_vols[1]}


def _inward_normal(q):
    nx = ny = nz = 0
    for a, b in zip(q, q[1:] + q[:1]):
        nx += (a[1] - b[1]) * (a[2] + b[2]); ny += (a[2] - b[2]) * (a[0] + b[0]); nz += (a[0] - b[0]) * (a[1] + b[1])
    return nx, ny, nz


def _point_in_poly_xy(px, py, poly):
    inside = False
    n = len(poly)
    for k in range(n):
        x1, y1 = poly[k][0], poly[k][1]; x2, y2 = poly[(k + 1) % n][0], poly[(k + 1) % n][1]
        if (y1 > py) != (y2 > py):
            xi = x1 + (py - y1) * (x2 - x1) / (y2 - y1)
            if px < xi: inside = not inside
    return inside


def zscan_volume(polys, sub=2, axis=2, envelope=False):
    """envelope=True (default model): inside from first entry to last exit per column (outer envelope; RE-0004 §2b)."""
    """Game-style buoyancy volume (RE-0004 §2b): scan +Z columns, toggle inside on Z-facing surfaces.
    Sub-sampled at 4 points per voxel column (offsets +-0.5 half-voxel units) to avoid hitting quarter-triangle seams.
    A column still 'inside' at the end is extended to the body's z_max + 1 voxel (the game's loop bound)."""
    cols = collections.defaultdict(list)
    zmax = -10**9
    perm = {2: (0, 1, 2), 1: (2, 0, 1), 0: (1, 2, 0)}[axis]
    polys = [[(v[perm[0]], v[perm[1]], v[perm[2]]) for v in p] for p in polys]
    for p in polys:
        n = _inward_normal(p)
        zmax = max(zmax, max(v[2] for v in p))
        if n[2] == 0:
            continue
        xs = [v[0] for v in p]; ys = [v[1] for v in p]
        for sx in range(min(xs) * sub, max(xs) * sub):
            for sy in range(min(ys) * sub, max(ys) * sub):
                px, py = (sx + 0.5) / sub, (sy + 0.5) / sub
                if not _point_in_poly_xy(px, py, p):
                    continue
                a = p[0]
                z = a[2] - (n[0] * (px - a[0]) + n[1] * (py - a[1])) / n[2]
                cols[(sx, sy)].append((z, 1 if n[2] > 0 else -1))
    zend = zmax + 2  # z_max + 1 voxel, in half-voxel units
    total = 0.0
    for key, hits in cols.items():
        hits.sort()
        if envelope:
            ins = [z for z, sg in hits if sg > 0]; outs = [z for z, sg in hits if sg < 0]
            if ins:
                zlast = max(outs) if outs and max(outs) > min(ins) else zend
                total += zlast - min(ins)
            continue
        depth = 0; zin = None
        for z, sgn in hits:
            prev = depth; depth += sgn
            if prev <= 0 < depth: zin = z
            elif prev > 0 >= depth and zin is not None: total += z - zin; zin = None
        if depth > 0 and zin is not None:
            total += zend - zin
    return total * HALF ** 3 / (sub * sub)


def _winding_sign(p, ref):
    """+1 if p has the same orientation as ref (same normal direction), -1 otherwise."""
    def normal(q):
        nx = ny = nz = 0
        for a, b in zip(q, q[1:] + q[:1]):
            nx += (a[1] - b[1]) * (a[2] + b[2]); ny += (a[2] - b[2]) * (a[0] + b[0]); nz += (a[0] - b[0]) * (a[1] + b[1])
        return (nx, ny, nz)
    n1, n2 = normal(p), normal(ref)
    return 1 if sum(x * y for x, y in zip(n1, n2)) > 0 else -1


def vehicle_bodies(vehicle_xml):
    """Yield (body_id, [(definition, vp, r), ...]) from a vehicle.xml (data_version 3)."""
    root = ET.parse(vehicle_xml).getroot()
    for b in root.iter('body'):
        comps = []
        for c in b.iter('c'):
            o = c.find('o')
            if o is None or c.get('d', '01_block') is None:
                continue
            vp = o.find('vp')
            pos = tuple(int(vp.get(a, 0)) for a in 'xyz') if vp is not None else (0, 0, 0)
            R = [int(x) for x in o.get('r', '1,0,0,0,1,0,0,0,1').split(',')]
            comps.append((c.get('d', '01_block'), pos, R))
        yield b.get('unique_id'), comps


# ---------------------------------------------------------------------------------------------
# Sealed-volume model (watertightness). E1-inferred structure + user-reported game behaviour:
# a compartment connected to the outside floods (open-top boats sink); faces fully covered by
# buoyancy surfaces block water (door frames have six sealed faces).
# ---------------------------------------------------------------------------------------------

def _face_key(poly):
    """If a polygon lies on a voxel-face plane, return (axis, plane_coord, cell_a, cell_b), else None.
    Half-voxel units: voxel centres are at even coordinates and face planes at odd ones."""
    for ax in range(3):
        c = {v[ax] for v in poly}
        if len(c) == 1:
            p = c.pop()
            if p % 2 == 0:
                return None
            # centroid in the other two axes picks the voxel column
            o = [i for i in range(3) if i != ax]
            cx = sum(v[o[0]] for v in poly) / len(poly); cy = sum(v[o[1]] for v in poly) / len(poly)
            vx = int(round((cx - 0) / 2.0 - 0.0001 * 0)) if False else None
            a = [0, 0, 0]; a[ax] = (p - 1) // 2
            a[o[0]] = int((cx + 1) // 2); a[o[1]] = int((cy + 1) // 2)
            b = list(a); b[ax] += 1
            return ax, tuple(a), tuple(b)
    return None


def _poly_area_half2(poly):
    nx = ny = nz = 0
    for a, b in zip(poly, poly[1:] + poly[:1]):
        nx += (a[1] - b[1]) * (a[2] + b[2]); ny += (a[2] - b[2]) * (a[0] + b[0]); nz += (a[0] - b[0]) * (a[1] + b[1])
    return 0.5 * (nx * nx + ny * ny + nz * nz) ** 0.5   # in half-voxel units^2; a full voxel face = 4


def sealed_volume(components, defs, tables: SurfaceTables, solid_volume_by_def=None):
    """Returns dict: sealed_m3 (air + solid not reachable from outside), reachable solid, voxel counts.
    solid_volume_by_def: {definition: m3 of solid per voxel}; defaults to a full voxel for every occupied voxel."""
    import collections as C
    cover = C.defaultdict(float)          # (axis, cell_a, cell_b) -> covered area (half units^2)
    occupied = {}                          # voxel -> solid m3
    vox_cache = {}
    for d, vp, R in components:
        r = defs.get(d)
        if r is None:
            continue
        if d not in vox_cache:
            vs = [tuple(int(float(p.get(a, 0) or 0)) for a in 'xyz') for p in (v.find('position') for v in r.find('voxels')) if p is not None]
            vox_cache[d] = vs or [(0, 0, 0)]
        per = (solid_volume_by_def or {}).get(d, 0.25 ** 3)
        for q in vox_cache[d]:
            w = rot_col(R, q); cell = (vp[0] + w[0], vp[1] + w[1], vp[2] + w[2])
            occupied[cell] = min(0.25 ** 3, occupied.get(cell, 0.0) + per)
        bs = r.find('buoyancy_surfaces')
        if bs is None:
            continue
        for s in bs:
            p = s.find('position')
            sp = tuple(int(float(p.get(a, 0) or 0)) for a in 'xyz') if p is not None else (0, 0, 0)
            wpos = rot_col(R, sp); centre = tuple(2 * (vp[i] + wpos[i]) for i in range(3))
            for t in tables.types_for(s.get('orientation', '0'), s.get('rotation', '0'), s.get('shape', '0')):
                poly = [tuple(centre[i] + q[i] for i in range(3)) for q in (rot_col(R, v) for v in tables.poly[t])]
                k = _face_key(poly)
                if k:
                    cover[k] += _poly_area_half2(poly)
    blocked = {k for k, a in cover.items() if a >= 4.0 - 1e-6}
    if not occupied:
        return {"sealed_m3": 0.0}
    lo = [min(c[i] for c in occupied) - 1 for i in range(3)]; hi = [max(c[i] for c in occupied) + 1 for i in range(3)]
    start = tuple(lo); seen = {start}; stack = [start]
    while stack:
        c = stack.pop()
        for ax in range(3):
            for dlt in (-1, 1):
                n = list(c); n[ax] += dlt; n = tuple(n)
                if not all(lo[i] <= n[i] <= hi[i] for i in range(3)) or n in seen:
                    continue
                key = (ax, c, n) if dlt == 1 else (ax, n, c)
                if key in blocked:
                    continue
                seen.add(n); stack.append(n)
    total_cells = (hi[0] - lo[0] + 1) * (hi[1] - lo[1] + 1) * (hi[2] - lo[2] + 1)
    sealed_cells = total_cells - len(seen)
    reachable_solid = sum(v for c, v in occupied.items() if c in seen)
    sealed_m3 = sealed_cells * 0.25 ** 3 + reachable_solid
    sealed_air = sum(1 for x in range(lo[0], hi[0] + 1) for y in range(lo[1], hi[1] + 1) for z in range(lo[2], hi[2] + 1)
                     if (x, y, z) not in seen and (x, y, z) not in occupied)
    return {"sealed_m3": sealed_m3, "sealed_air_voxels": sealed_air, "flooded_or_outside_voxels": len(seen),
            "occupied_voxels": len(occupied), "blocked_faces": len(blocked)}


def sealed_volume_fine(components, defs, tables: SurfaceTables, sub=4):
    """Watertight (non-flooding) volume via sub-voxel rasterisation.
    Every buoyancy surface is rasterised as a 1-cell wall on a grid of `sub` cells per voxel (0.25/sub m). Free cells
    connected to the outside (6-connectivity) are flooded or outside, and everything else is buoyant (solid or sealed air).
    Wall thickness adds about area × cell/2 of volume (< 1-2 % on typical hulls)."""
    import numpy as np
    from scipy import ndimage
    polys = []
    for d, vp, R in components:
        r = defs.get(d)
        if r is None:
            continue
        bs = r.find('buoyancy_surfaces')
        if bs is None:
            continue
        for s in bs:
            p = s.find('position')
            sp = tuple(int(float(p.get(a, 0) or 0)) for a in 'xyz') if p is not None else (0, 0, 0)
            wpos = rot_col(R, sp); centre = tuple(2 * (vp[i] + wpos[i]) for i in range(3))
            for t in tables.types_for(s.get('orientation', '0'), s.get('rotation', '0'), s.get('shape', '0')):
                polys.append([tuple(centre[i] + q[i] for i in range(3)) for q in (rot_col(R, v) for v in tables.poly[t])])
    if not polys:
        return {"sealed_m3": 0.0}
    # sample points on each polygon (fan triangulation, barycentric grid); coords in half-voxel units
    n = 2 * sub  # samples per half unit ~ 4 per cell
    u = np.linspace(0, 1, 2 * n + 1)
    U, V = np.meshgrid(u, u); m = (U + V) <= 1.0 + 1e-9
    bu, bv = U[m], V[m]
    pts = []
    for poly in polys:
        P = np.asarray(poly, float)
        for k in range(1, len(P) - 1):
            a, b, c = P[0], P[k], P[k + 1]
            pts.append(a + np.outer(bu, b - a) + np.outer(bv, c - a))
    pts = np.concatenate(pts)
    cell = 2.0 / sub                       # cell size in half-voxel units
    idx = np.floor((pts + cell / 2) / cell + 1e-9).astype(np.int64)   # cells centred on face planes
    lo = idx.min(0) - 2; hi = idx.max(0) + 2
    shape = tuple(int(x) for x in (hi - lo + 1))
    wall = np.zeros(shape, bool)
    j = idx - lo
    wall[j[:, 0], j[:, 1], j[:, 2]] = True
    lab, nlab = ndimage.label(~wall)       # default structure = 6-connectivity
    outside = lab == lab[0, 0, 0]
    cell_m3 = (0.25 / sub) ** 3
    # walls straddle the true surface: a wall cell touching the outside is half buoyant
    touch = np.zeros_like(wall)
    for ax in range(3):
        for sh in (1, -1):
            touch |= np.roll(outside, sh, axis=ax)
    wall_out = wall & touch
    interior_air = int((~wall & ~outside).sum())
    sealed = int(wall.sum()) - 0.5 * int(wall_out.sum()) + interior_air
    return {"sealed_m3": sealed * cell_m3, "sealed_air_m3": interior_air * cell_m3,
            "wall_m3": int(wall.sum()) * cell_m3, "grid": shape, "compartments": int(nlab - 1)}


def game_buoyancy(components, defs, tables: SurfaceTables):
    """Game-structured static buoyancy (RE-0004 §2b + user-reported flooding behaviour):
      inside      = per (x, y) voxel column, from the first to the last occupied voxel along z (envelope z-scan)
      compartment = 6-connected empty inside voxels
      open        = compartment has a 6-neighbour that is empty and not inside (outside air/water), so it floods
      buoyant     = exact envelope volume (polygon z-scan) − volume of open compartments
    Door frames, windows etc. count as occupied voxels and therefore seal (matches in-game door-frame trick)."""
    import numpy as np
    from scipy import ndimage
    occ = set(); vcache = {}
    for d, vp, R in components:
        r = defs.get(d)
        if r is None:
            continue
        if d not in vcache:
            vs = [tuple(int(float(p.get(a, 0) or 0)) for a in 'xyz') for p in (v.find('position') for v in r.find('voxels')) if p is not None]
            vcache[d] = vs or [(0, 0, 0)]
        for q in vcache[d]:
            w = rot_col(R, q); occ.add((vp[0] + w[0], vp[1] + w[1], vp[2] + w[2]))
    if not occ:
        return {"buoyant_m3": 0.0}
    O = np.array(sorted(occ)); lo = O.min(0) - 1; hi = O.max(0) + 1
    shape = tuple(int(x) for x in hi - lo + 1)
    occg = np.zeros(shape, bool); j = O - lo; occg[j[:, 0], j[:, 1], j[:, 2]] = True
    zs = np.arange(shape[2])
    anyz = occg.any(2)
    first = np.where(anyz, occg.argmax(2), shape[2])
    last = np.where(anyz, shape[2] - 1 - occg[:, :, ::-1].argmax(2), -1)
    inside = (zs[None, None, :] >= first[:, :, None]) & (zs[None, None, :] <= last[:, :, None])
    empty_inside = inside & ~occg
    lab, n = ndimage.label(empty_inside)
    outside_air = ~inside & ~occg
    touch = np.zeros(shape, bool)
    for ax in range(3):
        for sh in (1, -1):
            touch |= np.roll(outside_air, sh, axis=ax)
    open_labels = np.unique(lab[touch & empty_inside]); open_labels = open_labels[open_labels > 0]
    open_mask = np.isin(lab, open_labels)
    vox = 0.25 ** 3
    env = body_shell(components, defs, tables)['volume_m3']   # exact envelope z-scan
    open_m3 = float(open_mask.sum()) * vox
    return {"buoyant_m3": env - open_m3, "envelope_m3": env, "open_compartments": int(len(open_labels)),
            "sealed_compartments": int(n - len(open_labels)), "flooded_m3": open_m3,
            "sealed_air_m3": float((empty_inside & ~open_mask).sum()) * vox}
