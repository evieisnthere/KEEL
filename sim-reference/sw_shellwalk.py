"""Port of the game's buoyancy shell walk (RE-0004 §2/§5/§6), using the emulated static tables:
  surface_shapes.json     58 surface types: directed edges (edge ids) in half-voxel units
  surface_adjacency.json  per-edge ordered candidate lists (order, dx, dy, dz, neighbour edge id, neighbour type)
  surface_conversion.json definition surface (orientation, rotation, shape) -> surface types

Algorithm (E1 structure, details E1-inferred where marked):
  * every buoyancy surface is registered in its voxel *and* as its back-face twin (opposite type) on the other side
  * full faces are split into quarter triangles when a quarter triangle of the same face is present (sub_1402f5450)
  * outer walk: a loop of directed boundary edges grows by repeatedly expanding its head edge into the first present
    candidate surface (sub_140488260); already-visited candidates cancel edges (loops split / merge)
  * the outer shell's surfaces are the ones the z-scan sees; compartments are walked afterwards (holes = touching the outer shell)
"""
from __future__ import annotations
import json, os, collections


class Tables:
    def __init__(self, md):
        sh = json.load(open(os.path.join(md, 'surface_shapes.json')))['shapes']
        adj = json.load(open(os.path.join(md, 'surface_adjacency.json')))
        self.conv = json.load(open(os.path.join(md, 'surface_conversion.json')))['conv']
        self.verts = {s['shape']: [tuple(v) for v in s['vertices_halfvox']] for s in sh}
        self.edges = {s['shape']: s['edge_ids'] for s in sh}           # edge ids in polygon order
        self.edge_type = {}
        for t, ids in self.edges.items():
            for i in ids:
                self.edge_type[i] = t
        self.adj = {int(k): v for k, v in adj['adjacency'].items() if v}
        self.quarters = {}
        for f in range(6):
            n = sh[f]['normal']
            self.quarters[f] = [s['shape'] for s in sh[6:30] if s['normal'] == n]
        # opposite (back-face) twin: same vertex set (after offset), reversed winding
        self.twin = {}
        for t, vs in self.verts.items():
            target = frozenset(vs)
            for off in [(0, 0, 0)] + [(dx, dy, dz) for dx in (-1, 0, 1) for dy in (-1, 0, 1) for dz in (-1, 0, 1) if (dx, dy, dz) != (0, 0, 0)]:
                hit = None
                for t2, vs2 in self.verts.items():
                    if t2 == t:
                        continue
                    if frozenset(tuple(v[i] + 2 * off[i] for i in range(3)) for v in vs2) == target:
                        hit = t2; break
                if hit is not None:
                    self.twin[t] = (hit, off); break


def rot_col(R, v):
    return tuple(R[0 + i] * v[0] + R[3 + i] * v[1] + R[6 + i] * v[2] for i in range(3))


def _match_type(tab, verts):
    key = frozenset(verts)
    for t, vs in tab.verts.items():
        if frozenset(vs) == key:
            return t
    return None


def build_surface_map(components, defs, tab: Tables):
    """voxel -> set of surface types (both sides, normalised)."""
    present = collections.defaultdict(set)
    cache = {}
    for d, vp, R in components:
        r = defs.get(d)
        if r is None or r.find('buoyancy_surfaces') is None:
            continue
        for s in r.find('buoyancy_surfaces'):
            p = s.find('position')
            sp = tuple(int(float(p.get(a, 0) or 0)) for a in 'xyz') if p is not None else (0, 0, 0)
            c = tab.conv.get(f"{s.get('orientation', '0')},{s.get('rotation', '0')},{s.get('shape', '0')}")
            if not c:
                continue
            w = rot_col(R, sp); vox = tuple(vp[i] + w[i] for i in range(3))
            for t in c['types']:
                key = (t, tuple(R))
                if key not in cache:                     # rotate the type's polygon and re-identify it
                    cache[key] = _match_type(tab, [rot_col(R, v) for v in tab.verts[t]])
                tt = cache[key]
                if tt is None:
                    continue
                present[vox].add(tt)
                tw, off = tab.twin.get(tt, (None, None))
                if tw is not None:
                    present[tuple(vox[i] + off[i] for i in range(3))].add(tw)
    for v, ts in present.items():                        # normalisation (sub_1402f5450)
        for f in range(6):
            if f in ts and any(q in ts for q in tab.quarters[f]):
                ts.discard(f); ts.update(tab.quarters[f])
    return present


def walk_shell(seed_vox, seed_type, present, visited, tab: Tables, owner=None, shell_id=0, max_steps=2_000_000, parent=None):
    """Grow one shell from a seed surface. Returns (surfaces, touched_foreign) where touched_foreign is set of shell ids
    of already-visited surfaces reached through a candidate (used for hole detection of compartments)."""
    loops = [collections.deque((e, seed_vox) for e in tab.edges[seed_type])]
    visited.add((seed_vox, seed_type)); surfaces = [(seed_vox, seed_type)]
    if owner is not None:
        owner[(seed_vox, seed_type)] = shell_id
    touched = set(); steps = 0
    while loops and steps < max_steps:
        steps += 1
        loop = loops[-1]
        if not loop:
            loops.pop(); continue
        eid, vox = loop[0]
        cands = tab.adj.get(eid)
        chosen = None
        for order, dx, dy, dz, neid, ntype in (cands or []):
            nv = (vox[0] + dx, vox[1] + dy, vox[2] + dz)
            if ntype in present.get(nv, ()):
                chosen = (nv, ntype, neid); break
        if chosen is None:                                   # no continuation at all: drop the edge (open boundary)
            loop.popleft(); continue
        nv, ntype, neid = chosen
        if (nv, ntype) not in visited:                       # new surface: splice its remaining edges in place of this one
            visited.add((nv, ntype)); surfaces.append((nv, ntype))
            if parent is not None:
                parent[(nv, ntype)] = (vox, tab.edge_type[eid], eid)
            if owner is not None:
                owner[(nv, ntype)] = shell_id
            ids = tab.edges[ntype]
            k = ids.index(neid) if neid in ids else 0
            new = [(ids[(k + j) % len(ids)], nv) for j in range(1, len(ids))]
            loop.popleft()
            for item in reversed(new):
                loop.appendleft(item)
        else:                                                # closing: cancel this edge with its partner if present
            if owner is not None and owner.get((nv, ntype)) not in (None, shell_id):
                touched.add(owner[(nv, ntype)])
            partner = (neid, nv)
            loop.popleft()
            removed = False
            for lp in loops:
                try:
                    lp.remove(partner); removed = True; break
                except ValueError:
                    pass
    return surfaces, touched


SEED_TYPES = {5, 11, 17, 23, 29, 30, 34, 35, 38, 42, 43, 48, 49, 52, 53, 54, 55}   # 0x140486db0 switch (E1)
HALF = 0.125


def _signed_volume(surfs, tab):
    v6 = 0.0
    for vox, t in surfs:
        p = [tuple(2 * vox[i] + q[i] for i in range(3)) for q in tab.verts[t]]
        for k in range(1, len(p) - 1):
            a, b, c = p[0], p[k], p[k + 1]
            v6 += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])
    return -v6 / 6.0 * HALF ** 3


def buoyancy(components, defs, tab: Tables, sub=2):
    """Game-structured buoyancy volume: outer shells (walked) → parity z-scan; interior cavities counted unless their
    surfaces belong to an outer shell (open to the outside)."""
    import sw_buoyancy as B
    present = build_surface_map(components, defs, tab)
    visited, owner = set(), {}
    shells = []
    seeds = sorted(((v, t) for v, ts in present.items() for t in ts if t in SEED_TYPES), key=lambda x: (x[0][2], x[0][1], x[0][0], x[1]))
    for v, t in seeds:
        if (v, t) in visited:
            continue
        surfs, touched = walk_shell(v, t, present, visited, tab, owner, shell_id=len(shells))
        shells.append(dict(surfs=surfs, volume=_signed_volume(surfs, tab), touched=touched))
    # surfaces never reached from a seed (e.g. vertical-only shells) get their own walks too
    for v, ts in present.items():
        for t in ts:
            if (v, t) not in visited:
                surfs, touched = walk_shell(v, t, present, visited, tab, owner, shell_id=len(shells))
                shells.append(dict(surfs=surfs, volume=_signed_volume(surfs, tab), touched=touched))
    outer = [s for s in shells if s['volume'] > 1e-9]
    cavities = [s for s in shells if s['volume'] < -1e-9]
    polys = [[tuple(2 * v[i] + q[i] for i in range(3)) for q in tab.verts[t]] for s in outer for v, t in s['surfs']]
    v = B.zscan_volume(polys, sub=sub)
    return {"buoyant_m3": v, "outer_shells": len(outer), "outer_solid_m3": sum(s['volume'] for s in outer),
            "cavities": len(cavities), "cavity_m3": -sum(s['volume'] for s in cavities), "shells": len(shells)}
