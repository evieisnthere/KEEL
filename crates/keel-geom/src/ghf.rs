//! Generalised height fields (docs/engineering/hull-to-blocks.md §2.4).
//!
//! A height field h(u, v) over a rectilinear grid of nodes (runs 1, 2 or 4 along each axis) with integer node heights.
//! A tile (one grid cell) is **valid** when it is exactly one block per height layer:
//! * planar, with a rise of at most 1 along each edge: flat, wedge (1 × L) or pyramid / inverse-pyramid pair
//!   (box 1 × Lu × Lv);
//! * a crease: all corners in one layer, one corner raised (a pyramid Lu × Lv) or one lowered (an inverse pyramid).
//!
//! Neighbouring tiles share node heights, so the surface is continuous: no steps anywhere inside a field. A hull is
//! the intersection of several fields (side, bottom, bow face, transom, deck). Each is fitted as a tight envelope of
//! the NURBS hull, so a field never cuts into the hull where another field is the active surface.
use crate::lattice::{to_world8, voxel_of, Frame, ALL_BTS};
use crate::voxel::Placement;

#[derive(Debug, Clone)]
pub struct Ghf {
    pub u: Vec<i32>,
    pub v: Vec<i32>,
    /// node heights, index i * nv + j
    pub h: Vec<i32>,
}

/// Tile corner order: a = (u0, v0), b = (u1, v0), c = (u0, v1), d = (u1, v1).
pub fn tile_ok(a: i32, b: i32, c: i32, d: i32) -> bool {
    if (b - a).abs() > 1 || (c - a).abs() > 1 || (d - b).abs() > 1 || (d - c).abs() > 1 {
        return false;
    }
    let tw = d - c - b + a;
    if tw == 0 {
        return true;
    }
    if tw.abs() != 1 {
        return false;
    }
    a.max(b).max(c).max(d) - a.min(b).min(c).min(d) == 1
}

/// Crease diagonal: true = split along b–c (the odd corner is a or d), false = along a–d.
fn split_bc(a: i32, b: i32, c: i32, d: i32) -> bool {
    if d - c - b + a == 0 {
        return true;
    }
    let vals = [a, b, c, d];
    let odd = (0..4).find(|&k| vals.iter().filter(|&&x| x == vals[k]).count() == 1).unwrap_or(0);
    odd == 0 || odd == 3
}

impl Ghf {
    pub fn nu(&self) -> usize {
        self.u.len()
    }
    pub fn nv(&self) -> usize {
        self.v.len()
    }
    pub fn at(&self, i: usize, j: usize) -> i32 {
        self.h[i * self.v.len() + j]
    }

    /// Surface height at (pu, pv). Outside the grid the edge values continue.
    pub fn eval(&self, pu: f64, pv: f64) -> f64 {
        let find = |nodes: &[i32], p: f64| -> (usize, f64) {
            let n = nodes.len();
            if n < 2 {
                return (0, 0.0);
            }
            let mut i = 0;
            while i + 2 < n && p >= nodes[i + 1] as f64 {
                i += 1;
            }
            let t = ((p - nodes[i] as f64) / (nodes[i + 1] - nodes[i]) as f64).clamp(0.0, 1.0);
            (i, t)
        };
        let (i, s) = find(&self.u, pu);
        let (j, t) = find(&self.v, pv);
        if self.u.len() < 2 || self.v.len() < 2 {
            return self.h.first().copied().unwrap_or(0) as f64;
        }
        let (a, b, c, d) = (self.at(i, j) as f64, self.at(i + 1, j) as f64, self.at(i, j + 1) as f64, self.at(i + 1, j + 1) as f64);
        if split_bc(a as i32, b as i32, c as i32, d as i32) {
            if s + t <= 1.0 {
                a + s * (b - a) + t * (c - a)
            } else {
                d + (1.0 - s) * (c - d) + (1.0 - t) * (b - d)
            }
        } else if s >= t {
            a + s * (b - a) + t * (d - b)
        } else {
            a + t * (c - a) + s * (d - c)
        }
    }

    /// Number of invalid tiles (must be 0 after fitting).
    pub fn invalid_tiles(&self) -> usize {
        let mut n = 0;
        for i in 0..self.nu().saturating_sub(1) {
            for j in 0..self.nv().saturating_sub(1) {
                if !tile_ok(self.at(i, j), self.at(i + 1, j), self.at(i, j + 1), self.at(i + 1, j + 1)) {
                    n += 1;
                }
            }
        }
        n
    }
}

/// One block cut from a field: placement, the voxels it fills, and its solid vertices (eighth-metre units).
pub struct Piece {
    pub pl: Placement,
    pub cells: Vec<[i32; 3]>,
    pub pts: Vec<[i32; 3]>,
}

/// Cut every tile of `g` into its per-layer blocks. Full layers (whole boxes) are left out: the caller fills the
/// shell behind the surface. `solid_below`: solid where h ≤ H (else h ≥ H). `skip_tile(i, j)` filters tiles.
pub fn fill(g: &Ghf, frame: &Frame, solid_below: bool, skip_tile: &dyn Fn(usize, usize) -> bool) -> (Vec<Piece>, usize) {
    let mut out = vec![];
    let mut unmatched = 0;
    let mut cache: std::collections::HashMap<Vec<[i32; 3]>, Option<(&'static str, [i32; 9], [i32; 3])>> = Default::default();
    for i in 0..g.nu().saturating_sub(1) {
        for j in 0..g.nv().saturating_sub(1) {
            if skip_tile(i, j) {
                continue;
            }
            let (a, b, c, d) = (g.at(i, j), g.at(i + 1, j), g.at(i, j + 1), g.at(i + 1, j + 1));
            if !tile_ok(a, b, c, d) {
                unmatched += 1;
                continue;
            }
            // a wedge extruded across a run > 1 is one wedge block per slice along the flat direction
            let (tu0, tu1, tv0, tv1) = (g.u[i], g.u[i + 1], g.v[j], g.v[j + 1]);
            let mut subs: Vec<(i32, i32, i32, i32)> = vec![];
            if a == c && b == d && tv1 - tv0 > 1 {
                subs.extend((tv0..tv1).map(|vv| (tu0, tu1, vv, vv + 1)));
            } else if a == b && c == d && tu1 - tu0 > 1 {
                subs.extend((tu0..tu1).map(|uu| (uu, uu + 1, tv0, tv1)));
            } else {
                subs.push((tu0, tu1, tv0, tv1));
            }
            for (u0, u1, v0, v1) in subs {
            let corners = [(u0, v0, a), (u1, v0, b), (u0, v1, c), (u1, v1, d)];
            let tris: [[usize; 3]; 2] = if split_bc(a, b, c, d) { [[0, 1, 2], [1, 2, 3]] } else { [[0, 1, 3], [0, 2, 3]] };
            let (mn, mx) = (a.min(b).min(c).min(d), a.max(b).max(c).max(d));
            for k in mn..mx {
                let mut pts: Vec<[i32; 3]> = vec![];
                for tri in &tris {
                    let mut tp = vec![];
                    for &ci in tri {
                        let (uu, vv, hh) = corners[ci];
                        let (bot, top) = if solid_below { (hh >= k, hh > k) } else { (hh <= k, hh <= k + 1) };
                        if bot {
                            tp.push(to_world8(frame, k, uu, vv));
                        }
                        if top {
                            tp.push(to_world8(frame, k + 1, uu, vv));
                        }
                    }
                    if tp.len() >= 4 {
                        pts.extend(tp);
                    }
                }
                pts.sort();
                pts.dedup();
                if pts.len() < 4 || pts.len() == 8 {
                    continue;
                }
                let m = *cache.entry(pts.clone()).or_insert_with(|| crate::facets::match_block_pub(&ALL_BTS, &pts));
                match m {
                    Some((def, r, anchor)) => {
                        let cells = (u0..u1).flat_map(|uu| (v0..v1).map(move |vv| (uu, vv))).map(|(uu, vv)| voxel_of(frame, k, uu, vv)).collect();
                        out.push(Piece { pl: Placement { def: def.into(), vp: anchor, r }, cells, pts });
                    }
                    None => unmatched += 1,
                }
            }
            }
        }
    }
    (out, unmatched)
}

/// Grid breaks over [lo, hi]: run 4 where the demanded slope stays ≤ 0.3, run 2 where ≤ 0.6, else 1.
/// `demand(p)` = steepest active slope between lattice positions p and p + 1.
pub fn breaks(lo: i32, hi: i32, demand: &dyn Fn(i32) -> f64) -> Vec<i32> {
    let mut out = vec![lo];
    let mut p = lo;
    while p < hi {
        let span = |n: i32| (p..p + n).map(|q| demand(q)).fold(0.0, f64::max);
        let run = if p + 4 <= hi && span(4) <= 0.3 {
            4
        } else if p + 2 <= hi && span(2) <= 0.6 {
            2
        } else {
            1
        };
        p += run;
        out.push(p);
    }
    out
}

#[derive(Debug, Clone)]
pub struct FitOptions {
    /// envelope from above (the field must stay ≥ the target: solid below) or from below
    pub upper: bool,
    /// cost of changing the u-rise between neighbouring tiles
    pub u_change: f64,
    /// cost of changing the v-rise between neighbouring row pairs
    pub v_change: f64,
    /// cost of a crease tile (single pyramid / inverse pyramid)
    pub crease: f64,
    /// extra cost when consecutive crease tiles along a row twist in opposite directions (saw teeth)
    pub zigzag: f64,
    /// lowest allowed height
    pub hmin: Option<i32>,
    /// the first u tile is flat (centre column, shared by both halves)
    pub flat_first_u: bool,
}

impl Default for FitOptions {
    fn default() -> Self {
        FitOptions { upper: true, u_change: 1.5, v_change: 1.0, crease: 0.6, zigzag: 4.0, hmin: None, flat_first_u: false }
    }
}

/// Height inside one tile at local (s, t) ∈ [0, 1]², following the tile's crease diagonal.
fn tile_h(a: i32, b: i32, c: i32, d: i32, s: f64, t: f64) -> f64 {
    let (af, bf, cf, df) = (a as f64, b as f64, c as f64, d as f64);
    if split_bc(a, b, c, d) {
        if s + t <= 1.0 {
            af + s * (bf - af) + t * (cf - af)
        } else {
            df + (1.0 - s) * (cf - df) + (1.0 - t) * (bf - df)
        }
    } else if s >= t {
        af + s * (bf - af) + t * (df - bf)
    } else {
        af + t * (cf - af) + s * (df - cf)
    }
}

/// Lattice-resolution envelope of `target` over [u0, u1] × [v0, v1]: the tightest function with slope ≤ 1 per
/// lattice step that stays on the solid's outside of every defined target value. Returns (envelope, active), where
/// active marks points where the envelope is the target itself (this field is the hull surface there).
pub fn envelope(u0: i32, u1: i32, v0: i32, v1: i32, target: &dyn Fn(i32, i32) -> Option<f64>, upper: bool) -> (Vec<f64>, Vec<bool>) {
    let (nu, nv) = ((u1 - u0 + 1) as usize, (v1 - v0 + 1) as usize);
    let t: Vec<Option<f64>> = (0..nu * nv).map(|k| target(u0 + (k / nv) as i32, v0 + (k % nv) as i32)).collect();
    let mut e = t.clone();
    let better = |a: f64, b: f64| if upper { a > b + 1e-12 } else { a < b - 1e-12 };
    loop {
        let mut changed = false;
        for pass in 0..2 {
            let order: Vec<usize> = if pass == 0 { (0..nu * nv).collect() } else { (0..nu * nv).rev().collect() };
            for k in order {
                let (i, j) = (k / nv, k % nv);
                for (a, b) in [(i.wrapping_sub(1), j), (i + 1, j), (i, j.wrapping_sub(1)), (i, j + 1)] {
                    if a >= nu || b >= nv {
                        continue;
                    }
                    if let Some(x) = e[a * nv + b] {
                        let cand = if upper { x - 1.0 } else { x + 1.0 };
                        if e[k].is_none_or(|y| better(cand, y)) {
                            e[k] = Some(cand);
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    let active = (0..nu * nv).map(|k| matches!((t[k], e[k]), (Some(a), Some(b)) if (a - b).abs() < 0.25)).collect();
    (e.into_iter().map(|x| x.unwrap_or(0.0)).collect(), active)
}

/// Fit integer node heights to `target`, row by row (rows = v nodes) with a DP along u. Every lattice point of every
/// tile is scored against the envelope: crossing it (cutting into the hull) is expensive; elsewhere the cost is the
/// squared distance, weighted down where another field is the surface. Every row stays valid against the previous
/// one, so the result has no invalid tiles.
pub fn fit(u: Vec<i32>, v: Vec<i32>, target: &dyn Fn(i32, i32) -> Option<f64>, opt: &FitOptions) -> Ghf {
    let (nu, nv) = (u.len(), v.len());
    let (ul, vl) = (u[0], v[0]);
    let (mut e, act) = envelope(ul, u[nu - 1], vl, v[nv - 1], target, opt.upper);
    let lnv = (v[nv - 1] - vl + 1) as usize;
    let lk = |pu: i32, pv: i32| (pu - ul) as usize * lnv + (pv - vl) as usize;
    let truth: Vec<Option<f64>> = (0..e.len()).map(|k| target(ul + (k / lnv) as i32, vl + (k % lnv) as i32)).collect();
    // node-level envelope: a field can change by at most 1 per tile edge, so relax the node values with that limit
    // (otherwise an inactive edge row could start too far inside and the later rows could never catch up)
    {
        let mut en: Vec<f64> = (0..nu * nv).map(|k| e[lk(u[k / nv], v[k % nv])]).collect();
        loop {
            let mut changed = false;
            for k in 0..nu * nv {
                let (i, j) = (k / nv, k % nv);
                for (a, b) in [(i.wrapping_sub(1), j), (i + 1, j), (i, j.wrapping_sub(1)), (i, j + 1)] {
                    if a >= nu || b >= nv {
                        continue;
                    }
                    let x = en[a * nv + b];
                    let cand = if opt.upper { x - 1.0 } else { x + 1.0 };
                    if (opt.upper && cand > en[k] + 1e-12) || (!opt.upper && cand < en[k] - 1e-12) {
                        en[k] = cand;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for k in 0..nu * nv {
            e[lk(u[k / nv], v[k % nv])] = en[k];
        }
    }
    let pc = |pu: i32, pv: i32, h: f64| -> f64 {
        let k = lk(pu, pv);
        let t = e[k];
        let d = h - t;
        let mut c = if act[k] { d * d } else { 0.05 * d * d };
        // crossing the true surface cuts into the hull
        if let Some(tt) = truth[k] {
            let gap = if opt.upper { tt - 0.5 - h } else { h - tt - 0.5 };
            if gap > 1e-9 {
                c += 20.0 * gap * gap + 5.0;
            }
        }
        c
    };
    let mut hmin = e.iter().cloned().fold(f64::MAX, f64::min).floor() as i32 - 2;
    let hmax = e.iter().cloned().fold(f64::MIN, f64::max).ceil() as i32 + 2;
    if let Some(m) = opt.hmin {
        hmin = hmin.max(m);
    }
    let nh = (hmax - hmin + 1).max(1) as usize;
    // cost of the lattice points strictly between rows ja < jb inside tile i
    let strip = |i: usize, ja: usize, jb: usize, a0: i32, a1: i32, b0: i32, b1: i32| -> f64 {
        let (u0, u1, v0, v1) = (u[i], u[i + 1], v[ja], v[jb]);
        let last = i + 2 == nu;
        let mut c = 0.0;
        for pu in u0..=u1 {
            if pu == u1 && !last {
                continue;
            }
            for pv in v0 + 1..v1 {
                let (s_, t_) = ((pu - u0) as f64 / (u1 - u0) as f64, (pv - v0) as f64 / (v1 - v0) as f64);
                c += pc(pu, pv, tile_h(a0, a1, b0, b1, s_, t_));
            }
        }
        c
    };
    // a crease in a long thin tile is a tall pointy pyramid: cost grows with the tile's aspect ratio
    let aspect = |i: usize, j: usize| -> f64 {
        let (a, b) = ((u[i + 1] - u[i]) as f64, (v[j + 1] - v[j]) as f64);
        (a.max(b) / a.min(b)).powi(2)
    };
    // DP for row j given optional neighbour rows (below = j - 1, above = j + 1) and the rows beyond them
    let solve_row = |j: usize, below: Option<&[i32]>, above: Option<&[i32]>, below2: Option<&[i32]>, above2: Option<&[i32]>| -> Option<Vec<i32>> {
        let vcost = |i: usize, hh: i32| -> f64 {
            let mut c = pc(u[i], v[j], hh as f64);
            if let Some(p) = below {
                if (hh - p[i]).abs() > 1 {
                    return f64::INFINITY;
                }
                if let Some(p2) = below2 {
                    if hh - p[i] != p[i] - p2[i] {
                        c += opt.v_change;
                    }
                }
            }
            if let Some(p) = above {
                if (hh - p[i]).abs() > 1 {
                    return f64::INFINITY;
                }
                if let Some(p2) = above2 {
                    if p[i] - hh != p2[i] - p[i] {
                        c += opt.v_change;
                    }
                }
                if let Some(q) = below {
                    if p[i] - hh != hh - q[i] {
                        c += opt.v_change;
                    }
                }
            }
            c
        };
        // state: arriving rise (0..3, 3 = start) × twist of the last tile against the row below (0..3) and above
        const NS: usize = 36;
        let idx = |i: usize, hh: i32, st: usize| (i * nh + (hh - hmin) as usize) * NS + st;
        let mut cost = vec![f64::INFINITY; nu * nh * NS];
        let mut back = vec![(0i32, 0usize); nu * nh * NS];
        for hh in hmin..=hmax {
            let c = vcost(0, hh);
            if c.is_finite() {
                cost[idx(0, hh, 3 * 9 + 4)] = c;
            }
        }
        for i in 0..nu - 1 {
            for hh in hmin..=hmax {
                for st in 0..NS {
                    let c = cost[idx(i, hh, st)];
                    if !c.is_finite() {
                        continue;
                    }
                    let (r, twb0, twa0) = (st / 9, (st / 3) % 3, st % 3);
                    for rr in 0..3usize {
                        let rise = rr as i32 - 1;
                        if opt.flat_first_u && i == 0 && rise != 0 {
                            continue;
                        }
                        let h2 = hh + rise;
                        if h2 < hmin || h2 > hmax {
                            continue;
                        }
                        let mut nc = c + vcost(i + 1, h2);
                        if !nc.is_finite() {
                            continue;
                        }
                        for q in u[i] + 1..u[i + 1] {
                            nc += pc(q, v[j], hh as f64 + rise as f64 * (q - u[i]) as f64 / (u[i + 1] - u[i]) as f64);
                        }
                        let (mut twb, mut twa) = (1usize, 1usize);
                        if let Some(p) = below {
                            if !tile_ok(p[i], p[i + 1], hh, h2) {
                                continue;
                            }
                            let tw = h2 - hh - p[i + 1] + p[i];
                            twb = (tw + 1) as usize;
                            if tw != 0 {
                                nc += opt.crease * aspect(i, j - 1);
                                // alternating creases along the row read as saw teeth
                                if twb0 != 1 && twb0 != twb {
                                    nc += opt.zigzag;
                                }
                            }
                            nc += strip(i, j - 1, j, p[i], p[i + 1], hh, h2);
                        }
                        if let Some(p) = above {
                            if !tile_ok(hh, h2, p[i], p[i + 1]) {
                                continue;
                            }
                            let tw = p[i + 1] - p[i] - h2 + hh;
                            twa = (tw + 1) as usize;
                            if tw != 0 {
                                nc += opt.crease * aspect(i, j);
                                if twa0 != 1 && twa0 != twa {
                                    nc += opt.zigzag;
                                }
                            }
                            nc += strip(i, j, j + 1, hh, h2, p[i], p[i + 1]);
                        }
                        if r != 3 && r != rr {
                            nc += opt.u_change;
                        }
                        let k = idx(i + 1, h2, rr * 9 + twb * 3 + twa);
                        if nc < cost[k] {
                            cost[k] = nc;
                            back[k] = (hh, st);
                        }
                    }
                }
            }
        }
        let mut best = (f64::INFINITY, hmin, 0usize);
        for hh in hmin..=hmax {
            for st in 0..NS {
                let c = cost[idx(nu - 1, hh, st)];
                if c < best.0 {
                    best = (c, hh, st);
                }
            }
        }
        if !best.0.is_finite() {
            return None;
        }
        let mut row = vec![0; nu];
        let (mut hh, mut st) = (best.1, best.2);
        for i in (0..nu).rev() {
            row[i] = hh;
            if i > 0 {
                let (ph, ps) = back[idx(i, hh, st)];
                hh = ph;
                st = ps;
            }
        }
        Some(row)
    };
    // start from the row with the most active points and grow outward, then refine every row against both neighbours
    let active_in_row = |j: usize| (u[0]..=u[nu - 1]).filter(|&pu| act[lk(pu, v[j])]).count();
    let j0 = (0..nv).max_by_key(|&j| active_in_row(j)).unwrap_or(0);
    let mut rows: Vec<Vec<i32>> = vec![vec![]; nv];
    rows[j0] = solve_row(j0, None, None, None, None).unwrap_or(vec![0; nu]);
    for j in j0 + 1..nv {
        let r = solve_row(j, Some(&rows[j - 1]), None, (j >= j0 + 2).then(|| rows[j - 2].as_slice()), None).unwrap_or(rows[j - 1].clone());
        rows[j] = r;
    }
    for j in (0..j0).rev() {
        let r = solve_row(j, None, Some(&rows[j + 1]), None, (j + 2 <= j0).then(|| rows[j + 2].as_slice()).or((j + 2 < nv).then(|| rows[j + 2].as_slice()))).unwrap_or(rows[j + 1].clone());
        rows[j] = r;
    }
    for _ in 0..3 {
        for j in 0..nv {
            let below = (j > 0).then(|| rows[j - 1].clone());
            let above = (j + 1 < nv).then(|| rows[j + 1].clone());
            let below2 = (j > 1).then(|| rows[j - 2].clone());
            let above2 = (j + 2 < nv).then(|| rows[j + 2].clone());
            if let Some(r) = solve_row(j, below.as_deref(), above.as_deref(), below2.as_deref(), above2.as_deref()) {
                rows[j] = r;
            }
        }
    }
    let mut h = vec![0i32; nu * nv];
    for i in 0..nu {
        for j in 0..nv {
            h[i * nv + j] = rows[j][i];
        }
    }
    Ghf { u, v, h }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_validity() {
        assert!(tile_ok(0, 0, 0, 0));
        assert!(tile_ok(0, 1, 0, 1)); // wedge
        assert!(tile_ok(0, 1, 1, 2)); // pyramid pair
        assert!(tile_ok(0, 0, 0, 1)); // pyramid crease
        assert!(tile_ok(0, 1, 1, 1)); // inverse pyramid crease
        assert!(!tile_ok(0, 1, 1, 0)); // saddle
        assert!(!tile_ok(0, 2, 0, 2));
    }

    #[test]
    fn every_valid_tile_fills_with_blocks() {
        let frame = Frame { h_axis: 0, u_axis: 2, v_axis: 1, solid_below: true, offset: [0, 0, 0], mirror_x: false };
        for runs in [(1, 1), (1, 2), (2, 4), (4, 4), (4, 1)] {
            for code in 0..81 {
                let hs = [code % 3, (code / 3) % 3, (code / 9) % 3, (code / 27) % 3].map(|x| x as i32 - 1 + 3);
                if !tile_ok(hs[0], hs[1], hs[2], hs[3]) {
                    continue;
                }
                let g = Ghf { u: vec![0, runs.0], v: vec![0, runs.1], h: vec![hs[0], hs[2], hs[1], hs[3]] };
                for below in [true, false] {
                    let (_, bad) = fill(&g, &frame, below, &|_, _| false);
                    assert_eq!(bad, 0, "runs {runs:?} heights {hs:?} below {below}");
                }
            }
        }
    }

    #[test]
    fn fit_has_no_invalid_tiles() {
        let u: Vec<i32> = (0..=20).collect();
        let v: Vec<i32> = (0..=10).collect();
        let g = fit(u, v, &|a, b| Some(5.0 + 0.3 * a as f64 + 0.05 * (b * b) as f64 - 0.02 * (a * b) as f64), &FitOptions::default());
        assert_eq!(g.invalid_tiles(), 0);
    }
}
