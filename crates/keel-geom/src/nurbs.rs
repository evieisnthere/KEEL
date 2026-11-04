//! Non-uniform rational B-splines (curves and tensor-product surfaces).
//!
//! Algorithms follow Piegl & Tiller, *The NURBS Book*: span search (A2.1), basis functions (A2.2), global
//! interpolation with chord-length parameters and averaged knots (A9.1), and surface skinning (lofting) through
//! compatible section curves (§10.3).
use serde::{Deserialize, Serialize};

pub type P3 = [f64; 3];

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: P3, k: f64) -> P3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn dist(a: P3, b: P3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// A2.1: knot span index for parameter u.
pub fn find_span(n: usize, p: usize, u: f64, knots: &[f64]) -> usize {
    if u >= knots[n + 1] {
        return n;
    }
    if u <= knots[p] {
        return p;
    }
    let (mut lo, mut hi) = (p, n + 1);
    let mut mid = (lo + hi) / 2;
    while u < knots[mid] || u >= knots[mid + 1] {
        if u < knots[mid] {
            hi = mid;
        } else {
            lo = mid;
        }
        mid = (lo + hi) / 2;
    }
    mid
}

/// A2.2: the p+1 non-zero basis functions at u.
pub fn basis_funs(span: usize, u: f64, p: usize, knots: &[f64]) -> Vec<f64> {
    let mut n = vec![0.0; p + 1];
    let mut left = vec![0.0; p + 1];
    let mut right = vec![0.0; p + 1];
    n[0] = 1.0;
    for j in 1..=p {
        left[j] = u - knots[span + 1 - j];
        right[j] = knots[span + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            let den = right[r + 1] + left[j - r];
            let temp = if den.abs() < 1e-300 { 0.0 } else { n[r] / den };
            n[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        n[j] = saved;
    }
    n
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Curve {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub ctrl: Vec<P3>,
    pub weights: Vec<f64>,
}

impl Curve {
    pub fn eval(&self, u: f64) -> P3 {
        let p = self.degree;
        let n = self.ctrl.len() - 1;
        let u = u.clamp(self.knots[p], self.knots[n + 1]);
        let span = find_span(n, p, u, &self.knots);
        let b = basis_funs(span, u, p, &self.knots);
        let (mut num, mut den) = ([0.0; 3], 0.0);
        for (i, bi) in b.iter().enumerate() {
            let k = span - p + i;
            let w = self.weights[k] * bi;
            num = add(num, scale(self.ctrl[k], w));
            den += w;
        }
        scale(num, 1.0 / den)
    }
    pub fn sample(&self, n: usize) -> Vec<P3> {
        (0..=n).map(|i| self.eval(i as f64 / n as f64)).collect()
    }
}

/// Dense linear solve (Gaussian elimination with partial pivoting) for several right-hand sides.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<P3>) -> Option<Vec<P3>> {
    let n = a.len();
    for c in 0..n {
        let piv = (c..n).max_by(|&i, &j| a[i][c].abs().partial_cmp(&a[j][c].abs()).unwrap())?;
        if a[piv][c].abs() < 1e-14 {
            return None;
        }
        a.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            if f == 0.0 {
                continue;
            }
            for k in c..n {
                a[r][k] -= f * a[c][k];
            }
            b[r] = add(b[r], scale(b[c], -f));
        }
    }
    let mut x = vec![[0.0; 3]; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for k in r + 1..n {
            s = add(s, scale(x[k], -a[r][k]));
        }
        x[r] = scale(s, 1.0 / a[r][r]);
    }
    Some(x)
}

/// Chord-length parameters for a point row.
pub fn chord_params(pts: &[P3]) -> Vec<f64> {
    let mut d: Vec<f64> = vec![0.0];
    for i in 1..pts.len() {
        d.push(d[i - 1] + dist(pts[i], pts[i - 1]).max(1e-12));
    }
    let total = *d.last().unwrap();
    d.iter().map(|x| x / total).collect()
}

/// Averaged knot vector (eq. 9.8) for parameters `u` and degree p.
pub fn averaged_knots(u: &[f64], p: usize) -> Vec<f64> {
    let n = u.len() - 1;
    let mut k = vec![0.0; p + 1];
    for j in 1..=n.saturating_sub(p) {
        k.push(u[j..j + p].iter().sum::<f64>() / p as f64);
    }
    k.extend(std::iter::repeat_n(1.0, p + 1));
    k
}

/// A9.1: global interpolation through `pts` at parameters `u` with knots `knots`.
pub fn interpolate_with(pts: &[P3], p: usize, u: &[f64], knots: &[f64]) -> Option<Curve> {
    let n = pts.len() - 1;
    let mut a = vec![vec![0.0; n + 1]; n + 1];
    for (i, &ui) in u.iter().enumerate() {
        let span = find_span(n, p, ui, knots);
        let b = basis_funs(span, ui, p, knots);
        for (j, bj) in b.iter().enumerate() {
            a[i][span - p + j] = *bj;
        }
    }
    let ctrl = solve(a, pts.to_vec())?;
    Some(Curve { degree: p, knots: knots.to_vec(), weights: vec![1.0; ctrl.len()], ctrl })
}

/// Interpolating B-spline curve through points (degree reduced if there are too few points).
pub fn interpolate(pts: &[P3], degree: usize) -> Option<Curve> {
    if pts.len() < 2 {
        return None;
    }
    let p = degree.min(pts.len() - 1);
    let u = chord_params(pts);
    interpolate_with(pts, p, &u, &averaged_knots(&u, p))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Surface {
    pub du: usize,
    pub dv: usize,
    pub ku: Vec<f64>,
    pub kv: Vec<f64>,
    /// ctrl[i][j]: i along u (within a section), j along v (across sections)
    pub ctrl: Vec<Vec<P3>>,
    pub weights: Vec<Vec<f64>>,
}

impl Surface {
    pub fn eval(&self, u: f64, v: f64) -> P3 {
        let (n, m) = (self.ctrl.len() - 1, self.ctrl[0].len() - 1);
        let u = u.clamp(self.ku[self.du], self.ku[n + 1]);
        let v = v.clamp(self.kv[self.dv], self.kv[m + 1]);
        let su = find_span(n, self.du, u, &self.ku);
        let sv = find_span(m, self.dv, v, &self.kv);
        let bu = basis_funs(su, u, self.du, &self.ku);
        let bv = basis_funs(sv, v, self.dv, &self.kv);
        let (mut num, mut den) = ([0.0; 3], 0.0);
        for (a, nu) in bu.iter().enumerate() {
            for (b, nv) in bv.iter().enumerate() {
                let (i, j) = (su - self.du + a, sv - self.dv + b);
                let w = self.weights[i][j] * nu * nv;
                num = add(num, scale(self.ctrl[i][j], w));
                den += w;
            }
        }
        scale(num, 1.0 / den)
    }

    /// Iso-curve at fixed v (a section): evaluated densely.
    pub fn section(&self, v: f64, n: usize) -> Vec<P3> {
        (0..=n).map(|i| self.eval(i as f64 / n as f64, v)).collect()
    }

    /// Triangle mesh (grid of nu x nv quads), positions only.
    pub fn tessellate(&self, nu: usize, nv: usize) -> (Vec<P3>, Vec<[u32; 3]>) {
        let mut pts = Vec::with_capacity((nu + 1) * (nv + 1));
        for j in 0..=nv {
            for i in 0..=nu {
                pts.push(self.eval(i as f64 / nu as f64, j as f64 / nv as f64));
            }
        }
        let mut tris = Vec::new();
        let w = (nu + 1) as u32;
        for j in 0..nv as u32 {
            for i in 0..nu as u32 {
                let a = j * w + i;
                tris.push([a, a + 1, a + w + 1]);
                tris.push([a, a + w + 1, a + w]);
            }
        }
        (pts, tris)
    }
}

/// Skin (loft) a surface through rows of section points. Every row must have the same number of points; rows are
/// ordered along v. Section parameters and knots are shared (averaged chord lengths), making sections compatible.
pub fn loft(rows: &[Vec<P3>], du: usize, dv: usize) -> Option<Surface> {
    let nr = rows.len();
    let np = rows.first()?.len();
    if nr < 2 || np < 2 || rows.iter().any(|r| r.len() != np) {
        return None;
    }
    let du = du.min(np - 1);
    let dv = dv.min(nr - 1);
    // shared u parameters: average of per-row chord parameters
    let mut u = vec![0.0; np];
    for r in rows {
        for (k, x) in chord_params(r).into_iter().enumerate() {
            u[k] += x / nr as f64;
        }
    }
    let ku = averaged_knots(&u, du);
    let sections: Vec<Curve> = rows.iter().map(|r| interpolate_with(r, du, &u, &ku)).collect::<Option<_>>()?;
    // v parameters: average chord length between corresponding points across rows
    let mut v = vec![0.0; nr];
    for k in 0..np {
        let col: Vec<P3> = rows.iter().map(|r| r[k]).collect();
        for (j, x) in chord_params(&col).into_iter().enumerate() {
            v[j] += x / np as f64;
        }
    }
    let kv = averaged_knots(&v, dv);
    let mut ctrl = vec![vec![[0.0; 3]; nr]; np];
    for i in 0..np {
        let col: Vec<P3> = sections.iter().map(|c| c.ctrl[i]).collect();
        let c = interpolate_with(&col, dv, &v, &kv)?;
        for j in 0..nr {
            ctrl[i][j] = c.ctrl[j];
        }
    }
    Some(Surface { du, dv, ku, kv, weights: vec![vec![1.0; nr]; np], ctrl })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_passes_through_points() {
        let pts: Vec<P3> = (0..7).map(|i| { let t = i as f64 * 0.5; [t, t.sin(), 0.0] }).collect();
        let c = interpolate(&pts, 3).unwrap();
        let u = chord_params(&pts);
        for (p, ui) in pts.iter().zip(u) {
            assert!(dist(c.eval(ui), *p) < 1e-9);
        }
    }

    #[test]
    fn rational_quarter_circle_is_exact() {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let c = Curve { degree: 2, knots: vec![0., 0., 0., 1., 1., 1.], ctrl: vec![[1., 0., 0.], [1., 1., 0.], [0., 1., 0.]], weights: vec![1., w, 1.] };
        for i in 0..=10 {
            let p = c.eval(i as f64 / 10.0);
            assert!(((p[0] * p[0] + p[1] * p[1]).sqrt() - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn loft_reproduces_sections() {
        let rows: Vec<Vec<P3>> = (0..5).map(|j| { let z = j as f64; (0..6).map(|i| { let a = i as f64 / 5.0 * 1.5; [a.cos() * (1.0 + 0.1 * z), a.sin(), z] }).collect() }).collect();
        let s = loft(&rows, 3, 3).unwrap();
        // corners are interpolated exactly
        assert!(dist(s.eval(0.0, 0.0), rows[0][0]) < 1e-9);
        assert!(dist(s.eval(1.0, 1.0), rows[4][5]) < 1e-9);
    }
}
