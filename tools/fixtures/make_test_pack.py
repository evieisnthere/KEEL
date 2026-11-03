"""Generate in-game calibration vehicles (KEEL-authored, no game content) plus model predictions.
Usage: make_test_pack.py defs_dir metadata_dir out_dir"""
import sys, os, json
here = os.path.dirname(os.path.abspath(__file__))
sys.path[:0] = [os.path.join(here, '..', '..', 'src'), os.path.join(here, '..', 're'), os.path.join(here, '..', '..', 'sim-reference')]
from defs_load import load
import sw_buoyancy as B, sw_hydro as H

def box(nx, ny, nz, hollow=False, open_top=False):
    cells = []
    for x in range(nx):
        for y in range(ny):
            for z in range(nz):
                shell = x in (0, nx - 1) or y == 0 or z in (0, nz - 1) or (y == ny - 1 and not open_top)
                if not hollow or shell:
                    cells.append((x - nx // 2, y, z - nz // 2))
    return cells

def vehicle_xml(cells, block='01_block'):
    comps = []
    for x, y, z in cells:
        vp = ''.join(f' {a}="{v}"' for a, v in zip('xyz', (x, y, z)) if v)
        d = '' if block == '01_block' else f' d="{block}"'
        comps.append(f'<c{d}><o r="1,0,0,0,1,0,0,0,1" bc="FFFFFF" ac="FFFFFF" sc="6"><vp{vp}/></o></c>')
    return ('<?xml version="1.0" encoding="UTF-8"?>\n<vehicle data_version="3" bodies_id="1"><authors/><bodies><body unique_id="0">'
            '<components>' + ''.join(comps) + '</components></body></bodies><logic_node_links/></vehicle>\n')

TESTS = [  # name, cells, purpose
    ('A_solid_8x4x16',   box(8, 4, 16),   'light solid barge: tests buoyancy formula at k_buoy=10'),
    ('C_solid_26x5x100', box(26, 5, 100), 'heavy solid barge: model says floats nearly awash (mass scaling)'),
    ('D_solid_36x5x100', box(36, 5, 100), 'heavier solid barge: model says SINKS (mass-scaled buoyancy)'),
    ('E_hollow_closed_12x6x24', box(12, 6, 24, hollow=True), 'closed hollow box: tests that enclosed air counts'),
    ('F_hollow_opentop_12x6x24', box(12, 6, 24, hollow=True, open_top=True), 'open-top box: z-scan model says same buoyancy as E'),
]

defs, _ = load(sys.argv[1]); T = B.SurfaceTables(sys.argv[2]); out = sys.argv[3]; os.makedirs(out, exist_ok=True)
I = (1, 0, 0, 0, 1, 0, 0, 0, 1); report = []
for name, cells, purpose in TESTS:
    comps = [('01_block', c, I) for c in cells]
    M = float(len(cells)); r = B.body_shell(comps, defs, T); k = H.buoyancy_divisor(M)
    V = r['volume_m3']; ny = max(c[1] for c in cells) + 1
    frac = M * k / (1000.0 * V)
    pred = (f'floats, draft ≈ {frac * ny * 0.25:.2f} m of {ny * 0.25:.2f} m depth ({frac * 100:.0f}% of envelope)' if frac < 1
            else f'sinks (needs {frac * 100:.0f}% of envelope volume)')
    open(os.path.join(out, name + '.xml'), 'w').write(vehicle_xml(cells))
    report.append({'file': name + '.xml', 'blocks': len(cells), 'mass': M, 'k_buoy': round(k, 3),
                   'envelope_volume_m3': round(V, 4), 'prediction': pred, 'purpose': purpose})
json.dump(report, open(os.path.join(out, 'predictions.json'), 'w'), indent=1)
for r in report: print(f"{r['file']:32s} blocks={r['blocks']:6d} k={r['k_buoy']:6.3f}  {r['prediction']}")
