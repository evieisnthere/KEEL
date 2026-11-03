"""Check that each definition's buoyancy_surfaces form a closed shell (sum of area-weighted normals == 0)."""
import sys, os, json, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__))); from defs_load import load
defs, _ = load(sys.argv[1]); md = sys.argv[2]
conv = json.load(open(md + '/surface_conversion.json'))['conv']
sh = {s['shape']: s for s in json.load(open(md + '/surface_shapes.json'))['shapes']}
bs = collections.Counter(); res = collections.Counter(); bad = []; unk = collections.Counter()
for name, r in defs.items():
    surfs = r.find('buoyancy_surfaces')
    if surfs is None or len(surfs) == 0: res['no_buoyancy_surfaces'] += 1; continue
    v = [0.0] * 3
    for s in surfs:
        key = f"{s.get('orientation','0')},{s.get('rotation','0')},{s.get('shape','0')}"; bs[s.get('shape', '0')] += 1
        c = conv.get(key)
        if not c: unk[s.get('shape', '0')] += 1; continue
        for t in c['types']:
            for i in range(3): v[i] += sh[t]['normal'][i] * sh[t]['area_m2']
    if max(abs(x) for x in v) < 1e-3: res['closed'] += 1
    else: res['open'] += 1; bad.append((name, [round(x, 4) for x in v]))
print('buoyancy surface shape ids:', bs.most_common()); print('unconverted shape ids:', unk.most_common())
print(dict(res)); print('open examples:', bad[:12])
