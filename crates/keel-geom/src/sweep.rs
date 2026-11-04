//! Swept lattice curves: how builders shape a hull (docs/engineering/hull-to-blocks.md §2.5).
//!
//! A builder first lays out the deck-edge curve in plan: from the front a few blocks of flat front, then 1×2, 1×1,
//! 1×2 and 1×4 slopes as the curve straightens into the midbody. The hull below follows that curve moved inward row by
//! row (pyramids "down from there"). The keel profile is treated the same way: the keel line in side elevation, rising
//! into the forefoot and stem, moved upward by the deadrise as it goes outboard.
//!
//! The curve is a monotone lattice path. Its steps are (across, along) ∈ {(0, 1), (1, 4), (1, 2), (1, 1), (2, 1),
//! (4, 1), (1, 0)}. Moving a segment by a per-row offset (0, ¼, ½ or 1 per row) sweeps a plane through lattice
//! points, which is a wedge, a pyramid pair or a flat face in almost every combination. The tiler then places the long
//! blocks.

/// Steps (across, along), ordered from flattest to steepest.
pub const STEPS: [(i32, i32); 7] = [(0, 1), (1, 4), (1, 2), (1, 1), (2, 1), (4, 1), (1, 0)];

#[derive(Debug, Clone)]
pub struct Curve {
    /// lattice points (across, along) from the start
    pub pts: Vec<[i32; 2]>,
    /// direction signs of (across, along)
    pub dir: [i32; 2],
}

#[derive(Debug, Clone)]
pub struct CurveOptions {
    /// cost of changing the step type
    pub change: f64,
    /// cost of the curve getting flatter again after getting steeper (the curve should steepen steadily)
    pub reverse: f64,
    /// allowed steps (indices into [`STEPS`]). By default slopes up to 1×1 plus the flat front: swept by any row offset
    /// (0, ¼, ½, 1 per row) those always give a tileable plane, while 2×1 and 4×1 swept by gentle offsets do not.
    pub allowed: Vec<usize>,
}

impl Default for CurveOptions {
    fn default() -> Self {
        CurveOptions { change: 0.4, reverse: 3.0, allowed: vec![0, 1, 2, 3, 4, 6] }
    }
}

fn seg_dist2(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (a[0] + t * dx - p[0], a[1] + t * dy - p[1]);
    qx * qx + qy * qy
}

/// Distance² from p to a polyline.
fn poly_dist2(p: [f64; 2], poly: &[[f64; 2]]) -> f64 {
    poly.windows(2).map(|w| seg_dist2(p, w[0], w[1])).fold(f64::MAX, f64::min)
}

/// Fit a monotone lattice path to `target` (a polyline in (across, along) lattice units, in walking order), from
/// `start`, moving by `dir` signs, until `end(point)` holds. The cost is the squared distance of the path to the
/// target (sampled along each step), the distance of the target to the path end, and the smoothness terms.
pub fn fit_curve(start: [i32; 2], dir: [i32; 2], target: &[[f64; 2]], end: &dyn Fn([i32; 2]) -> bool, bounds: ([i32; 2], [i32; 2]), opt: &CurveOptions) -> Curve {
    use std::collections::HashMap;
    let (lo, hi) = bounds;
    let inb = |p: [i32; 2]| p[0] >= lo[0] && p[0] <= hi[0] && p[1] >= lo[1] && p[1] <= hi[1];
    let step_cost = |p: [i32; 2], k: usize| -> f64 {
        let (da, db) = STEPS[k];
        let q = [p[0] + dir[0] * da, p[1] + dir[1] * db];
        let len = ((da * da + db * db) as f64).sqrt();
        let n = (len * 2.0).ceil().max(1.0) as usize;
        let mut c = 0.0;
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let s = [p[0] as f64 + (q[0] - p[0]) as f64 * t, p[1] as f64 + (q[1] - p[1]) as f64 * t];
            c += poly_dist2(s, target);
        }
        c * len / n as f64
    };
    // Dijkstra-like DP over (point, last step); the path is monotone so a sweep in order of (along, across) works,
    // but a priority-free relaxation over a sorted frontier is simpler: points only move forward.
    type Key = ([i32; 2], usize);
    let mut cost: HashMap<Key, f64> = HashMap::new();
    let mut back: HashMap<Key, Key> = HashMap::new();
    let start_key = (start, STEPS.len());
    cost.insert(start_key, 0.0);
    let mut order: Vec<[i32; 2]> = vec![];
    for a in lo[0]..=hi[0] {
        for b in lo[1]..=hi[1] {
            order.push([a, b]);
        }
    }
    // process points in walking order: further along (in dir) means later
    order.sort_by_key(|p| (dir[1] * p[1], dir[0] * p[0]));
    let mut best_end: Option<(f64, Key)> = None;
    let tail = target.last().copied().unwrap_or([0.0, 0.0]);
    for p in order {
        for last in 0..=STEPS.len() {
            let key = (p, last);
            let Some(&c) = cost.get(&key) else { continue };
            if end(p) {
                let e = c + 2.0 * ((p[0] as f64 - tail[0]).powi(2) + (p[1] as f64 - tail[1]).powi(2));
                if best_end.is_none_or(|(b, _)| e < b) {
                    best_end = Some((e, key));
                }
                continue;
            }
            for &k in &opt.allowed {
                let (da, db) = STEPS[k];
                let q = [p[0] + dir[0] * da, p[1] + dir[1] * db];
                if !inb(q) {
                    continue;
                }
                let mut nc = c + step_cost(p, k);
                if last < STEPS.len() && last != k {
                    nc += opt.change;
                    if k < last {
                        nc += opt.reverse;
                    }
                }
                let nk = (q, k);
                if cost.get(&nk).is_none_or(|&x| nc < x) {
                    cost.insert(nk, nc);
                    back.insert(nk, key);
                }
            }
        }
    }
    let mut pts = vec![];
    if let Some((_, mut k)) = best_end {
        loop {
            pts.push(k.0);
            match back.get(&k) {
                Some(&pk) => k = pk,
                None => break,
            }
        }
    }
    pts.reverse();
    if pts.is_empty() {
        pts.push(start);
    }
    Curve { pts, dir }
}

impl Curve {
    /// The curve's `across` value at position `along` (linear on each step; at a pure-across step the larger value
    /// toward the start). Before the start: the start value; past the end: None.
    pub fn across_at(&self, along: f64) -> Option<f64> {
        let s = self.dir[1] as f64;
        let first = self.pts[0];
        if s * (along - first[1] as f64) <= 1e-9 {
            return Some(first[0] as f64);
        }
        for w in self.pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (ba, bb) = (s * a[1] as f64, s * b[1] as f64);
            let t = s * along;
            if t >= ba - 1e-9 && t <= bb + 1e-9 && bb > ba {
                let f = (t - ba) / (bb - ba);
                return Some(a[0] as f64 + (b[0] - a[0]) as f64 * f);
            }
        }
        None
    }

    /// Run-length summary of the steps, e.g. "4×(0,1) 2×(1,4) 3×(1,1)".
    pub fn describe(&self) -> String {
        let mut out: Vec<(i32, i32, usize)> = vec![];
        for w in self.pts.windows(2) {
            let d = ((w[1][0] - w[0][0]).abs(), (w[1][1] - w[0][1]).abs());
            match out.last_mut() {
                Some(l) if (l.0, l.1) == d => l.2 += 1,
                _ => out.push((d.0, d.1, 1)),
            }
        }
        out.iter().map(|(a, b, n)| format!("{n}×({a},{b})")).collect::<Vec<_>>().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_circle_steepens_steadily() {
        // plan of a bow: x from 10 at z = 0 down to 1 at z ≈ 10 along a quarter circle
        let target: Vec<[f64; 2]> = (0..=90).map(|d| {
            let t = (d as f64).to_radians();
            [1.0 + 9.0 * t.cos(), 10.0 * t.sin()]
        }).collect();
        let c = fit_curve([10, 0], [-1, 1], &target, &|p| p[0] == 1, ([0, 0], [12, 14]), &CurveOptions::default());
        let d = c.describe();
        eprintln!("{d}");
        // steepness never decreases along the path
        let idx: Vec<usize> = c.pts.windows(2).map(|w| STEPS.iter().position(|s| *s == ((w[1][0] - w[0][0]).abs(), (w[1][1] - w[0][1]).abs())).unwrap()).collect();
        assert!(idx.windows(2).all(|w| w[1] >= w[0]), "{d}");
        assert_eq!(c.pts.last().unwrap()[0], 1);
    }
}
