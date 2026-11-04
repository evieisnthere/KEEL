"""Pyramid chirality test. Usage: make_pyramid_test.py <definitions dir> <freyja vehicle.xml> <out.xml>
Section 1 (z = 0): all pyramid/inverse-pyramid types at identity rotation, x = 0, 8, 16 ... Faces coloured by declared
orientation: +x red, -x blue, +y white, -y grey, +z green, -z yellow, slopes orange. Marker columns: red at x = -6
(points along +x row), green at z = +6.
Section 2 (z = -20): the KB Freyja bow cluster as-is (left) and with the inverse pyramids moved +2 in x (right)."""
import re, sys, xml.etree.ElementTree as ET
defs, freyja, out = sys.argv[1:4]
TYPES = ['03_pyramid', '04_invpyramid', '06_pyramid_2', '07_invpyramid_2', '09_pyramid_4', '10_invpyramid_4',
         '11_pyramid_2x2', '14_invpyramid_2x2', '12_pyramid_2x4', '15_invpyramid_2x4', '13_pyramid_4x4', '16_invpyramid_4x4']
COL = {0: 'FF2020', 1: '2050FF', 2: 'F0F0F0', 3: '808080', 4: '20C020', 5: 'FFD000'}
def slope(s): return s in (6, 7, 8) or s >= 21
cs = []
for k, d in enumerate(TYPES):
    t = open(f'{defs}/{d}.xml', errors='ignore').read()
    m = re.search(r'<surfaces>(.*?)</surfaces>', t, re.S)
    surf = re.findall(r'<surface ([^>]*)>', m.group(1))
    cols = []
    for a in surf:
        g = dict(re.findall(r'(\w+)="([^"]*)"', a))
        o, s = int(g.get('orientation', 0)), int(g.get('shape', 0))
        cols.append('FF8000' if slope(s) else COL[o])
    cs.append(f'<c d="{d}"><o r="1,0,0,0,1,0,0,0,1" sc="{len(cols)},{",".join(cols)}"><vp x="{k * 8}"/></o></c>')
for y in range(3):  # markers
    cs.append(f'<c><o r="1,0,0,0,1,0,0,0,1" sc="6" bc="FF0000"><vp x="-6" y="{y}"/></o></c>')
    cs.append(f'<c><o r="1,0,0,0,1,0,0,0,1" sc="6" bc="00C000"><vp x="0" y="{y}" z="6"/></o></c>')
# section 2: Freyja cluster
r = ET.parse(freyja).getroot()
b0 = list(r.iter('body'))[0]
cl = []
for c in b0.find('components'):
    o = c.find('o'); d = c.get('d', '01_block')
    if o is None or d not in ('06_pyramid_2', '07_invpyramid_2') or o.get('r') != '1,0,0,0,-1,0,0,0,-1':
        continue
    vp = o.find('vp'); v = [int(float(vp.get(a, 0) or 0)) if vp is not None else 0 for a in 'xyz']
    if 6 <= v[0] <= 9 and 2 <= v[1] <= 5 and 72 <= v[2] <= 76:
        cl.append((d, v))
for variant, shift in ((0, 0), (1, 2)):
    for d, v in cl:
        x = v[0] - 6 + variant * 12 + (shift if 'inv' in d else 0)
        col = ('C02020' if 'inv' not in d else '2040C0')
        cs.append(f'<c d="{d}"><o r="1,0,0,0,-1,0,0,0,-1" sc="0" bc="{col}"><vp x="{x}" y="{v[1]}" z="{v[2] - 72 - 20}"/></o></c>')
open(out, 'w').write('<?xml version="1.0" encoding="UTF-8"?>\n<vehicle data_version="3" bodies_id="1"><authors/><bodies><body unique_id="0"><components>'
                     + ''.join(cs) + '</components></body></bodies><logic_node_links/></vehicle>\n')
print(len(cs), 'components;', len(cl), 'cluster parts')
