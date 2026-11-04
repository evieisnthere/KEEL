//! Lattice-surface construction for smooth block hulls (docs/engineering/hull-to-blocks.md §2.3).
//!
//! * [`quantize_path`]: dynamic programme that turns a 1D target curve into a lattice path of steps
//!   (run 1, 2 or 4; rise −1, 0, +1; flat steps have run 1). Cost = squared error + a penalty per change of step type,
//!   so long constant gradients win (smoothness over fidelity).
//! * [`fill_additive`]: an additive height field H(u, v) = c + A(u) + B(v) built from two quantised paths. Every tile
//!   (step of A × step of B) is planar and continuous with its neighbours. Each one-voxel layer of a tile is cut by the
//!   plane through box corners, and the solid part is matched exactly to a block (cube, wedge, pyramid or inverse
//!   pyramid of the right size).
use crate::voxel::Placement;
use std::collections::HashMap;

/// One step of a lattice path: from `start` over `run` positions, height `h0` → `h0 + rise`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub start: i32,
    pub run: i32,
    pub h0: i32,
    pub rise: i32,
}

impl Step {
    pub fn at(&self, p: f64) -> f64 {
        self.h0 as f64 + self.rise as f64 * (p - self.start as f64) / self.run as f64
    }
}

#[derive(Debug, Clone)]
pub struct PathOptions {
    /// cost of changing the step type (gradient) between consecutive steps
    pub change_penalty: f64,
    /// allowed (run, rise) steps
    pub moves: Vec<(i32, i32)>,
    /// hard constraint on the final height (used to join regions at a seam)
    pub end_h: Option<i32>,
}

impl Default for PathOptions {
    fn default() -> Self {
        PathOptions { change_penalty: 0.6, moves: vec![(1, 0), (1, 1), (1, -1), (2, 1), (2, -1), (4, 1), (4, -1)], end_h: None }
    }
}

/// Quantise `target[p]` (heights at integer positions p = 0..=n, lattice units) into a lattice path.
/// Returns the steps; their heights are integers at step ends. NaN targets are unconstrained positions.
pub fn quantize_path(target: &[f64], opt: &PathOptions) -> Vec<Step> {
    let n = target.len() as i32 - 1;
    if n <= 0 {
        return vec![];
    }
    let finite = || target.iter().cloned().filter(|v| v.is_finite()).chain(opt.end_h.map(|h| h as f64));
    let hmin = finite().fold(f64::MAX, f64::min).floor() as i32 - 2;
    let hmax = finite().fold(f64::MIN, f64::max).ceil() as i32 + 2;
    let sq = |d: f64| if d.is_finite() { d * d } else { 0.0 };
    let nh = (hmax - hmin + 1) as usize;
    let nm = opt.moves.len();
    // dp[p][h][m] = min cost to reach position p at height h, arriving with move m (nm = start)
    let idx = |p: i32, h: i32, m: usize| (p as usize * nh + (h - hmin) as usize) * (nm + 1) + m;
    let size = (n as usize + 1) * nh * (nm + 1);
    let mut cost = vec![f64::INFINITY; size];
    let mut back: Vec<(i32, i32, usize)> = vec![(-1, 0, 0); size];
    for h in hmin..=hmax {
        let e = sq(h as f64 - target[0]);
        cost[idx(0, h, nm)] = e;
    }
    for p in 0..n {
        for h in hmin..=hmax {
            for m in 0..=nm {
                let c = cost[idx(p, h, m)];
                if !c.is_finite() {
                    continue;
                }
                for (k, &(run, rise)) in opt.moves.iter().enumerate() {
                    let q = p + run;
                    let h2 = h + rise;
                    if q > n || h2 < hmin || h2 > hmax {
                        continue;
                    }
                    let mut e = 0.0;
                    for t in 1..=run {
                        let hh = h as f64 + rise as f64 * t as f64 / run as f64;
                        e += sq(hh - target[(p + t) as usize]);
                    }
                    let change = if m != nm && m != k { opt.change_penalty } else { 0.0 };
                    let nc = c + e + change;
                    let j = idx(q, h2, k);
                    if nc < cost[j] {
                        cost[j] = nc;
                        back[j] = (p, h, m);
                    }
                }
            }
        }
    }
    // best end state
    let mut best = (f64::INFINITY, 0, 0);
    for h in hmin..=hmax {
        if opt.end_h.is_some_and(|e| e != h) {
            continue;
        }
        for m in 0..=nm {
            let c = cost[idx(n, h, m)];
            if c < best.0 {
                best = (c, h, m);
            }
        }
    }
    let mut steps = Vec::new();
    let (mut p, mut h, mut m) = (n, best.1, best.2);
    while p > 0 {
        let (pp, ph, pm) = back[idx(p, h, m)];
        steps.push(Step { start: pp, run: p - pp, h0: ph, rise: h - ph });
        p = pp;
        h = ph;
        m = pm;
    }
    steps.reverse();
    steps
}

/// Height of a path at a lattice position.
pub fn path_height(steps: &[Step], p: f64) -> f64 {
    for s in steps {
        if p <= (s.start + s.run) as f64 + 1e-9 {
            return s.at(p.max(s.start as f64));
        }
    }
    steps.last().map(|s| (s.h0 + s.rise) as f64).unwrap_or(0.0)
}

/// The 16 standard block definitions with their solid type index (keel_model::geometry::block_points).
pub const ALL_BTS: [(i32, &str); 16] = [
    (0, "01_block"),
    (1, "02_wedge"),
    (2, "03_pyramid"),
    (3, "04_invpyramid"),
    (4, "05_wedge_2"),
    (5, "06_pyramid_2"),
    (6, "07_invpyramid_2"),
    (7, "08_wedge_4"),
    (8, "09_pyramid_4"),
    (9, "10_invpyramid_4"),
    (10, "11_pyramid_2x2"),
    (11, "12_pyramid_2x4"),
    (12, "13_pyramid_4x4"),
    (13, "14_invpyramid_2x2"),
    (14, "15_invpyramid_2x4"),
    (15, "16_invpyramid_4x4"),
];

/// Solid vertices of a placed block in eighth-metre units (e = 2ℓ − 1 lattice coordinates).
pub fn placement_points8(pl: &Placement) -> Vec<[i32; 3]> {
    let Some(&(bt, _)) = ALL_BTS.iter().find(|(_, d)| *d == pl.def) else { return vec![] };
    let r = pl.r;
    keel_model::geometry::block_points(bt)
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let p = p.map(|x| (x * 8.0).round() as i32);
            [0, 1, 2].map(|i| r[i] * p[0] + r[3 + i] * p[1] + r[6 + i] * p[2] + 2 * pl.vp[i])
        })
        .collect()
}

/// Axis mapping of a height-field region into the vehicle frame. Coordinates are lattice units: voxel i spans [i, i+1]
/// along each axis before `flip`. `flip` mirrors an axis about lattice 0 (voxel −1 ↔ voxel 0 edge), used for port.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub h_axis: usize,
    pub u_axis: usize,
    pub v_axis: usize,
    /// solid lies where h <= H (true) or h >= H (false)
    pub solid_below: bool,
    /// lattice origin offset per world axis (added after the axis mapping)
    pub offset: [i32; 3],
    /// mirror the world x axis (port side): world voxel = -1 - voxel
    pub mirror_x: bool,
}

/// Result of filling a region: placements plus the voxels each one occupies (for shell/overlap logic).
pub struct Filled {
    pub placements: Vec<Placement>,
    pub occupied: HashMap<[i32; 3], usize>,
    /// partial layers that matched no block (must stay 0)
    pub unmatched: usize,
}

pub(crate) fn to_world8(frame: &Frame, h: i32, u: i32, v: i32) -> [i32; 3] {
    // lattice coordinate ℓ → eighth-metre units: e = 2ℓ − 1 (voxel i spans e ∈ [2i−1, 2i+1])
    let mut w = [0i32; 3];
    w[frame.h_axis] = h;
    w[frame.u_axis] = u;
    w[frame.v_axis] = v;
    let mut e = [0, 1, 2].map(|i| 2 * (w[i] + frame.offset[i]) - 1);
    if frame.mirror_x {
        e[0] = -e[0];
    }
    e
}

pub(crate) fn voxel_of(frame: &Frame, h: i32, u: i32, v: i32) -> [i32; 3] {
    let mut w = [0i32; 3];
    w[frame.h_axis] = h;
    w[frame.u_axis] = u;
    w[frame.v_axis] = v;
    let mut c = [0, 1, 2].map(|i| w[i] + frame.offset[i]);
    if frame.mirror_x {
        c[0] = -c[0];
    }
    c
}

/// Fill an additive region H(u, v) = c + A(u) + B(v). `depth` = number of full cube layers behind the skin
/// (1 = a one-voxel shell). `keep` filters tiles (u0, v0) (e.g. domain limits).
pub fn fill_additive(frame: &Frame, c: i32, a: &[Step], b: &[Step], depth: i32, keep: &dyn Fn(i32, i32) -> bool) -> Filled {
    let all_bts: Vec<(i32, &'static str)> = ALL_BTS.to_vec();
    let mut out = Filled { placements: vec![], occupied: HashMap::new(), unmatched: 0 };
    let mut cache: HashMap<Vec<[i32; 3]>, Option<(&'static str, [i32; 9], [i32; 3])>> = HashMap::new();
    for sa in a {
        for sb in b {
            if !keep(sa.start, sb.start) {
                continue;
            }
            let (u0, u1, v0, v1) = (sa.start, sa.start + sa.run, sb.start, sb.start + sb.run);
            let hc = |u: i32, v: i32| c as f64 + sa.at(u as f64) + sb.at(v as f64);
            let corners = [hc(u0, v0), hc(u1, v0), hc(u0, v1), hc(u1, v1)];
            let (lo, hi) = corners.iter().fold((f64::MAX, f64::MIN), |m, x| (m.0.min(*x), m.1.max(*x)));
            let k_lo = lo.floor() as i32;
            let k_hi = hi.ceil() as i32;
            let on_solid_side = |hh: i32, uu: i32, vv: i32| {
                let plane = hc(uu, vv);
                if frame.solid_below { hh as f64 <= plane + 1e-9 } else { hh as f64 >= plane - 1e-9 }
            };
            let layer_cells = |k: i32| -> Vec<[i32; 3]> { (u0..u1).flat_map(|uu| (v0..v1).map(move |vv| (uu, vv))).map(|(uu, vv)| voxel_of(frame, k, uu, vv)).collect() };
            // partial layers [k, k+1] crossed by the plane
            for k in k_lo..k_hi {
                let mut solid_pts: Vec<[i32; 3]> = Vec::new();
                for &hh in &[k, k + 1] {
                    for &uu in &[u0, u1] {
                        for &vv in &[v0, v1] {
                            if on_solid_side(hh, uu, vv) {
                                solid_pts.push(to_world8(frame, hh, uu, vv));
                            }
                        }
                    }
                }
                if solid_pts.len() < 4 {
                    continue;
                }
                let cells = layer_cells(k);
                if solid_pts.len() == 8 {
                    for c in &cells {
                        out.occupied.insert(*c, out.placements.len());
                        out.placements.push(Placement { def: "01_block".into(), vp: *c, r: keel_io::vehicle::IDENTITY });
                    }
                    continue;
                }
                let mut key = solid_pts.clone();
                key.sort();
                let m = *cache.entry(key.clone()).or_insert_with(|| crate::facets::match_block_pub(&all_bts, &key));
                if let Some((def, r, anchor)) = m {
                    for c in &cells {
                        out.occupied.insert(*c, out.placements.len());
                    }
                    out.placements.push(Placement { def: def.into(), vp: anchor, r });
                } else {
                    out.unmatched += 1;
                }
            }
            // full layers behind the skin (toward the solid side)
            for d in 1..=depth {
                let k = if frame.solid_below { k_lo - d } else { k_hi + d - 1 };
                for cvox in layer_cells(k) {
                    if !out.occupied.contains_key(&cvox) {
                        out.occupied.insert(cvox, out.placements.len());
                        out.placements.push(Placement { def: "01_block".into(), vp: cvox, r: keel_io::vehicle::IDENTITY });
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_prefers_long_constant_gradients() {
        // a line of slope 0.25 must become 1x4 steps only
        let t: Vec<f64> = (0..=20).map(|p| 3.0 + p as f64 * 0.25).collect();
        let s = quantize_path(&t, &PathOptions::default());
        assert!(s.iter().all(|x| x.run == 4 && x.rise == 1), "{s:?}");
        // slope 0.5 → 1x2 steps
        let t: Vec<f64> = (0..=20).map(|p| p as f64 * 0.5).collect();
        let s = quantize_path(&t, &PathOptions::default());
        assert!(s.iter().all(|x| x.run == 2 && x.rise == 1), "{s:?}");
    }

    #[test]
    fn every_tile_combination_matches_a_block() {
        // tilted planes with all run combinations (1, 2, 4) x (1, 2, 4) and both rises: no tile may fail to match
        for ra in [1, 2, 4] {
            for rb in [1, 2, 4] {
                for (sa, sb) in [(1, 1), (1, -1), (-1, 1), (-1, -1), (0, 1), (1, 0)] {
                    let a: Vec<Step> = (0..4).map(|k| Step { start: k * if sa == 0 { 1 } else { ra }, run: if sa == 0 { 1 } else { ra }, h0: 10 + k * sa, rise: sa }).collect();
                    let b: Vec<Step> = (0..4).map(|k| Step { start: k * if sb == 0 { 1 } else { rb }, run: if sb == 0 { 1 } else { rb }, h0: k * sb, rise: sb }).collect();
                    for below in [true, false] {
                    for mirror in [false, true] {
                    let frame = Frame { h_axis: 0, u_axis: 1, v_axis: 2, solid_below: below, offset: [0, 0, 0], mirror_x: mirror };
                    let f = fill_additive(&frame, 0, &a, &b, 1, &|_, _| true);
                    assert_eq!(f.unmatched, 0, "unmatched tiles: runs {ra},{rb} rises {sa},{sb} below {below} mirror {mirror}");
                    let tiles = a.len() * b.len();
                    let partial = f.placements.iter().filter(|p| p.def != "01_block").count();
                    let expected_partial = if sa == 0 || sb == 0 { tiles } else { 2 * tiles };
                    assert_eq!(partial, expected_partial, "runs {ra},{rb} rises {sa},{sb}");
                    if sa != 0 && sb != 0 {
                        assert!(f.placements.iter().any(|p| p.def.contains("pyramid")));
                    }
                    }
                    }
                }
            }
        }
    }
}
