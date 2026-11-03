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
    axis_vols = [zscan_volume(kept, axis=a) for a in (0, 1)]
    return {"polygons": len(kept), "cancelled": cancelled, "unmatched_edges": unmatched,
            "edge_segments": sum(edges.values()), "volume_closed_m3": volume, "volume_m3": zvol, "volume_xscan_m3": axis_vols[0], "volume_yscan_m3": axis_vols[1]}


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


def zscan_volume(polys, sub=2, axis=2):
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
