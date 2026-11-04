//! Phase P1 block hull (docs/engineering/hull-to-blocks.md): additive lattice regions.
//!
//! Lattice (starboard half): voxel i spans ℓx ∈ [i, i+1] with ℓx = x/voxel + 0.5 (the centre column is voxel 0),
//! ℓy = y/voxel (row j spans [j, j+1] above the baseline), ℓz = z/voxel + 0.5.
//! * SIDE (above the chine row): x-height field ℓx = A(z) + B(y).
//! * BOTTOM (below the chine): y-height field ℓy = C(z) + D(x), solid above, for ℓx ≥ 1.
//! * Centre column (ℓx ∈ [0, 1]): bottom height C(z) + D(1). Only flat in x (cubes and fore-aft wedges), never
//!   pyramids, because it is shared by both halves.
//! * Deck: flat plate at the midship sheer (P1; sheer ramps in P2).
//! * Shell: one voxel. The port half mirrors the starboard half.
use crate::hull::Hull;
use crate::lattice::{fill_additive, path_height, quantize_path, Frame, PathOptions, Step};
use crate::voxel::{Placement, VOXEL};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HullBlockOptions {
    pub origin: [i32; 3],
    pub deck: bool,
}

impl Default for HullBlockOptions {
    fn default() -> Self {
        HullBlockOptions { origin: [0, 0, 0], deck: true }
    }
}

/// Least-squares additive fit f(i, j) ≈ a[i] + b[j] over the defined samples.
fn additive_fit(f: &[Vec<Option<f64>>]) -> (Vec<f64>, Vec<f64>) {
    let ni = f.len();
    let nj = f.first().map(|r| r.len()).unwrap_or(0);
    let mut a = vec![0.0; ni];
    let mut b = vec![0.0; nj];
    for _ in 0..30 {
        for i in 0..ni {
            let (s, n) = (0..nj).filter_map(|j| f[i][j].map(|v| v - b[j])).fold((0.0, 0), |m, v| (m.0 + v, m.1 + 1));
            if n > 0 {
                a[i] = s / n as f64;
            }
        }
        for j in 0..nj {
            let (s, n) = (0..ni).filter_map(|i| f[i][j].map(|v| v - a[i])).fold((0.0, 0), |m, v| (m.0 + v, m.1 + 1));
            if n > 0 {
                b[j] = s / n as f64;
            }
        }
    }
    (a, b)
}

pub struct HullBlocks {
    pub placements: Vec<Placement>,
    /// quantised paths, for inspection and tests
    pub side_a: Vec<Step>,
    pub side_b: Vec<Step>,
    pub unmatched: usize,
}

pub fn build(hull: &Hull, opt: &HullBlockOptions) -> HullBlocks {
    let p = &hull.params;
    let lz0 = ((-p.length / 2.0) / VOXEL + 0.5).floor() as i32;
    let lz1 = ((p.length / 2.0) / VOXEL + 0.5).ceil() as i32;
    let secs: Vec<Option<Vec<[f64; 2]>>> = (lz0..=lz1).map(|lz| hull.section_at((lz as f64 - 0.5) * VOXEL, 64)).collect();
    let half = |iz: usize, y: f64| -> Option<f64> {
        let s = secs[iz].as_ref()?;
        if y < s[0][1] || y > s[s.len() - 1][1] {
            return None;
        }
        crate::hull::Hull::half_breadth(s, y)
    };
    // rows: chine at the height where the midship section turns 45° (bilge), deck at the midship sheer
    let mid = secs.len() / 2;
    let ms = secs[mid].clone().unwrap_or_default();
    let mut y_chine = p.draft * 0.3;
    for w in ms.windows(2) {
        let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
        if dy > dx {
            y_chine = w[0][1];
            break;
        }
    }
    let row_c = (y_chine / VOXEL).round().max(1.0) as i32;
    let row_top = (p.sheer(0.5) / VOXEL).round() as i32;

    // ---- SIDE: ℓx = A(z) + B(y) over rows row_c..row_top, z lattice lz0..lz1
    let side_samples: Vec<Vec<Option<f64>>> = (0..secs.len())
        .map(|iz| (row_c..=row_top).map(|ry| half(iz, ry as f64 * VOXEL).map(|b| b / VOXEL + 0.5)).collect())
        .collect();
    let (a_fit, b_fit) = additive_fit(&side_samples);
    // B carries the absolute level: put the midship A at 0
    let a_mid = a_fit[mid];
    let a_t: Vec<f64> = a_fit.iter().map(|x| x - a_mid).collect();
    let b_t: Vec<f64> = b_fit.iter().map(|x| x + a_mid).collect();
    let po = PathOptions::default();
    let side_a = quantize_path(&a_t, &po);
    let side_b = quantize_path(&b_t, &po);
    // shift to lattice positions
    let side_a: Vec<Step> = side_a.into_iter().map(|s| Step { start: s.start + lz0, ..s }).collect();
    let side_b: Vec<Step> = side_b.into_iter().map(|s| Step { start: s.start + row_c, ..s }).collect();
    let side_h = |ry: f64, lz: f64| path_height(&side_a, lz) + path_height(&side_b, ry);

    // ---- BOTTOM: ℓy = C(z) + D(x) for ℓx in 1..X, below the chine
    let bottom_y = |iz: usize, lx: f64| -> Option<f64> {
        // invert the section polyline (keel → chine part) for height at half-breadth x
        let s = secs[iz].as_ref()?;
        let x = (lx - 0.5) * VOXEL;
        for w in s.windows(2) {
            if w[0][1] > y_chine + 1e-9 {
                break;
            }
            if (w[0][0] <= x && x <= w[1][0]) || (w[1][0] <= x && x <= w[0][0]) {
                let t = if (w[1][0] - w[0][0]).abs() < 1e-12 { 0.0 } else { (x - w[0][0]) / (w[1][0] - w[0][0]) };
                return Some((w[0][1] + (w[1][1] - w[0][1]) * t) / VOXEL);
            }
        }
        None
    };
    // the bottom path D runs from the centre column (ℓx = 1) to the chine x of the side path at midships, and must
    // end at the chine row there, so the section is one continuous lattice polyline (no step at the chine)
    let lmid = (lz0 + mid as i32) as f64;
    let xc = (path_height(&side_b, row_c as f64) + path_height(&side_a, lmid)).round() as i32;
    let x_max = xc.max(2);
    let bot_samples: Vec<Vec<Option<f64>>> = (0..secs.len()).map(|iz| (1..=x_max).map(|lx| bottom_y(iz, lx as f64)).collect()).collect();
    let (c_fit, d_fit) = additive_fit(&bot_samples);
    let c_mid = c_fit[mid];
    let c_t: Vec<f64> = c_fit.iter().map(|x| x - c_mid).collect();
    let bot_c: Vec<Step> = quantize_path(&c_t, &po).into_iter().map(|s| Step { start: s.start + lz0, ..s }).collect();
    let cq_mid = path_height(&bot_c, lmid);
    let d_has: Vec<bool> = (0..x_max as usize).map(|j| bot_samples.iter().any(|r| r[j].is_some())).collect();
    let d_t: Vec<f64> = d_fit.iter().zip(&d_has).map(|(x, &h)| if h { x + c_mid } else { f64::NAN }).collect();
    let po_d = PathOptions { end_h: Some(row_c - cq_mid.round() as i32), ..po.clone() };
    let bot_d: Vec<Step> = quantize_path(&d_t, &po_d).into_iter().map(|s| Step { start: s.start + 1, ..s }).collect();

    let mut occupied: HashMap<[i32; 3], ()> = HashMap::new();
    let mut unmatched = 0;
    let mut out: Vec<Placement> = Vec::new();
    let o = opt.origin;
    let push = |out: &mut Vec<Placement>, occupied: &mut HashMap<[i32; 3], ()>, pl: Placement, cells: &[[i32; 3]]| {
        if cells.iter().any(|c| occupied.contains_key(c)) {
            return;
        }
        for c in cells {
            occupied.insert(*c, ());
        }
        out.push(Placement { vp: [pl.vp[0] + o[0], pl.vp[1] + o[1], pl.vp[2] + o[2]], ..pl });
    };
    // a tile is kept when the side surface is outside the centre column at all its corners and the hull exists there
    let side_keep = |ry0: i32, lz0_: i32| -> bool {
        let sa = side_a.iter().find(|s| s.start == lz0_).unwrap();
        let sb = side_b.iter().find(|s| s.start == ry0).unwrap();
        [(ry0, lz0_), (ry0 + sb.run, lz0_), (ry0, lz0_ + sa.run), (ry0 + sb.run, lz0_ + sa.run)].iter().all(|&(r, z)| side_h(r as f64, z as f64) >= 1.0)
    };
    for mirror in [false, true] {
        let frame = Frame { h_axis: 0, u_axis: 1, v_axis: 2, solid_below: true, offset: [0, 0, 0], mirror_x: mirror };
        let f = fill_additive(&frame, 0, &side_b, &side_a, 1, &|r, z| side_keep(r, z));
        unmatched += f.unmatched;
        let mut cells_of: HashMap<usize, Vec<[i32; 3]>> = HashMap::new();
        for (c, i) in &f.occupied {
            cells_of.entry(*i).or_default().push(*c);
        }
        for (i, pl) in f.placements.into_iter().enumerate() {
            // voxel indices: lattice cell ℓ ∈ [i, i+1] maps to world voxel x = i (starboard) / -i (port)
            let cells = cells_of.remove(&i).unwrap_or_default();
            if cells.iter().any(|c| c[0] == 0) {
                continue; // the centre column belongs to the centre builder
            }
            push(&mut out, &mut occupied, pl, &cells);
        }
    }
    // bottom: keep tiles fully inside the side's chine line
    let bot_keep = |lx0: i32, lz0_: i32| -> bool {
        let sd = bot_d.iter().find(|s| s.start == lx0).unwrap();
        let sc = bot_c.iter().find(|s| s.start == lz0_).unwrap();
        (lz0_..=lz0_ + sc.run).all(|z| (lx0 + sd.run) as f64 <= side_h(row_c as f64, z as f64) + 1e-9)
            && (lz0_..=lz0_ + sc.run).all(|z| path_height(&bot_c, z as f64) + path_height(&bot_d, lx0 as f64) < row_c as f64 + 1e-9)
    };
    for mirror in [false, true] {
        let frame = Frame { h_axis: 1, u_axis: 0, v_axis: 2, solid_below: false, offset: [0, 0, 0], mirror_x: mirror };
        let f = fill_additive(&frame, 0, &bot_d, &bot_c, 1, &|x, z| bot_keep(x, z));
        unmatched += f.unmatched;
        let mut cells_of: HashMap<usize, Vec<[i32; 3]>> = HashMap::new();
        for (c, i) in &f.occupied {
            cells_of.entry(*i).or_default().push(*c);
        }
        for (i, pl) in f.placements.into_iter().enumerate() {
            let cells = cells_of.remove(&i).unwrap_or_default();
            push(&mut out, &mut occupied, pl, &cells);
        }
    }
    // centre column (ℓx ∈ [0, 1]): bottom skin from the fore-aft path C(z) + D(1), flat across (no pyramids)
    let d1 = path_height(&bot_d, 1.0);
    let centre_u = [Step { start: 0, run: 1, h0: d1.round() as i32, rise: 0 }];
    {
        let frame = Frame { h_axis: 1, u_axis: 0, v_axis: 2, solid_below: false, offset: [0, 0, 0], mirror_x: false };
        let f = fill_additive(&frame, 0, &centre_u, &bot_c, 1, &|_, z| (z..=z + 4).any(|zz| side_h(row_c as f64, zz as f64) >= 1.0));
        unmatched += f.unmatched;
        let mut cells_of: HashMap<usize, Vec<[i32; 3]>> = HashMap::new();
        for (c, i) in &f.occupied {
            cells_of.entry(*i).or_default().push(*c);
        }
        for (i, pl) in f.placements.into_iter().enumerate() {
            let cells = cells_of.remove(&i).unwrap_or_default();
            push(&mut out, &mut occupied, pl, &cells);
        }
    }
    // ---- closure: the quantised model solid (bottom C + D, side A + B, deck at row_top) defines the hull. Every
    // model-boundary voxel that no tile covers becomes a cube. In the midbody the tiles already cover the boundary,
    // so this only closes the ends (stem, transom), the chine seam and the deck plate.
    let cube = |v: [i32; 3]| Placement { def: "01_block".into(), vp: v, r: keel_io::vehicle::IDENTITY };
    let has = |iz: usize| side_samples[iz].iter().any(|v| v.is_some());
    let iz_lo = (0..secs.len()).find(|&i| has(i)).unwrap_or(0) as i32;
    let iz_hi = (0..secs.len()).rev().find(|&i| has(i)).unwrap_or(0) as i32;
    let (vz_lo, vz_hi) = (lz0 + iz_lo, lz0 + iz_hi);
    let model_inside = |v: [i32; 3]| -> bool {
        if v[2] < vz_lo || v[2] > vz_hi {
            return false;
        }
        let (lx, ly, lz) = (v[0].abs() as f64 + 0.5, v[1] as f64 + 0.5, v[2] as f64 + 0.5);
        if opt.deck && ly > row_top as f64 {
            return false;
        }
        let bottom = path_height(&bot_c, lz) + path_height(&bot_d, lx.max(1.0));
        ly >= bottom && lx <= side_h(ly.max(row_c as f64), lz)
    };
    let x_ext = (0..=row_top).map(|ry| side_h(ry.max(row_c) as f64, ((lz0 + lz1) / 2) as f64)).fold(0.0, f64::max).ceil() as i32 + 2;
    let y_lo = (vz_lo..=vz_hi).map(|z| path_height(&bot_c, z as f64)).fold(f64::MAX, f64::min).floor() as i32 - 1;
    let y_hi = if opt.deck { row_top } else { row_top - 1 };
    for z in vz_lo..=vz_hi {
        for y in y_lo..y_hi {
            for x in -x_ext..=x_ext {
                let v = [x, y, z];
                if occupied.contains_key(&v) || !model_inside(v) {
                    continue;
                }
                let nb = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
                let open_top = !opt.deck && y == y_hi - 1;
                if nb.iter().any(|d| !model_inside([x + d[0], y + d[1], z + d[2]]) && !(open_top && d[1] == 1)) {
                    push(&mut out, &mut occupied, cube(v), &[v]);
                }
            }
        }
    }
    let _ = (bot_samples, side_samples);
    HullBlocks { placements: out, side_a, side_b, unmatched }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hull::HullParams;
    use std::collections::{BTreeSet, HashSet};

    fn prism() -> HullParams {
        HullParams { length: 12.0, beam: 4.0, depth: 2.0, draft: 1.0, entrance: 0.05, run: 0.0, transom_width: 1.0, transom_lift: 0.0, forefoot: 0.0, sheer_bow: 0.0, sheer_stern: 0.0, bow_deadrise_deg: 12.0, ..HullParams::default() }
    }

    #[test]
    fn default_hull_builds_cleanly() {
        let h = HullParams::default().build().unwrap();
        let b = build(&h, &HullBlockOptions::default());
        assert_eq!(b.unmatched, 0);
        assert!(b.placements.len() > 300);
        // no two placements share a voxel origin
        let mut seen = HashSet::new();
        assert!(b.placements.iter().all(|p| seen.insert(p.vp)));
        assert!(b.placements.iter().any(|p| p.def.contains("pyramid")), "tapered regions must use pyramid pairs");
    }

    #[test]
    fn default_hull_metrics() {
        let h = HullParams::default().build().unwrap();
        let b = build(&h, &HullBlockOptions::default());
        let m = crate::metrics::measure(&b.placements, &h);
        eprintln!("METRICS {}", m.summary());
        assert_eq!(m.leak_samples, 0, "Q1: the shell must be watertight");
        assert!(m.mid_step_share() < 0.06, "Q2 regression: {:.3}", m.mid_step_share());
        let mut hist: std::collections::BTreeMap<String, (usize, f64, f64, f64, f64)> = Default::default();
        for (c, n) in &m.steps {
            let k = format!("n {:.2},{:.2},{:.2} y {:.2}", n[0], n[1], n[2], c[1]);
            let e = hist.entry(k).or_insert((0, f64::MAX, f64::MIN, 0.0, 0.0));
            e.0 += 1;
            e.1 = e.1.min(c[2]);
            e.2 = e.2.max(c[2]);
            e.3 = c[0];
        }
        for (k, v) in hist.iter().filter(|(_, v)| v.0 > 3) {
            eprintln!("  {k}: {} faces z {:.2}..{:.2} x {:.2}", v.0, v.1, v.2, v.3);
        }
    }

    #[test]
    fn prism_rows_are_identical() {
        let h = prism().build().unwrap();
        let b = build(&h, &HullBlockOptions::default());
        // midbody rows (exclude 2 m at each end): the set of (def, x, y, r) must be the same for every z
        let row = |z: i32| -> BTreeSet<(String, i32, i32, [i32; 9])> { b.placements.iter().filter(|p| p.vp[2] == z).map(|p| (p.def.clone(), p.vp[0], p.vp[1], p.r)).collect() };
        let zs: Vec<i32> = (-16..=16).collect();
        let r0 = row(zs[0]);
        assert!(!r0.is_empty());
        for z in &zs[1..] {
            assert_eq!(row(*z), r0, "row {z} differs from row {}", zs[0]);
        }
    }

    #[test]
    fn centre_column_never_has_pyramids() {
        let h = HullParams::default().build().unwrap();
        let b = build(&h, &HullBlockOptions::default());
        assert!(b.placements.iter().filter(|p| p.vp[0] == 0).all(|p| !p.def.contains("pyramid")));
    }
}
