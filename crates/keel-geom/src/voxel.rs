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
    for k in keys {
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
        // best fit among empty, cube and the shape library
        let mut best: (u32, Option<(&str, [i32; 9])>) = (ones as u32, None);
        let cube = (full ^ bits).count_ones();
        if cube < best.0 {
            best = (cube, Some(("01_block", keel_io::vehicle::IDENTITY)));
        }
        for (b, def, r) in lib {
            let d = (b ^ bits).count_ones();
            if d < best.0 {
                best = (d, Some((def, *r)));
            }
        }
        if let Some((def, r)) = best.1 {
            out.push(Placement { def: def.to_string(), vp: k, r });
        }
    }
    out
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
