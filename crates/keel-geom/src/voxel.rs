//! Conversion of solids to Stormworks blocks.
//!
//! Each 0.25 m voxel is sampled at 4×4×4 points. Fully inside → cube, empty → nothing, otherwise the best-fitting
//! generated block (wedge, pyramid, inverse pyramid; 24 rotations each) by Hamming distance of the occupancy
//! pattern. Only standard, editor-placeable blocks are produced (realistic default; no XML scale edits).
use keel_model::geometry::{block_points, convex_hull_faces};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub const VOXEL: f64 = 0.25;
const N: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Placement {
    pub def: String,
    pub vp: [i32; 3],
    pub r: [i32; 9],
}

/// The 24 proper rotations as `r` arrays (keel convention: world = M·v with M[i][j] = r[3j+i]).
pub fn rotations() -> &'static [[i32; 9]] {
    static R: OnceLock<Vec<[i32; 9]>> = OnceLock::new();
    R.get_or_init(|| {
        let mut out = Vec::new();
        let axes: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
        for a in axes {
            for b in axes {
                if a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<i32>() != 0 {
                    continue;
                }
                let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                // columns of M are the images of local x, y, z
                out.push([a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]]);
            }
        }
        out
    })
}

pub fn sample_offsets_pub() -> Vec<[f64; 3]> {
    sample_offsets()
}

fn sample_offsets() -> Vec<[f64; 3]> {
    let mut v = Vec::with_capacity(N * N * N);
    for i in 0..N {
        for j in 0..N {
            for k in 0..N {
                let f = |q: usize| ((q as f64 + 0.5) / N as f64 - 0.5) * VOXEL;
                v.push([f(i), f(j), f(k)]);
            }
        }
    }
    v
}

type Bits = u64;

/// Slope normal of each single-voxel shape in local coordinates (outward, unit).
fn local_slope(def: &str) -> [f64; 3] {
    let k2 = std::f64::consts::FRAC_1_SQRT_2;
    let k3 = 1.0 / 3f64.sqrt();
    match def {
        "02_wedge" => [0.0, k2, -k2],
        // pyramid: right-angle corner at (+x, -y, +z), slope faces away from it
        "03_pyramid" => [-k3, k3, -k3],
        _ => [-k3, k3, -k3], // inverse pyramid: missing corner at (-x, +y, -z)
    }
}

/// Rotation of a single-voxel slope block (`02_wedge`, `03_pyramid`, `04_invpyramid`) whose slope normal best matches `n`.
pub fn oriented_block(def: &str, n: [f64; 3]) -> [i32; 9] {
    let ln = local_slope(def);
    *rotations()
        .iter()
        .max_by(|a, b| {
            let da = rotate_f(a, ln);
            let db = rotate_f(b, ln);
            (da[0] * n[0] + da[1] * n[1] + da[2] * n[2]).partial_cmp(&(db[0] * n[0] + db[1] * n[1] + db[2] * n[2])).unwrap()
        })
        .unwrap()
}

/// Fraction of a voxel (centre `c`, metres) inside the solid, from 4x4x4 samples.
pub fn fill_fraction(solid: &dyn Solid, c: [f64; 3]) -> f64 {
    let offs = sample_offsets();
    offs.iter().filter(|o| solid.inside([c[0] + o[0], c[1] + o[1], c[2] + o[2]])).count() as f64 / offs.len() as f64
}

fn rotate_f(r: &[i32; 9], v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| r[i] as f64 * v[0] + r[3 + i] as f64 * v[1] + r[6 + i] as f64 * v[2])
}

/// Occupancy patterns of every single-voxel block shape in every rotation.
fn library() -> &'static Vec<(Bits, &'static str, [i32; 9])> {
    static L: OnceLock<Vec<(Bits, &'static str, [i32; 9])>> = OnceLock::new();
    L.get_or_init(|| {
        let offs = sample_offsets();
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for (bt, def) in [(1, "02_wedge"), (2, "03_pyramid"), (3, "04_invpyramid")] {
            let planes: Vec<([f64; 3], f64)> = convex_hull_faces(&block_points(bt).unwrap())
                .into_iter()
                .map(|(n, poly)| (n, n[0] * poly[0][0] + n[1] * poly[0][1] + n[2] * poly[0][2]))
                .collect();
            for r in rotations() {
                let mut bits: Bits = 0;
                for (k, p) in offs.iter().enumerate() {
                    // local = Mᵀ·p
                    let l = [0, 1, 2].map(|j| (0..3).map(|i| r[3 * j + i] as f64 * p[i]).sum::<f64>());
                    if planes.iter().all(|(n, d)| n[0] * l[0] + n[1] * l[1] + n[2] * l[2] <= d + 1e-9) {
                        bits |= 1 << k;
                    }
                }
                if seen.insert(bits) {
                    out.push((bits, def, *r));
                }
            }
        }
        out
    })
}

/// A solid described by an inside test (metres, Stormworks frame) and its bounding box.
pub trait Solid {
    fn bounds(&self) -> ([f64; 3], [f64; 3]);
    fn inside(&self, p: [f64; 3]) -> bool;
    /// Exact outward surface normal near p, if the solid knows it (falls back to the voxel fill gradient).
    fn normal(&self, _p: [f64; 3]) -> Option<[f64; 3]> {
        None
    }
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// hollow hull shell thickness in voxels (None = solid)
    pub shell: Option<u32>,
    /// skip the top layer (open-topped hull for a separate deck)
    pub open_top: bool,
}

/// Voxelise a solid into block placements (vehicle voxel coordinates: voxel v spans (v ± 0.5)·0.25 m).
pub fn voxelise(solid: &dyn Solid, opt: &Options) -> Vec<Placement> {
    let (lo, hi) = solid.bounds();
    let vlo = lo.map(|x| (x / VOXEL).floor() as i32 - 1);
    let vhi = hi.map(|x| (x / VOXEL).ceil() as i32 + 1);
    let offs = sample_offsets();
    let full: Bits = if N * N * N == 64 { u64::MAX } else { (1 << (N * N * N)) - 1 };
    let mut cells: HashMap<[i32; 3], Bits> = HashMap::new();
    for x in vlo[0]..=vhi[0] {
        for y in vlo[1]..=vhi[1] {
            for z in vlo[2]..=vhi[2] {
                let c = [x as f64 * VOXEL, y as f64 * VOXEL, z as f64 * VOXEL];
                let mut bits: Bits = 0;
                for (k, o) in offs.iter().enumerate() {
                    if solid.inside([c[0] + o[0], c[1] + o[1], c[2] + o[2]]) {
                        bits |= 1 << k;
                    }
                }
                if bits != 0 {
                    cells.insert([x, y, z], bits);
                }
            }
        }
    }
    // hollowing: keep voxels within `shell` steps of a non-full voxel
    let keep: Option<HashSet<[i32; 3]>> = opt.shell.map(|t| {
        let solid_cells: HashSet<[i32; 3]> = cells.iter().filter(|(_, b)| (**b).count_ones() as usize * 2 > N * N * N).map(|(k, _)| *k).collect();
        let mut dist: HashMap<[i32; 3], u32> = HashMap::new();
        let mut frontier: Vec<[i32; 3]> = solid_cells
            .iter()
            .filter(|c| {
                let full_neighbours = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]]
                    .iter()
                    .filter(|d| !(opt.open_top && d[1] == 1))
                    .all(|d| solid_cells.contains(&[c[0] + d[0], c[1] + d[1], c[2] + d[2]]));
                !full_neighbours
            })
            .copied()
            .collect();
        for c in &frontier {
            dist.insert(*c, 1);
        }
        let mut level = 1;
        while level < t && !frontier.is_empty() {
            let mut next = Vec::new();
            for c in &frontier {
                for d in [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]] {
                    let n = [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
                    if solid_cells.contains(&n) && !dist.contains_key(&n) {
                        dist.insert(n, level + 1);
                        next.push(n);
                    }
                }
            }
            frontier = next;
            level += 1;
        }
        dist.into_keys().collect()
    });
    let top_y: HashMap<(i32, i32), i32> = if opt.open_top {
        let mut m = HashMap::new();
        for k in cells.keys() {
            let e = m.entry((k[0], k[2])).or_insert(k[1]);
            *e = (*e).max(k[1]);
        }
        m
    } else {
        HashMap::new()
    };
    let lib = library();
    let mut out = Vec::new();
    let mut keys: Vec<_> = cells.keys().copied().collect();
    keys.sort();
    // pass 1: long wedges on shallow slopes
    let mut consumed: HashSet<[i32; 3]> = HashSet::new();
    for k in &keys {
        let bits = cells[k];
        if bits == full || consumed.contains(k) {
            continue;
        }
        if let Some(keep) = &keep {
            if bits.count_ones() as usize * 2 > N * N * N && !keep.contains(k) {
                continue;
            }
        }
        if let Some((p, cov)) = long_wedge(*k, &cells, &consumed, full, solid) {
            consumed.extend(cov);
            out.push(p);
        }
    }
    for k in keys {
        if consumed.contains(&k) {
            continue;
        }
        let bits = cells[&k];
        if opt.open_top && top_y.get(&(k[0], k[2])) == Some(&k[1]) && bits != full {
            continue;
        }
        let ones = bits.count_ones() as usize;
        if let Some(keep) = &keep {
            if ones * 2 > N * N * N && !keep.contains(&k) {
                continue;
            }
        }
        if let Some((def, r)) = choose(k, bits, &cells, lib, full, solid) {
            out.push(Placement { def: def.to_string(), vp: k, r });
        }
    }
    out
}

fn fill(cells: &HashMap<[i32; 3], Bits>, k: [i32; 3]) -> f64 {
    cells.get(&k).map(|b| b.count_ones() as f64 / (N * N * N) as f64).unwrap_or(0.0)
}

fn surface_normal(solid: &dyn Solid, cells: &HashMap<[i32; 3], Bits>, k: [i32; 3]) -> Option<[f64; 3]> {
    solid.normal([k[0] as f64 * VOXEL, k[1] as f64 * VOXEL, k[2] as f64 * VOXEL]).or_else(|| normal_at(cells, k))
}

fn normal_at(cells: &HashMap<[i32; 3], Bits>, k: [i32; 3]) -> Option<[f64; 3]> {
    let mut g = [0.0f64; 3];
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                let w = fill(cells, [k[0] + dx, k[1] + dy, k[2] + dz]);
                g[0] += w * dx as f64;
                g[1] += w * dy as f64;
                g[2] += w * dz as f64;
            }
        }
    }
    let len = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
    (len > 1e-9).then(|| [-g[0] / len, -g[1] / len, -g[2] / len])
}

/// Try to cover a shallow edge slope with a 1x2 or 1x4 wedge anchored so that the covered voxels' fills match.
/// Returns the placement and the voxels it covers.
fn long_wedge(k: [i32; 3], cells: &HashMap<[i32; 3], Bits>, consumed: &HashSet<[i32; 3]>, full: Bits, solid: &dyn Solid) -> Option<(Placement, Vec<[i32; 3]>)> {
    let n = surface_normal(solid, cells, k)?;
    let mut a: Vec<f64> = n.iter().map(|x| x.abs()).collect();
    a.sort_by(|x, y| y.partial_cmp(x).unwrap());
    if a[2] > 0.2 || a[1] < 0.12 {
        return None; // not a clean edge slope, or practically flat
    }
    let ratio = a[1] / a[0];
    let (len, def) = if ratio < 0.36 { (4, "08_wedge_4") } else if ratio < 0.7 { (2, "05_wedge_2") } else { return None };
    let ln = {
        let v = [0.0, len as f64, -1.0];
        let l = (v[1] * v[1] + 1.0f64).sqrt();
        [0.0, v[1] / l, v[2] / l]
    };
    let r = *rotations().iter().max_by(|x, y| {
        let dx = rotate_f(x, ln);
        let dy = rotate_f(y, ln);
        (dx[0] * n[0] + dx[1] * n[1] + dx[2] * n[2]).partial_cmp(&(dy[0] * n[0] + dy[1] * n[1] + dy[2] * n[2])).unwrap()
    })?;
    let d = [-r[6], -r[7], -r[8]]; // world direction of local -z (the run)
    let expected: Vec<f64> = (0..len).map(|i| (2 * len - 1 - 2 * i) as f64 / (2 * len) as f64).collect();
    let mut best: Option<(f64, [i32; 3])> = None;
    for j in 0..len {
        let anchor = [k[0] - j * d[0], k[1] - j * d[1], k[2] - j * d[2]];
        let mut err = 0.0;
        let mut ok = true;
        for (i, e) in expected.iter().enumerate() {
            let v = [anchor[0] + i as i32 * d[0], anchor[1] + i as i32 * d[1], anchor[2] + i as i32 * d[2]];
            match cells.get(&v) {
                Some(b) if *b != full && !consumed.contains(&v) => err += (fill(cells, v) - e).abs(),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && best.is_none_or(|b| err < b.0) {
            best = Some((err, anchor));
        }
    }
    let (err, anchor) = best?;
    if err / len as f64 > 0.14 {
        return None;
    }
    let covered = (0..len).map(|i| [anchor[0] + i * d[0], anchor[1] + i * d[1], anchor[2] + i * d[2]]).collect();
    Some((Placement { def: def.to_string(), vp: anchor, r }, covered))
}

/// Surface-aligned block choice. The outward normal comes from the fill gradient over the 3x3x3 neighbourhood.
/// Axis-aligned surfaces become cubes, edge slopes become wedges oriented to the normal, and corners become pyramids or
/// inverse pyramids. Neighbours on one smooth surface therefore pick the same shape and orientation, which avoids the
/// spiky "best occupancy match" look.
fn choose(k: [i32; 3], bits: Bits, cells: &HashMap<[i32; 3], Bits>, lib: &[(Bits, &'static str, [i32; 9])], full: Bits, solid: &dyn Solid) -> Option<(&'static str, [i32; 9])> {
    let f = bits.count_ones() as f64 / (N * N * N) as f64;
    if bits == full {
        return Some(("01_block", keel_io::vehicle::IDENTITY));
    }
    let Some(n) = surface_normal(solid, cells, k) else {
        return if f >= 0.5 { Some(("01_block", keel_io::vehicle::IDENTITY)) } else { None };
    };
    let mut a: Vec<f64> = n.iter().map(|x| x.abs()).collect();
    a.sort_by(|x, y| y.partial_cmp(x).unwrap());
    let class = if a[1] < 0.38 { 0 } else if a[2] < 0.38 { 1 } else { 2 };
    let best_oriented = |def: &str| {
        lib.iter()
            .filter(|(_, d, _)| *d == def)
            .map(|(_, d, r)| {
                let sn = rotate_f(r, local_slope(d));
                (sn[0] * n[0] + sn[1] * n[1] + sn[2] * n[2], *d, *r)
            })
            .max_by(|x, y| x.0.partial_cmp(&y.0).unwrap())
            .map(|(_, d, r)| (d, r))
    };
    match class {
        0 => (f >= 0.5).then_some(("01_block", keel_io::vehicle::IDENTITY)),
        1 => {
            if f < 0.2 {
                None
            } else if f > 0.8 {
                Some(("01_block", keel_io::vehicle::IDENTITY))
            } else {
                best_oriented("02_wedge")
            }
        }
        _ => {
            if f < 0.07 {
                None
            } else if f < 0.42 {
                best_oriented("03_pyramid")
            } else if f < 0.93 {
                best_oriented("04_invpyramid")
            } else {
                Some(("01_block", keel_io::vehicle::IDENTITY))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ball(f64);
    impl Solid for Ball {
        fn bounds(&self) -> ([f64; 3], [f64; 3]) {
            ([-self.0; 3], [self.0; 3])
        }
        fn inside(&self, p: [f64; 3]) -> bool {
            p[0] * p[0] + p[1] * p[1] + p[2] * p[2] <= self.0 * self.0
        }
    }

    #[test]
    fn rotations_are_24_proper() {
        let r = rotations();
        assert_eq!(r.len(), 24);
        for m in r {
            let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[3] * (m[1] * m[8] - m[2] * m[7]) + m[6] * (m[1] * m[5] - m[2] * m[4]);
            assert_eq!(det, 1);
        }
    }

    #[test]
    fn ball_uses_slopes_and_volume_matches() {
        let b = Ball(1.5);
        let p = voxelise(&b, &Options::default());
        let vol: f64 = p.iter().map(|x| match x.def.as_str() { "01_block" => 1.0, "02_wedge" => 0.5, "03_pyramid" => 1.0 / 6.0, _ => 5.0 / 6.0 }).sum::<f64>() * VOXEL.powi(3);
        let exact = 4.0 / 3.0 * std::f64::consts::PI * 1.5f64.powi(3);
        assert!((vol / exact - 1.0).abs() < 0.04, "{vol} vs {exact}");
        assert!(p.iter().any(|x| x.def != "01_block"));
        let hollow = voxelise(&b, &Options { shell: Some(1), open_top: false });
        assert!(hollow.len() < p.len() / 2);
    }
}
