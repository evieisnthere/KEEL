"""Render ground-truth test vehicle: every generated block type with each surface index painted a distinct colour,
wedges in four rotations, a paintable sign with an asymmetric glyph, a propeller and an enclosed pipe.
Usage: make_render_test.py out_dir   (writes vehicle.xml; load it in game and screenshot from the stated angle)"""
import os, sys
PAL = ['FF0000', '00C000', '0050FF', 'FFD000', 'FF00FF', '00E0E0', 'FF8000', '8000FF', 'FFFFFF', '202020', '80FF80', '804000']
BLOCKS = ['01_block', '02_wedge', '03_pyramid', '04_invpyramid', '05_wedge_2', '06_pyramid_2', '07_invpyramid_2', '08_wedge_4',
          '09_pyramid_4', '10_invpyramid_4', '11_pyramid_2x2', '12_pyramid_2x4', '13_pyramid_4x4', '14_invpyramid_2x2',
          '15_invpyramid_2x4', '16_invpyramid_4x4']
NSURF = {'01_block': 6, '02_wedge': 5, '03_pyramid': 4, '04_invpyramid': 7, '05_wedge_2': 9, '06_pyramid_2': 7, '07_invpyramid_2': 12,
         '08_wedge_4': 17, '09_pyramid_4': 13, '10_invpyramid_4': 26, '11_pyramid_2x2': 10, '12_pyramid_2x4': 18, '13_pyramid_4x4': 28,
         '14_invpyramid_2x2': 20, '15_invpyramid_2x4': 36, '16_invpyramid_4x4': 56}
ROTY = ['1,0,0,0,1,0,0,0,1', '0,0,-1,0,1,0,1,0,0', '-1,0,0,0,1,0,0,0,-1', '0,0,1,0,1,0,-1,0,0']   # 0/90/180/270 about Y
F = ["000000000", "011111100", "011111100", "011000000", "011111000", "011111000", "011000000", "011000000", "000000000"]

def comp(d, x, y, z, r=None, sc=None, extra=''):
    ra = f' r="{r}"' if r else ''
    sca = f' sc="{sc}"' if sc else ''
    vp = ''.join(f' {a}="{v}"' for a, v in zip('xyz', (x, y, z)) if v)
    return f'<c d="{d}"><o{ra}{sca}{extra}><vp{vp}/></o></c>'

cs = []
for k, d in enumerate(BLOCKS):           # row of block types along +x, spaced 6 voxels
    n = NSURF[d]
    cs.append(comp(d, k * 6, 0, 0, sc=str(n) + ',' + ','.join(PAL[i % len(PAL)] for i in range(n))))
for k, r in enumerate(ROTY):             # wedges in four rotations, row at z = 10 / 16
    cs.append(comp('02_wedge', k * 4, 0, 10, r=r, sc='5,' + ','.join(PAL[:5])))
    cs.append(comp('05_wedge_2', k * 4, 0, 16, r=r, sc='9,' + ','.join(PAL[:9])))
gc = ','.join('FF0000' if F[row][col] == '1' else 'FFFFFF' for row in range(9) for col in range(9))
cs.append(comp('sign_na', 0, 0, 24, extra=f' bc="FFFFFF" gc="{gc}"'))                       # face up
cs.append(comp('sign_na', 4, 1, 24, r='1,0,0,0,0,1,0,-1,0', extra=f' bc="FFFFFF" gc="{gc}"'))  # face +z
cs.append(comp('propeller', 8, 0, 24))
cs.append(comp('trans_block_straight', 12, 0, 24))
xml = ('<?xml version="1.0" encoding="UTF-8"?>\n<vehicle data_version="3" bodies_id="1"><authors/><bodies><body unique_id="0">'
       '<components>' + ''.join(cs) + '</components></body></bodies><logic_node_links/></vehicle>\n')
os.makedirs(sys.argv[1], exist_ok=True)
open(os.path.join(sys.argv[1], 'vehicle.xml'), 'w').write(xml)
print('wrote', os.path.join(sys.argv[1], 'vehicle.xml'), len(cs), 'components')
