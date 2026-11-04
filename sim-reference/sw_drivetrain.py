"""Drivetrain topology extraction and 60 Hz tick simulation (RE-0006/0007/0008).

Topology (from a vehicle file + definitions):
  * engine assemblies: 6-connected sets of voxels belonging to modular-engine parts (definition type 52 / modular_engine_*)
  * torque networks: union-find over physical torque ports (type 2) and pipe blocks (trans_*: definition type 6),
    joined where a port/pipe-opening faces the neighbouring voxel and that voxel's port/opening faces back
  * links: modular clutch (engine assembly → network), gearbox / torque_clutch (network ↔ network), propeller (network)
Inputs are set directly here (air/fuel throttle, clutch pressure, gear switch). Logic evaluation is a separate layer.

Assumptions not yet backed by E1 are marked ASSUME and listed in docs/reverse-engineering/0009-drivetrain-simulation.md.
"""
from __future__ import annotations
import collections, math
import sw_buoyancy as B, sw_engine as E

DIRS = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
GEAR = {0: -1.0, 1: 1.0, 2: 5 / 6, 3: 2 / 3, 4: 5 / 9, 5: 0.5, 6: 0.4, 7: 1 / 3}
CRANK_FAMILY = {0, 1, 18, 19, 20, 22, 24, 25, 27}      # engine_module_type of parts on the crank


class UF:
    def __init__(self): self.p = {}
    def f(self, x):
        self.p.setdefault(x, x)
        while self.p[x] != x:
            self.p[x] = self.p[self.p[x]]; x = self.p[x]
        return x
    def u(self, a, b): self.p[self.f(a)] = self.f(b)


def _v(e):
    return tuple(int(float(e.get(a, 0) or 0)) for a in 'xyz') if e is not None else (0, 0, 0)


def extract(vehicle_path, defs):
    comps = []                                     # (body, idx, def, vp, R, o_element)
    import xml.etree.ElementTree as ET
    root = ET.parse(vehicle_path).getroot()
    for b in root.iter('body'):
        bid = b.get('unique_id')
        for c in b.iter('c'):
            o = c.find('o')
            if o is None:
                continue
            d = c.get('d', '01_block'); vp = _v(o.find('vp'))
            R = [int(x) for x in o.get('r', '0,0,-1,-1,0,0,0,-1,0')  # game default when r is absent (E2).split(',')]
            comps.append((bid, len(comps), d, vp, R, o))
    vox = {}; ends = collections.defaultdict(list)   # (body, voxel) -> comp ; (body, voxel, dir) openings/ports
    ports = []
    for bid, ci, d, vp, R, o in comps:
        r = defs.get(d)
        if r is None:
            continue
        for v in r.find('voxels'):
            q = B.rot_col(R, _v(v.find('position'))); vox[(bid,) + tuple(vp[i] + q[i] for i in range(3))] = ci
        if r.get('type') == '6':                   # pipe: openings are surfaces with trans_type 1
            for s in r.find('surfaces'):
                if s.get('trans_type') == '1':
                    q = B.rot_col(R, _v(s.find('position'))); dv = B.rot_col(R, DIRS[int(s.get('orientation', '0'))])
                    ends[(bid,) + tuple(vp[i] + q[i] for i in range(3))].append((dv, ('pipe', ci)))
        for k, n in enumerate(r.find('logic_nodes')):
            if n.get('type') != '2':
                continue
            q = B.rot_col(R, _v(n.find('position'))); dv = B.rot_col(R, DIRS[int(n.get('orientation', '0') or 0)])
            pos = (bid,) + tuple(vp[i] + q[i] for i in range(3))
            ends[pos].append((dv, ('port', ci, k))); ports.append(((('port', ci, k)), ci, d, n.get('label')))
    uf = UF()
    for pos, lst in ends.items():
        for dv, node in lst:
            uf.f(node)
            nb = (pos[0],) + tuple(pos[i + 1] + dv[i] for i in range(3))
            for dv2, node2 in ends.get(nb, []):
                if all(dv2[i] == -dv[i] for i in range(3)):
                    uf.u(node, node2)
    # engine assemblies: 6-connected modular-engine voxels
    eng_uf = UF(); modvox = {k: ci for k, ci in vox.items() if comps[ci][2].startswith('modular_engine')}
    for k, ci in modvox.items():
        eng_uf.f(ci)
        for dv in DIRS:
            nb = (k[0],) + tuple(k[i + 1] + dv[i] for i in range(3))
            if nb in modvox:
                eng_uf.u(ci, modvox[nb])
    return comps, ports, uf, eng_uf


class Drivetrain:
    """Minimal executable drivetrain: engines, networks, clutches, gearboxes, props."""

    def __init__(self, vehicle_path, defs, *, manifold_temp_c=90.0, gear_switch=False, clutch_pressure=1.0,
                 air_throttle=1.0, fuel_throttle=1.0, fuel_pressure=100.0):
        comps, ports, uf, eng_uf = extract(vehicle_path, defs)
        self.defs = defs; self.comps = comps
        net = lambda node: uf.f(node)
        # engines
        self.engines = collections.defaultdict(lambda: dict(I=0.0, cyl=[], clutches=[], rps=0.0, burn=1.0))
        for bid, ci, d, vp, R, o in comps:
            r = defs.get(d)
            if r is None or not d.startswith('modular_engine'):
                continue
            mt = int(r.get('engine_module_type', '0') or 0); e = self.engines[eng_uf.f(ci)]
            if mt in CRANK_FAMILY and mt not in (1, 22, 27):
                e['I'] += float(r.get('engine_max_force', 0) or 0)
            if mt in (1, 22, 27):
                e['cyl'].append(len(r.find('voxels')))     # ASSUME cylinder size = voxel count
        self.networks = collections.defaultdict(lambda: dict(rps=0.0, slots=[]))
        self.clutches = []; self.gearboxes = []; self.tclutches = []; self.props = []
        port_by_comp = collections.defaultdict(list)
        for node, ci, d, label in ports:
            port_by_comp[ci].append((label, net(node)))
        for bid, ci, d, vp, R, o in comps:
            r = defs.get(d)
            if r is None:
                continue
            p = dict(port_by_comp.get(ci, []))
            if d.startswith('modular_engine_clutch') and 'RPS' in p:
                self.clutches.append(dict(engine=eng_uf.f(ci), net=p['RPS'], I=float(r.get('engine_max_force')), e=0.0))
            elif 'gearbox' in d and 'RPS A' in p and 'RPS B' in p:
                g1 = int(o.get('gear_ratio_1', '1')); g2 = int(o.get('gear_ratio_2', '5'))
                self.gearboxes.append(dict(a=p['RPS A'], b=p['RPS B'], ratio=GEAR[g2 if gear_switch else g1]))
            elif d == 'torque_clutch' and 'RPS A' in p and 'RPS B' in p:
                self.tclutches.append(dict(a=p['RPS A'], b=p['RPS B']))
            elif (d.startswith('propeller') or d == 'large_propeller') and 'RPS' in p:
                kw = dict(max_force=float(r.get('force_emitter_max_force')), rotation_speed=float(r.get('force_emitter_rotation_speed', 1) or 1),
                          blade_length=float(r.get('force_emitter_blade_physics_length')), blade_efficiency=float(r.get('force_emitter_blade_efficiency')),
                          efficiency=float(r.get('force_emitter_efficiency', 1) or 1))
                self.props.append(dict(def_=d, net=p['RPS'], kw=kw, rps=0.0, thrust=0.0))
        for k in [c['net'] for c in self.clutches] + [g['a'] for g in self.gearboxes] + [g['b'] for g in self.gearboxes] + \
                 [t['a'] for t in self.tclutches] + [t['b'] for t in self.tclutches] + [p['net'] for p in self.props]:
            self.networks[k]
        self.T = manifold_temp_c; self.cp = clutch_pressure; self.th = air_throttle; self.fu = fuel_throttle; self.fp = fuel_pressure

    def summary(self):
        return dict(engines=[(len(e['cyl']), sum(e['cyl']), round(e['I'], 1)) for e in self.engines.values() if e['cyl']],
                    clutches=len(self.clutches), gearboxes=[round(g['ratio'], 3) for g in self.gearboxes],
                    torque_clutches=len(self.tclutches), props=[p['def_'] for p in self.props], networks=len(self.networks))

    def tick(self, boat_speed=0.0):
        # 1. engines: cylinder combustion (RE-0006); ASSUME air & fuel supply unlimited, manifold temp fixed
        for e in self.engines.values():
            if not e['cyl'] or e['I'] <= 0:
                continue
            d = 0.0
            for size in e['cyl']:
                st = E.cylinder_tick(max(e['rps'], 0.0), self.th, self.fu, self.T, size, e['I'], 1e9, burn=e['burn'], fuel_pressure=self.fp)
                e['afr'] = st.afr; e['eff'] = st.efficiency
                d += st.d_rps
            e['rps'] += d
        # 2. slot contributions → network averaging (RE-0008 §4)
        acc = collections.defaultdict(lambda: [0.0, 0.0])
        for c in self.clutches:
            c['e'] += max(-0.1, min(0.1, self.cp - c['e']))
            eng = self.engines.get(c['engine'])
            if eng is None or eng['I'] <= 0:
                continue
            a = acc[c['net']]; a[0] += c['e'] * eng['I'] * eng['rps']; a[1] += c['e'] * eng['I']
        for p in self.props:
            n = self.networks[p['net']]
            newr, thrust, absorbed = E.propeller_tick(n['rps'], 1.0, v_axial=-boat_speed, **p['kw'])
            m = p['kw']['max_force'] * 0.5 / 1000.0                  # F_eff/1000 with 4 blades (ASSUME blades=4)
            a = acc[p['net']]; a[0] += m * newr; a[1] += m
            p['thrust'] = thrust
        # gearbox / clutch coupling (ASSUME: accumulators applied immediately, f = 0.95)
        for g in self.gearboxes + [dict(a=t['a'], b=t['b'], ratio=1.0, f=self.cp ** 2) for t in self.tclutches]:
            f = g.get('f', 1.0) * 0.95; na, nb = self.networks[g['a']], self.networks[g['b']]
            wa = 1.0 + acc[g['a']][1]; wb = 1.0 + acc[g['b']][1]
            acc[g['b']][0] += g['ratio'] * na['rps'] * wa * f; acc[g['b']][1] += wa * f
            acc[g['a']][0] += nb['rps'] / g['ratio'] * wb * f; acc[g['a']][1] += wb * f
        for k, n in self.networks.items():
            a = acc[k]; n['rps'] = (n['rps'] + a[0]) / (1.0 + a[1])
        # 3. clutches pull the crank toward the network
        for c in self.clutches:
            eng = self.engines.get(c['engine'])
            if eng:
                eng['rps'] += c['e'] * (self.networks[c['net']]['rps'] - eng['rps'])
        return sum(p['thrust'] for p in self.props)
