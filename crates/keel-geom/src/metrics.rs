//! Quality metrics for generated block hulls (docs/engineering/hull-to-blocks.md §1).
//!
//! The placed blocks are rasterised (4 samples per voxel per axis) from their exact game solids. The outside is
//! flood-filled. Then:
//! * Q1 holes: interior samples (well inside the NURBS hull, below the deck) that the outside reaches;
//! * Q2 step faces: exposed block faces whose normal is more than 25° off the NURBS normal (decks and the bow/stern
//!   zones are reported separately).
use crate::hull::Hull;
use crate::voxel::{Placement, Solid, VOXEL};
use keel_model::geometry::{block_points, convex_hull_faces};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Metrics {
    pub blocks: usize,
    /// interior samples reached from outside (Q1; must be 0)
    pub leak_samples: usize,
    /// voxel z indices where leaks occur
    pub leak_z: Vec<i32>,
    /// exposed hull area (m²) outside the end zones, excluding decks
    pub mid_area: f64,
    /// exposed step-face area (m²) outside the end zones (Q2)
    pub mid_step_area: f64,
    /// same for the bow and stern zones
    pub end_area: f64,
    pub end_step_area: f64,
    /// exposed faces that are steps: (centroid metres, normal)
    pub steps: Vec<([f64; 3], [f64; 3])>,
    /// every exposed face: (polygon in metres, outward normal, is a step, block index)
    pub faces: Vec<(Vec<[f64; 3]>, [f64; 3], bool, usize)>,
}

impl Metrics {
    pub fn mid_step_share(&self) -> f64 {
        if self.mid_area > 0.0 { self.mid_step_area / self.mid_area } else { 0.0 }
    }
    pub fn summary(&self) -> String {
        format!(
            "blocks {}, leaks {} (z {:?}), steps mid {:.1}% of {:.1} m², ends {:.1}% of {:.1} m²",
            self.blocks,
            self.leak_samples,
            self.leak_z.iter().take(8).collect::<Vec<_>>(),
            100.0 * self.mid_step_share(),
            self.mid_area,
            if self.end_area > 0.0 { 100.0 * self.end_step_area / self.end_area } else { 0.0 },
            self.end_area
        )
    }
}

fn def_bt(def: &str) -> Option<i32> {
    let n: i32 = def.get(0..2)?.parse().ok()?;
    if (1..=16).contains(&n) { Some(n - 1) } else { None }
}

/// Block solid in eighth-metre units (e = 8·metres before the y shift): convex polytope as (normal, d) planes plus faces.
struct Poly {
    planes: Vec<([f64; 3], f64)>,
    faces: Vec<([f64; 3], Vec<[f64; 3]>)>,
    lo: [f64; 3],
    hi: [f64; 3],
}

fn block_poly(pl: &Placement) -> Option<Poly> {
    let bt = def_bt(&pl.def)?;
    let r = pl.r;
    let pts: Vec<[f64; 3]> = block_points(bt)?
        .into_iter()
        .map(|p| {
            let p = p.map(|x| x * 8.0);
            let q = [0, 1, 2].map(|i| r[i] as f64 * p[0] + r[3 + i] as f64 * p[1] + r[6 + i] as f64 * p[2]);
            [0, 1, 2].map(|i| q[i] + 2.0 * pl.vp[i] as f64)
        })
        .collect();
    let faces = convex_hull_faces(&pts);
    let planes = faces.iter().map(|(n, poly)| (*n, n[0] * poly[0][0] + n[1] * poly[0][1] + n[2] * poly[0][2])).collect();
    let lo = [0, 1, 2].map(|i| pts.iter().map(|p| p[i]).fold(f64::MAX, f64::min));
    let hi = [0, 1, 2].map(|i| pts.iter().map(|p| p[i]).fold(f64::MIN, f64::max));
    Some(Poly { planes, faces, lo, hi })
}

/// e-units → hull metres (lattice: x = e/8, y = (e + 1)/8, z = e/8).
fn to_m(e: [f64; 3]) -> [f64; 3] {
    [e[0] * 0.125, (e[1] + 1.0) * 0.125, e[2] * 0.125]
}

/// Compare the rasterised blocks with a reference solid (hull metres): per voxel, the number of the 64 samples where
/// they disagree. Returns (voxel, extra samples (block but not reference), missing samples), worst first.
pub fn compare(placements: &[Placement], reference: &dyn Fn([f64; 3]) -> bool) -> Vec<([i32; 3], usize, usize)> {
    let polys: Vec<(Poly, usize)> = placements.iter().enumerate().filter_map(|(i, p)| block_poly(p).map(|q| (q, i))).collect();
    let mut solid: HashMap<[i32; 3], u64> = HashMap::new();
    let mut voxels: std::collections::HashSet<[i32; 3]> = Default::default();
    let samp = |v: [i32; 3], k: usize| -> [f64; 3] {
        let o = [k & 3, (k >> 2) & 3, (k >> 4) & 3];
        [0, 1, 2].map(|i| 2.0 * v[i] as f64 - 1.0 + 0.25 + 0.5 * o[i] as f64 + [0.0131, 0.0173, 0.0197][i])
    };
    for (p, _) in &polys {
        let lo = [0, 1, 2].map(|i| ((p.lo[i] + 1.0) / 2.0).floor() as i32);
        let hi = [0, 1, 2].map(|i| ((p.hi[i] + 1.0) / 2.0).ceil() as i32 - 1);
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    let v = [x, y, z];
                    voxels.insert(v);
                    for k in 0..64 {
                        let q = samp(v, k);
                        if p.planes.iter().all(|(nn, d)| nn[0] * q[0] + nn[1] * q[1] + nn[2] * q[2] <= d + 1e-6) {
                            *solid.entry(v).or_default() |= 1 << k;
                        }
                    }
                }
            }
        }
    }
    let mut out = vec![];
    for v in voxels {
        let s = solid.get(&v).copied().unwrap_or(0);
        let (mut extra, mut missing) = (0, 0);
        for k in 0..64 {
            let r = reference(to_m(samp(v, k)));
            let b = s & (1 << k) != 0;
            if b && !r {
                extra += 1;
            }
            if r && !b {
                missing += 1;
            }
        }
        if extra + missing > 0 {
            out.push((v, extra, missing));
        }
    }
    out.sort_by_key(|x| std::cmp::Reverse(x.1 + x.2));
    out
}

pub fn measure(placements: &[Placement], hull: &Hull) -> Metrics {
    measure_with(placements, hull, None)
}

/// As [`measure`], with an optional interior test (hull metres) used for the leak seeds instead of the NURBS: a seed
/// is any outside-reachable sample where `interior` holds half a voxel deep in every axis direction.
pub fn measure_with(placements: &[Placement], hull: &Hull, interior: Option<&dyn Fn([f64; 3]) -> bool>) -> Metrics {
    let solid = crate::HullSolid::new(hull);
    let polys: Vec<Poly> = placements.iter().filter_map(block_poly).collect();
    let mut m = Metrics { blocks: placements.len(), ..Default::default() };
    if polys.is_empty() {
        return m;
    }
    // grid: 0.5 e-unit spacing, padded by one voxel
    let lo = [0, 1, 2].map(|i| polys.iter().map(|p| p.lo[i]).fold(f64::MAX, f64::min) - 2.0);
    let hi = [0, 1, 2].map(|i| polys.iter().map(|p| p.hi[i]).fold(f64::MIN, f64::max) + 2.0);
    const S: f64 = 0.5;
    const JIT: [f64; 3] = [0.0131, 0.0173, 0.0197];
    let n = [0, 1, 2].map(|i| ((hi[i] - lo[i]) / S).ceil() as usize);
    let idx = |a: usize, b: usize, c: usize| (a * n[1] + b) * n[2] + c;
    let pos = |a: usize, b: usize, c: usize| [lo[0] + (a as f64 + 0.5) * S + JIT[0], lo[1] + (b as f64 + 0.5) * S + JIT[1], lo[2] + (c as f64 + 0.5) * S + JIT[2]];
    let mut solid_s = vec![false; n[0] * n[1] * n[2]];
    for p in &polys {
        let r = |i: usize| -> (usize, usize) {
            let a = (((p.lo[i] - lo[i]) / S) - 1.0).floor().max(0.0) as usize;
            let b = ((((p.hi[i] - lo[i]) / S) + 1.0).ceil() as usize).min(n[i]);
            (a, b)
        };
        let (rx, ry, rz) = (r(0), r(1), r(2));
        for a in rx.0..rx.1 {
            for b in ry.0..ry.1 {
                for c in rz.0..rz.1 {
                    let q = pos(a, b, c);
                    if p.planes.iter().all(|(nn, d)| nn[0] * q[0] + nn[1] * q[1] + nn[2] * q[2] <= d + 1e-6) {
                        solid_s[idx(a, b, c)] = true;
                    }
                }
            }
        }
    }
    // flood the outside
    let mut ext = vec![false; solid_s.len()];
    let mut stack = std::collections::VecDeque::from(vec![(0usize, 0usize, 0usize)]);
    ext[idx(0, 0, 0)] = true;
    let trace = std::env::var("KEEL_METRICS_TRACE").is_ok();
    let mut parent: Vec<u32> = if trace { vec![u32::MAX; solid_s.len()] } else { vec![] };
    while let Some((a, b, c)) = stack.pop_front() {
        let nb = [
            (a.wrapping_sub(1), b, c),
            (a + 1, b, c),
            (a, b.wrapping_sub(1), c),
            (a, b + 1, c),
            (a, b, c.wrapping_sub(1)),
            (a, b, c + 1),
        ];
        for (x, y, z) in nb {
            if x >= n[0] || y >= n[1] || z >= n[2] {
                continue;
            }
            let k = idx(x, y, z);
            if !ext[k] && !solid_s[k] {
                ext[k] = true;
                if trace {
                    parent[k] = idx(a, b, c) as u32;
                }
                stack.push_back((x, y, z));
            }
        }
    }
    let ext_at = |e: [f64; 3]| -> bool {
        let a = ((e[0] - lo[0]) / S).floor();
        let b = ((e[1] - lo[1]) / S).floor();
        let c = ((e[2] - lo[2]) / S).floor();
        if a < 0.0 || b < 0.0 || c < 0.0 || a as usize >= n[0] || b as usize >= n[1] || c as usize >= n[2] {
            return true;
        }
        ext[idx(a as usize, b as usize, c as usize)]
    };
    // debug: ASCII section at voxel z = KEEL_METRICS_SLICE ('#' solid, '.' outside, ' ' enclosed)
    if let Some(zv) = std::env::var("KEEL_METRICS_SLICE").ok().and_then(|v| v.parse::<f64>().ok()) {
        let c = ((2.0 * zv - lo[2]) / S).floor() as usize;
        for b in (0..n[1]).rev() {
            let row: String = (0..n[0]).map(|a| { let k = idx(a, b, c); if solid_s[k] { '#' } else if ext[k] { '.' } else { ' ' } }).collect();
            eprintln!("{:6.2} {}", to_m(pos(0, b, c))[1], row);
        }
    }
    // Q1: interior seeds well inside the NURBS (one voxel margin) and below the generated deck
    let p = &hull.params;
    let deck_e = polys.iter().map(|p| p.hi[1]).fold(f64::MIN, f64::max);
    let mut leak_z: HashMap<i32, usize> = HashMap::new();
    let d = 3.0 * VOXEL;
    for a in 0..n[0] {
        for b in 0..n[1] {
            for c in 0..n[2] {
                let k = idx(a, b, c);
                if !ext[k] {
                    continue;
                }
                let e = pos(a, b, c);
                if interior.is_none() && e[1] > deck_e - 4.0 {
                    continue;
                }
                let q = to_m(e);
                let seed = match interior {
                    Some(f) => {
                        let h = VOXEL;
                        [[0.0, 0.0, 0.0], [h, 0.0, 0.0], [-h, 0.0, 0.0], [0.0, h, 0.0], [0.0, -h, 0.0], [0.0, 0.0, h], [0.0, 0.0, -h]].iter().all(|o| f([q[0] + o[0], q[1] + o[1], q[2] + o[2]]))
                    }
                    None => false,
                };
                let ins = |dx: f64, dy: f64, dz: f64| solid.inside([q[0] + dx, q[1] + dy, q[2] + dz]);
                if seed || interior.is_none() && ins(0.0, 0.0, 0.0) && ins(d, 0.0, 0.0) && ins(-d, 0.0, 0.0) && ins(0.0, -d, 0.0) && ins(0.0, d, 0.0) && ins(0.0, 0.0, d) && ins(0.0, 0.0, -d) {
                    if trace && m.leak_samples == 0 {
                        // walk back toward the outside and print the path as it crosses the skin
                        let mut k = k;
                        let mut path = vec![];
                        while parent[k] != u32::MAX {
                            path.push(k);
                            k = parent[k] as usize;
                        }
                        for &k in path.iter() {
                            let (a, r) = (k / (n[1] * n[2]), k % (n[1] * n[2]));
                            let (b, c) = (r / n[2], r % n[2]);
                            let e = pos(a, b, c);
                            eprintln!("  path voxel {:?} m {:?} inside {}", [0, 1, 2].map(|i| ((e[i] + 1.0) / 2.0).floor() as i32), to_m(e).map(|v| (v * 100.0).round() / 100.0), solid.inside(to_m(e)));
                        }
                    }
                    m.leak_samples += 1;
                    *leak_z.entry(((e[2] + 1.0) / 2.0).floor() as i32).or_default() += 1;
                }
            }
        }
    }
    let mut lz: Vec<i32> = leak_z.into_keys().collect();
    lz.sort();
    m.leak_z = lz;
    // Q2: exposed faces
    let end_zone = 0.35 * p.length;
    for (bi, poly) in polys.iter().enumerate() {
        for (nrm, f) in &poly.faces {
            let k = f.len() as f64;
            let c = [0, 1, 2].map(|i| f.iter().map(|q| q[i]).sum::<f64>() / k);
            // exposed if the outside touches the face anywhere: probe the centroid and points toward each vertex
            let mut probes = vec![c];
            for q in f {
                probes.push([0, 1, 2].map(|i| c[i] + 0.7 * (q[i] - c[i])));
            }
            if !probes.iter().any(|p| ext_at([p[0] + nrm[0] * 0.3, p[1] + nrm[1] * 0.3, p[2] + nrm[2] * 0.3])) {
                continue;
            }
            // area (e² → m²)
            let mut cr = [0.0; 3];
            for i in 0..f.len() {
                let (u, v) = (f[i], f[(i + 1) % f.len()]);
                cr[0] += u[1] * v[2] - u[2] * v[1];
                cr[1] += u[2] * v[0] - u[0] * v[2];
                cr[2] += u[0] * v[1] - u[1] * v[0];
            }
            let area = 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt() * 0.125 * 0.125;
            let cm = to_m(c);
            let fm: Vec<[f64; 3]> = f.iter().map(|q| to_m(*q)).collect();
            if nrm[1] > 0.9 {
                m.faces.push((fm, *nrm, false, bi));
                continue; // deck
            }
            // the keel strip (centre column) is flat by symmetry: its target normal is straight down
            let sn = if cm[0].abs() < VOXEL * 0.5 && nrm[1] < -0.9 { Some([0.0, -1.0, 0.0]) } else { solid.normal([cm[0], cm[1].min(p.sheer(cm[2] / p.length + 0.5) - 0.2), cm[2]]) };
            let Some(sn) = sn else {
                m.faces.push((fm, *nrm, false, bi));
                continue;
            };
            let cos = nrm[0] * sn[0] + nrm[1] * sn[1] + nrm[2] * sn[2];
            // a step is an exposed axis-aligned face (a ledge or a block corner) where the surface is sloped
            let axis = nrm.iter().any(|c| c.abs() > 0.99);
            let step = axis && cos < 25f64.to_radians().cos();
            m.faces.push((fm, *nrm, step, bi));
            if cm[2].abs() > end_zone {
                m.end_area += area;
                if step {
                    m.end_step_area += area;
                }
            } else {
                m.mid_area += area;
                if step {
                    m.mid_step_area += area;
                    m.steps.push((cm, *nrm));
                }
            }
        }
    }
    m
}
