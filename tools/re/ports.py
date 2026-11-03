"""World-space ports (logic nodes) for every component of a vehicle, and what each physical port faces.
Usage: ports.py defs_dir vehicle.xml [definition-substring ...]"""
import sys, os, collections
here = os.path.dirname(os.path.abspath(__file__)); sys.path[:0] = [here, os.path.join(here, '..', '..', 'sim-reference')]
from defs_load import load
import sw_buoyancy as B
DIRS = [(1,0,0),(-1,0,0),(0,1,0),(0,-1,0),(0,0,1),(0,0,-1)]
def load_vehicle(defs, path):
    occ = {}; ports = []
    for bid, comps in B.vehicle_bodies(path):
        for ci, (d, vp, R) in enumerate(comps):
            r = defs.get(d)
            if r is None: continue
            for v in r.find('voxels'):
                p = v.find('position'); q = tuple(int(float(p.get(a,0) or 0)) for a in 'xyz') if p is not None else (0,0,0)
                w = B.rot_col(R, q); occ[(bid, vp[0]+w[0], vp[1]+w[1], vp[2]+w[2])] = (d, ci)
            for n in r.find('logic_nodes'):
                p = n.find('position'); q = tuple(int(float(p.get(a,0) or 0)) for a in 'xyz') if p is not None else (0,0,0)
                w = B.rot_col(R, q); o = int(n.get('orientation', '0') or 0); dvec = B.rot_col(R, DIRS[o])
                ports.append(dict(body=bid, comp=ci, d=d, label=n.get('label'), type=int(n.get('type', '0') or 0), mode=int(n.get('mode', '0') or 0),
                                  pos=(vp[0]+w[0], vp[1]+w[1], vp[2]+w[2]), dir=dvec))
    return occ, ports
if __name__ == '__main__':
    defs, _ = load(sys.argv[1]); occ, ports = load_vehicle(defs, sys.argv[2]); keys = sys.argv[3:] or ['propeller']
    for p in ports:
        if p['type'] in (2, 3) and any(k in p['d'] for k in keys):
            f = tuple(p['pos'][i] + p['dir'][i] for i in range(3))
            print(p['d'], p['label'], 'type', p['type'], 'at', p['pos'], 'facing', p['dir'], '->', occ.get((p['body'],) + f))
