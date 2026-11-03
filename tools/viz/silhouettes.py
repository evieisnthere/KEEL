"""Orthographic occupancy silhouettes (XY, ZY, XZ planes) of a vehicle, for checking axis conventions.
Usage: silhouettes.py defs_dir vehicle.xml out.png"""
import sys, os
import numpy as np
from PIL import Image, ImageDraw
here = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, os.path.join(here, '..', '..', 'src')); sys.path.insert(0, os.path.join(here, '..', 're'))
from keel_io.vehicle import Vehicle, rotate
from defs_load import load
defs, _ = load(sys.argv[1]); v = Vehicle.load(sys.argv[2])
vox = {k: [tuple(int(p.get(a, 0) or 0) for a in 'xyz') for p in (x.find('position') for x in r.find('voxels')) if p is not None] or [(0, 0, 0)]
       for k, r in defs.items()}
pts = []
for c in v.components():
    for q in vox.get(c.definition, [(0, 0, 0)]):
        o = rotate(c.rotation, q); pts.append(tuple(c.voxel[i] + o[i] for i in range(3)))
P = np.array(pts); lo = P.min(0); P = P - lo; ext = P.max(0) + 1
S = max(2, min(8, 900 // int(ext.max())))
def view(a, b, flip_b=True):
    img = np.zeros((ext[b], ext[a]), np.int32)
    np.add.at(img, (P[:, b], P[:, a]), 1)
    if flip_b: img = img[::-1]
    g = (255 - np.clip(img * 40, 0, 220)).astype(np.uint8)
    return Image.fromarray(g).resize((ext[a] * S, ext[b] * S), Image.NEAREST)
views = [('X right, Y up', view(0, 1)), ('Z right, Y up', view(2, 1)), ('X right, Z up', view(0, 2))]
W = sum(im.width for _, im in views) + 40 * len(views); H = max(im.height for _, im in views) + 40
out = Image.new('L', (W, H), 255); d = ImageDraw.Draw(out); x = 20
for label, im in views:
    out.paste(im, (x, 30)); d.text((x, 8), label, fill=0); x += im.width + 40
out.save(sys.argv[3]); print('extent (x,y,z voxels):', tuple(int(e) for e in ext), 'components:', len(pts))
