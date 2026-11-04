#!/usr/bin/env python3
"""Flat-colour review renders of a generated block hull (exposed faces from `hulldump`).

usage: hullshot.py faces.json out.png [--focus z0,z1] [--views yaw,pitch;...]
Colours: grey shaded = sloped block faces, blue tint = cube faces, red = step faces (off the NURBS normal by > 25°).
Edges are drawn so the tiling is visible.
"""
import json, math, sys
from PIL import Image, ImageDraw
import numpy as np

def main():
    a = sys.argv[1:]
    data = json.load(open(a[0]))
    out = a[1]
    focus = None
    bytype = '--bytype' in a
    views = [(20, 15), (60, 30), (160, -25), (200, -20), (90, 0), (0, -89)]
    i = 2
    while i < len(a):
        if a[i] == '--focus':
            focus = tuple(float(v) for v in a[i + 1].split(',')); i += 2
        elif a[i] == '--views':
            views = [tuple(float(x) for x in v.split(',')) for v in a[i + 1].split(';')]; i += 2
        else:
            i += 1
    faces = data['faces']
    if focus:
        faces = [f for f in faces if any(focus[0] <= p[2] <= focus[1] for p in f['p'])]
    W, H = 700, 480
    tiles = []
    light = (0.4, 0.8, 0.45)
    ln = math.sqrt(sum(x * x for x in light)); light = tuple(x / ln for x in light)
    for yaw, pitch in views:
        cy, sy = math.cos(math.radians(yaw)), math.sin(math.radians(yaw))
        cp, sp = math.cos(math.radians(pitch)), math.sin(math.radians(pitch))
        # camera looks from direction d toward the origin; right = (cy, 0, -sy), up = cross
        d = (sy * cp, sp, cy * cp)
        r = (cy, 0.0, -sy)
        u = (d[1] * r[2] - d[2] * r[1], d[2] * r[0] - d[0] * r[2], d[0] * r[1] - d[1] * r[0])
        def proj(p):
            return (p[0] * r[0] + p[1] * r[1] + p[2] * r[2], p[0] * u[0] + p[1] * u[1] + p[2] * u[2], p[0] * d[0] + p[1] * d[1] + p[2] * d[2])
        vis = [f for f in faces if f['n'][0] * d[0] + f['n'][1] * d[1] + f['n'][2] * d[2] > 1e-6]
        pts = [proj(p) for f in vis for p in f['p']]
        if not pts:
            continue
        x0, x1 = min(p[0] for p in pts), max(p[0] for p in pts)
        y0, y1 = min(p[1] for p in pts), max(p[1] for p in pts)
        s = min((W - 20) / max(x1 - x0, 1e-6), (H - 20) / max(y1 - y0, 1e-6))
        # depth-buffered rasterisation (numpy), so occlusion is exact
        img = np.zeros((H, W, 3), dtype=np.uint8); img[:] = (18, 19, 43)
        zb = np.full((H, W), -1e9)
        edges = []
        for f in vis:
            n = f['n']
            sh = 0.35 + 0.65 * max(0.0, n[0] * light[0] + n[1] * light[1] + n[2] * light[2])
            if bytype:
                d_ = f['d']
                if d_ == '01_block':
                    col = (120 * sh, 140 * sh, 220 * sh)
                elif d_ in ('02_wedge', '03_pyramid', '04_invpyramid'):
                    col = (240 * sh, 150 * sh, 40 * sh)
                else:
                    col = (215 * sh, 215 * sh, 205 * sh)
            elif f['s']:
                col = (230 * sh, 60 * sh, 60 * sh)
            elif f['d'] == '01_block':
                col = (150 * sh, 175 * sh, 235 * sh)
            else:
                col = (225 * sh, 222 * sh, 215 * sh)
            P = [proj(p) for p in f['p']]
            P = [((q[0] - x0) * s + 10, H - ((q[1] - y0) * s + 10), q[2]) for q in P]
            for k in range(1, len(P) - 1):
                tri = (P[0], P[k], P[k + 1])
                xs = [t[0] for t in tri]; ys = [t[1] for t in tri]
                bx0, bx1 = max(int(min(xs)), 0), min(int(max(xs)) + 1, W - 1)
                by0, by1 = max(int(min(ys)), 0), min(int(max(ys)) + 1, H - 1)
                if bx1 < bx0 or by1 < by0:
                    continue
                X, Y = np.meshgrid(np.arange(bx0, bx1 + 1) + 0.5, np.arange(by0, by1 + 1) + 0.5)
                (ax, ay, az), (bx, by, bz), (cx, cy, cz) = tri
                dd = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
                if abs(dd) < 1e-9:
                    continue
                l1 = ((by - cy) * (X - cx) + (cx - bx) * (Y - cy)) / dd
                l2 = ((cy - ay) * (X - cx) + (ax - cx) * (Y - cy)) / dd
                l3 = 1 - l1 - l2
                m = (l1 >= -1e-6) & (l2 >= -1e-6) & (l3 >= -1e-6)
                z = l1 * az + l2 * bz + l3 * cz
                sub = zb[by0:by1 + 1, bx0:bx1 + 1]
                upd = m & (z > sub + 1e-6)
                sub[upd] = z[upd]
                img[by0:by1 + 1, bx0:bx1 + 1][upd] = col
            edges.append(([(q[0], q[1]) for q in P], col, sum(q[2] for q in P) / len(P)))
        im = Image.fromarray(img)
        dr = ImageDraw.Draw(im)
        # edges where visible (depth test at the edge midpoints)
        for poly, col, zc in edges:
            for k in range(len(poly)):
                a_, b_ = poly[k], poly[(k + 1) % len(poly)]
                mx, my = int((a_[0] + b_[0]) / 2), int((a_[1] + b_[1]) / 2)
                if 0 <= mx < W and 0 <= my < H and zb[my, mx] <= zc + 0.05 * (1 / s if s else 1) + 0.02:
                    dr.line([a_, b_], fill=(int(col[0] * 0.55), int(col[1] * 0.55), int(col[2] * 0.55)))
        dr.text((6, 4), f"yaw {yaw:g} pitch {pitch:g}", fill=(200, 200, 230))
        tiles.append(im)
    cols = 2
    rows = (len(tiles) + 1) // 2
    sheet = Image.new('RGB', (W * cols, H * rows + 22), (10, 10, 25))
    for k, t in enumerate(tiles):
        sheet.paste(t, ((k % cols) * W, (k // cols) * H + 22))
    ImageDraw.Draw(sheet).text((6, 4), data.get('summary', ''), fill=(230, 230, 255))
    sheet.save(out)

main()
