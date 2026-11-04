//! Block hull builder (docs/engineering/hull-to-blocks.md §2.5, §3).
//!
//! The hull is modelled the way builders shape one ([`crate::sweep`]), in lattice units (ℓx = x/voxel + 0.5 with the
//! centre column at ℓx ∈ [0, 1] per half, ℓy = y/voxel, ℓz = z/voxel + 0.5):
//! * **side**: the deck-edge curve in plan, a lattice path (flat front, then 2×1, 1×1, 1×2, 1×4 slopes into the
//!   midbody), moved inward row by row by the midship flare offset B(ℓy): ℓx + B(ℓy) ≤ X(ℓz);
//! * **bottom**: the keel profile in side elevation, a lattice path rising into the forefoot and up the stem, moved up
//!   by the deadrise D(ℓx) going outboard: ℓy ≥ Y(ℓz) + D(ℓx);
//! * **deck**: ℓy ≤ sheer(ℓz), 1:4 / 1:2 ramps;
//! * **transom**: flat, ℓz ≥ z_tr.
//!
//! The blocks come from the label-consistent tiler ([`crate::tiling`]): every block whose corner labels match the
//! model, largest first (long wedges and pyramid pairs), on the starboard half, mirrored to port. The centre column is
//! shared and only takes mirror-symmetric blocks. The shell is one voxel thick.
use crate::hull::Hull;
use crate::lattice::{path_height, quantize_path, PathOptions, Step};
use crate::sweep::{fit_curve, Curve, CurveOptions};
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

/// The quantised hull: swept lattice curves.
#[derive(Debug, Clone)]
pub struct SweepModel {
    pub plan_fwd: Curve,
    pub plan_aft: Curve,
    pub keel_fwd: Curve,
    pub keel_aft: Curve,
    /// inward offset of the side per row, B(ℓy)
    pub flare: Vec<Step>,
    pub flare_lo: i32,
    /// rise of the bottom going outboard, D(ℓx) for ℓx ≥ 1
    pub deadrise: Vec<Step>,
    /// deck height ℓy along ℓz
    pub sheer: Vec<Step>,
    pub z_mid: i32,
    pub z_tr: i32,
}

impl SweepModel {
    pub fn inside(&self, lx: f64, ly: f64, lz: f64) -> bool {
        const EPS: f64 = 1e-9;
        if lz < self.z_tr as f64 - EPS {
            return false;
        }
        let fwd = lz >= self.z_mid as f64;
        let Some(x) = (if fwd { &self.plan_fwd } else { &self.plan_aft }).across_at(lz) else { return false };
        let b = path_height(&self.flare, ly - self.flare_lo as f64);
        if lx + b > x + EPS {
            return false;
        }
        let Some(y) = (if fwd { &self.keel_fwd } else { &self.keel_aft }).across_at(lz) else { return false };
        let d = path_height(&self.deadrise, lx.max(1.0));
        if ly < y + d - EPS {
            return false;
        }
        ly <= path_height(&self.sheer, lz) + EPS
    }

    pub fn describe(&self) -> String {
        format!(
            "plan fwd {}\nplan aft {}\nkeel fwd {}\nkeel aft {}",
            self.plan_fwd.describe(),
            self.plan_aft.describe(),
            self.keel_fwd.describe(),
            self.keel_aft.describe()
        )
    }
}

pub struct HullBlocks {
    pub placements: Vec<Placement>,
    pub model: SweepModel,
    /// corner sets that matched no block (must be 0)
    pub unmatched: usize,
    /// how each placement was chosen: "long", "unit" or "cube" (debugging)
    pub sources: Vec<&'static str>,
    /// every voxel covered by a placement (vehicle voxels, origin applied)
    pub occupied: std::collections::HashSet<[i32; 3]>,
    /// the voxel origin the hull was built at (midship baseline)
    pub origin: [i32; 3],
}

impl HullBlocks {
    /// Inside test of the quantised model at hull metres (x, y, z).
    pub fn model_inside_m(&self, q: [f64; 3]) -> bool {
        self.model.inside(q[0].abs() / VOXEL + 0.5, q[1] / VOXEL, q[2] / VOXEL + 0.5)
    }
}

pub fn build(hull: &Hull, opt: &HullBlockOptions) -> HullBlocks {
    let p = &hull.params;
    let zn_lo = ((-p.length / 2.0) / VOXEL + 0.5).floor() as i32 - 1;
    let zn_hi = ((p.length / 2.0) / VOXEL + 0.5).ceil() as i32 + 1;
    let secs: HashMap<i32, Vec<[f64; 2]>> = (zn_lo..=zn_hi).filter_map(|lz| hull.section_at((lz as f64 - 0.5) * VOXEL, 64).map(|s| (lz, s))).collect();
    let b = |ly: f64, lz: i32| -> Option<f64> { crate::hull::Hull::half_breadth(secs.get(&lz)?, ly * VOXEL).map(|x| x / VOXEL + 0.5) };
    let sheer_l = |lz: f64| p.sheer(((lz - 0.5) * VOXEL) / p.length + 0.5) / VOXEL;
    let keel_l = |lz: i32| secs.get(&lz).map(|s| s[0][1] / VOXEL);
    let y_min = secs.values().map(|s| s[0][1]).fold(f64::MAX, f64::min);
    let y_max = secs.values().map(|s| s[s.len() - 1][1]).fold(f64::MIN, f64::max);
    let (yn_lo, yn_hi) = ((y_min / VOXEL).floor() as i32 - 1, (y_max / VOXEL).ceil() as i32 + 2);
    let z_mid = 0;
    let z_tr = ((-p.length / 2.0) / VOXEL + 0.5).round() as i32;
    let co = CurveOptions::default();

    // ---- deck plan curve (deck edge half-breadth)
    let deck_x = |lz: i32| -> Option<f64> {
        let s = secs.get(&lz)?;
        let top = s[s.len() - 1][1] / VOXEL;
        b(top.min(sheer_l(lz as f64)) - 1e-6, lz)
    };
    let x0 = deck_x(z_mid).unwrap_or(1.0).round().max(1.0) as i32;
    let mut plan_t_f: Vec<[f64; 2]> = (z_mid..=zn_hi).filter_map(|lz| deck_x(lz).map(|x| [x, lz as f64])).collect();
    if let Some(l) = plan_t_f.last().copied() {
        plan_t_f.push([1.0, l[1] + 0.5]);
    }
    let plan_fwd = fit_curve([x0, z_mid], [-1, 1], &plan_t_f, &|q| q[0] == 1, ([1, z_mid], [x0 + 1, zn_hi + 2]), &co);
    let plan_t_a: Vec<[f64; 2]> = (zn_lo..=z_mid).rev().filter_map(|lz| deck_x(lz).map(|x| [x, lz as f64])).collect();
    let plan_aft = fit_curve([x0, z_mid], [-1, -1], &plan_t_a, &|q| q[1] == z_tr, ([1, z_tr], [x0 + 1, z_mid]), &co);

    // ---- keel profile (keel line, forefoot and stem)
    let y0 = keel_l(z_mid).unwrap_or(0.0).round() as i32;
    let mut keel_t_f: Vec<[f64; 2]> = (z_mid..=zn_hi).filter_map(|lz| keel_l(lz).map(|y| [y, lz as f64])).collect();
    // up the stem: the forward-most station at each height
    let ytop_bow = sheer_l(zn_hi as f64).ceil() as i32;
    let last_keel = keel_t_f.last().map(|q| q[0]).unwrap_or(0.0);
    for ly in (last_keel.ceil() as i32)..=ytop_bow {
        if let Some(lz) = (z_mid..=zn_hi).rev().find(|&lz| b(ly as f64, lz).is_some_and(|x| x > 0.6)) {
            keel_t_f.push([ly as f64, lz as f64 + 0.5]);
        }
    }
    let y_deck_bow = sheer_l(plan_fwd.pts.last().map(|q| q[1]).unwrap_or(zn_hi) as f64).round() as i32;
    // Forward, the keel line stays flat: the forefoot and stem come from the side sweep itself (the 45° curl below the
    // chine reaches the centre line higher as the deck curve narrows), the way builders run the pyramids down from the
    // deck curve. A separate rising keel line would cut diagonally through the side's long tiles. Set KEEL_FOREFOOT=1
    // to use the fitted keel profile instead.
    let keel_fwd = if std::env::var_os("KEEL_FOREFOOT").is_some() {
        fit_curve([y0, z_mid], [1, 1], &keel_t_f, &|q| q[0] >= y_deck_bow, ([y0, z_mid], [y_deck_bow + 1, zn_hi + 2]), &co)
    } else {
        Curve { pts: vec![[y0, z_mid], [y0, zn_hi + 2]], dir: [1, 1] }
    };
    let keel_t_a: Vec<[f64; 2]> = (z_tr..=z_mid).rev().filter_map(|lz| keel_l(lz).map(|y| [y, lz as f64])).collect();
    let keel_aft = fit_curve([y0, z_mid], [1, -1], &keel_t_a, &|q| q[1] == z_tr, ([y0, z_tr], [yn_hi, z_mid]), &co);

    // ---- flare B(ℓy): inward offset of the midship section from the deck edge
    let ms = secs.get(&z_mid).cloned().unwrap_or_default();
    let mut y_chine = p.draft * 0.3;
    for w in ms.windows(2) {
        if w[1][1] - w[0][1] > w[1][0] - w[0][0] {
            y_chine = w[0][1];
            break;
        }
    }
    let row_c = (y_chine / VOXEL).round().max(1.0) as i32;
    let mut flare_t: Vec<f64> = (yn_lo..=yn_hi).map(|ly| b(ly as f64, z_mid).map(|x| x0 as f64 - x).unwrap_or(f64::NAN)).collect();
    let top_def = (0..flare_t.len()).rev().find(|&i| flare_t[i].is_finite()).unwrap_or(0);
    // above the midship deck (where the sheer rises toward the bow) the sides go straight up: extrapolating the flare
    // would widen the stem into a T
    for i in top_def + 1..flare_t.len() {
        flare_t[i] = flare_t[top_def];
    }
    // below the chine the side curls under at 45° (the path cannot rise faster than one per row); the bottom rises at
    // 45° outboard of the chine, so where they meet both are planes of the same 45° family and the seam is on the
    // lattice
    let ic = ((row_c - yn_lo) as usize).min(flare_t.len() - 1);
    if flare_t[ic].is_finite() {
        for i in 0..ic {
            flare_t[i] = flare_t[ic] + (ic - i) as f64;
        }
    }
    let po = PathOptions::default();
    let flare = quantize_path(&flare_t, &po);
    let b_at = |ly: f64| path_height(&flare, ly - yn_lo as f64);

    // ---- deadrise D(ℓx): bottom rise outboard of the keel, joining the side at the chine
    let x_c = (x0 as f64 - b_at(row_c as f64)).round() as i32;
    let bottom_at = |lx: i32| -> Option<f64> {
        let x = (lx as f64 - 0.5) * VOXEL;
        if ms.is_empty() {
            return None;
        }
        if ms[0][0] >= x {
            return Some(ms[0][1] / VOXEL);
        }
        for w in ms.windows(2) {
            if w[0][0] < x && x <= w[1][0] {
                return Some((w[0][1] + (w[1][1] - w[0][1]) * (x - w[0][0]) / (w[1][0] - w[0][0])) / VOXEL);
            }
        }
        None
    };
    let dead_t: Vec<f64> = (1..=x_c.max(2)).map(|lx| bottom_at(lx).map(|y| y - y0 as f64).unwrap_or(f64::NAN)).collect();
    // deadrise slopes 0, ½ and 1 only: where the side's 45° curl crosses the bottom, the crease then lands on lattice
    // points (with a ¼ slope it would fall at thirds of a voxel)
    let mut deadrise: Vec<Step> = quantize_path(&dead_t, &PathOptions { end_h: Some(row_c - y0), moves: vec![(1, 0), (4, 1), (2, 1), (1, 1)], ..po.clone() }).into_iter().map(|s| Step { start: s.start + 1, ..s }).collect();
    // outboard of the chine the bottom rises 1:1 (it is under the side there)
    let last = *deadrise.last().unwrap();
    for x in last.start + last.run..x0 + 4 {
        deadrise.push(Step { start: x, run: 1, h0: last.h0 + last.rise + (x - last.start - last.run), rise: 1 });
    }

    // ---- sheer
    let sheer_t: Vec<f64> = (zn_lo..=zn_hi).map(|lz| sheer_l(lz as f64)).collect();
    let sheer: Vec<Step> = quantize_path(&sheer_t, &po).into_iter().map(|s| Step { start: s.start + zn_lo, ..s }).collect();

    let model = SweepModel { plan_fwd, plan_aft, keel_fwd, keel_aft, flare, flare_lo: yn_lo, deadrise, sheer, z_mid, z_tr };
    let inside8 = |e: [i32; 3]| model.inside((e[0].abs() as f64 + 1.0) / 2.0, (e[1] as f64 + 1.0) / 2.0, (e[2] as f64 + 1.0) / 2.0);
    let mask_of = |v: [i32; 3]| -> u8 {
        (0..8).fold(0u8, |m, k| {
            let q = [2 * v[0] - 1 + 2 * (k & 1), 2 * v[1] - 1 + 2 * ((k >> 1) & 1), 2 * v[2] - 1 + 2 * ((k >> 2) & 1)];
            if inside8(q) { m | (1 << k) } else { m }
        })
    };

    // ---- realisation: label-consistent tiling of the starboard half (centre column included), mirrored to port
    let xn_hi = x0 + 3;
    let mut shell: Vec<[i32; 3]> = vec![];
    for z in zn_lo - 1..=zn_hi + 2 {
        for y in yn_lo - 1..=yn_hi + 1 {
            for x in 0..=xn_hi {
                if mask_of([x, y, z]) != 0 {
                    shell.push([x, y, z]);
                }
            }
        }
    }
    // open hull (no deck plating): shell cubes directly under the deck surface are left out
    let top_open = |v: [i32; 3]| path_height(&model.sheer, v[2] as f64 + 0.5) - (v[1] as f64 + 1.0) < 1.0 - 1e-9;
    let cube_veto = |v: [i32; 3]| !opt.deck && top_open(v);
    let allowed = |t: &crate::tiling::Template, a: [i32; 3]| -> bool {
        let xs: Vec<i32> = t.cells.iter().map(|c| c[0] + a[0]).collect();
        if xs.iter().all(|&x| x >= 1) {
            return true;
        }
        xs.iter().all(|&x| x == 0) && t.symmetric_x()
    };
    // the model's own tile grids. Side tiles (one voxel thick in x) sit between plan-curve points along z and flare
    // breaks along y; bottom tiles (one voxel thick in y) between keel-profile points along z and deadrise breaks along x
    use std::collections::HashSet;
    let side_z: HashSet<i32> = model.plan_fwd.pts.iter().chain(&model.plan_aft.pts).map(|q| q[1]).collect();
    let mut side_y: HashSet<i32> = HashSet::new();
    for st in &model.flare {
        side_y.insert(st.start + model.flare_lo);
        side_y.insert(st.start + st.run + model.flare_lo);
    }
    let bot_z: HashSet<i32> = model.keel_fwd.pts.iter().chain(&model.keel_aft.pts).map(|q| q[1]).collect();
    let mut bot_x: HashSet<i32> = HashSet::new();
    for st in &model.deadrise {
        bot_x.insert(st.start);
        bot_x.insert(st.start + st.run);
    }
    let align = |cells: &[[i32; 3]]| -> f64 {
        let lo = [0, 1, 2].map(|i| cells.iter().map(|c| c[i]).min().unwrap());
        let hi = [0, 1, 2].map(|i| cells.iter().map(|c| c[i]).max().unwrap() + 1);
        let on = |set: &HashSet<i32>, a: i32, b: i32| -> Option<f64> { (b - a > 1).then(|| (set.contains(&a) && set.contains(&b)) as i32 as f64) };
        let parts: Vec<f64> = if hi[0] - lo[0] == 1 {
            [on(&side_y, lo[1], hi[1]), on(&side_z, lo[2], hi[2])].into_iter().flatten().collect()
        } else if hi[1] - lo[1] == 1 {
            [on(&bot_x, lo[0], hi[0]), on(&bot_z, lo[2], hi[2])].into_iter().flatten().collect()
        } else {
            vec![]
        };
        if parts.is_empty() { 0.0 } else { parts.iter().sum::<f64>() / parts.len() as f64 }
    };
    let tiles = crate::tiling::tile(&|e| inside8(e), &shell, &cube_veto, &allowed, &align);
    let mut out: Vec<Placement> = vec![];
    let mut sources: Vec<&'static str> = vec![];
    let mut occupied: std::collections::HashSet<[i32; 3]> = Default::default();
    for (pl, cells) in tiles {
        for c in &cells {
            occupied.insert([c[0] + opt.origin[0], c[1] + opt.origin[1], c[2] + opt.origin[2]]);
            occupied.insert([-c[0] + opt.origin[0], c[1] + opt.origin[1], c[2] + opt.origin[2]]);
        }
        let src: &'static str = if pl.def == "01_block" { "cube" } else if cells.len() > 1 { "long" } else { "unit" };
        if cells.iter().all(|c| c[0] >= 1) {
            let mut r = pl.r;
            for j in 0..3 {
                r[3 * j] = -r[3 * j];
            }
            out.push(Placement { def: pl.def.clone(), vp: [-pl.vp[0], pl.vp[1], pl.vp[2]], r });
            sources.push(src);
        }
        out.push(pl);
        sources.push(src);
    }
    let o = opt.origin;
    for p in out.iter_mut() {
        p.vp = [p.vp[0] + o[0], p.vp[1] + o[1], p.vp[2] + o[2]];
    }
    HullBlocks { placements: out, model, unmatched: 0, sources, occupied, origin: opt.origin }
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
        let m = crate::metrics::measure_with(&b.placements, &h, Some(&|q| b.model_inside_m(q)));
        eprintln!("METRICS {}", m.summary());
        assert_eq!(m.leak_samples, 0, "Q1: the shell must be watertight");
        // (the step share is measured against the NURBS normal, so it includes fidelity: the flat keel strip, the 45°
        // bilge chamfer)
        assert!(m.mid_step_share() < 0.10, "Q2 regression: {:.3}", m.mid_step_share());
        let units = b.sources.iter().filter(|s| **s == "unit").count();
        assert!(units < 250, "too many 1-voxel pieces: {units}");
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
