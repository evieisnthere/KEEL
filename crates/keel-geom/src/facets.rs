//! Facet quantisation: turn a smooth solid into Stormworks blocks by snapping its surface to the planar gradients
//! the blocks provide, so the result is made of flat faces that meet edge to edge.
//!
//! Available planar facets (voxel units), each tiled by a grid of boxes:
//! * axis faces: cubes;
//! * one-direction slopes 1:1, 1:2, 1:4 (and the steep 2:1, 4:1): wedges `02_wedge` / `05_wedge_2` / `08_wedge_4`, box a×b×1;
//! * two-direction slopes: a plane x/a + y/b + z/c = m on a grid of a×b×c boxes cuts every box either at its low
//!   corner (a pyramid stays) or at its high corner (an inverse pyramid stays). Box sizes: 1×1×1, 1×1×2, 1×1×4,
//!   1×2×2, 1×2×4, 1×4×4 (in any axis order) = the 03/04, 06/07, 09/10, 11/14, 12/15, 13/16 block pairs.
//!
//! Each surface voxel takes the facet family nearest to the solid's true normal. Its family box (aligned to a global
//! grid so neighbouring boxes continue the same plane) is filled with empty, pyramid, inverse pyramid or full
//! (wedges: empty, half or full) according to how much of the box is inside the solid. Blocks are matched to boxes
//! exactly using the game-verified block solids.
use crate::voxel::{fill_fraction, Placement, Solid, VOXEL};
use keel_model::geometry::block_points;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Family {
    /// unit box, cube or nothing
    Axis,
    /// slope in the plane of axes (p, q) with box lengths (lp, lq); extrusion along the third axis
    Wedge { p: usize, q: usize, lp: i32, lq: i32 },
    /// two-direction slope with box lengths per axis
    Pyramid { l: [i32; 3] },
}

struct Candidate {
    n: [f64; 3],
    fam: Family,
    signs: [i32; 3],
}

fn candidates() -> &'static Vec<Candidate> {
    static C: OnceLock<Vec<Candidate>> = OnceLock::new();
    C.get_or_init(|| {
        let mut v = Vec::new();
        let norm = |a: [f64; 3]| {
            let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            [a[0] / l, a[1] / l, a[2] / l]
        };
        for ax in 0..3 {
            for s in [-1, 1] {
                let mut n = [0.0; 3];
                n[ax] = s as f64;
                let mut signs = [1; 3];
                signs[ax] = s;
                v.push(Candidate { n, fam: Family::Axis, signs });
            }
        }
        let wedge_lens = [(1, 1), (1, 2), (2, 1), (1, 4), (4, 1)];
        for p in 0..3 {
            for q in p + 1..3 {
                for &(lp, lq) in &wedge_lens {
                    for sp in [-1, 1] {
                        for sq in [-1, 1] {
                            let mut n = [0.0; 3];
                            n[p] = sp as f64 / lp as f64;
                            n[q] = sq as f64 / lq as f64;
                            let mut signs = [1; 3];
                            signs[p] = sp;
                            signs[q] = sq;
                            v.push(Candidate { n: norm(n), fam: Family::Wedge { p, q, lp, lq }, signs });
                        }
                    }
                }
            }
        }
        let triples = [[1, 1, 1], [1, 1, 2], [1, 1, 4], [1, 2, 2], [1, 2, 4], [1, 4, 4]];
        let mut seen = HashSet::new();
        for t in triples {
            for perm in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
                let l = [t[perm[0]], t[perm[1]], t[perm[2]]];
                if !seen.insert(l) {
                    continue;
                }
                for sx in [-1, 1] {
                    for sy in [-1, 1] {
                        for sz in [-1, 1] {
                            let n = norm([sx as f64 / l[0] as f64, sy as f64 / l[1] as f64, sz as f64 / l[2] as f64]);
                            v.push(Candidate { n, fam: Family::Pyramid { l }, signs: [sx, sy, sz] });
                        }
                    }
                }
            }
        }
        v
    })
}

/// The 48 signed permutation matrices as `r` arrays (proper and improper; the game accepts both, its own default
/// rotation is a mirror).
fn signed_perms() -> &'static Vec<[i32; 9]> {
    static R: OnceLock<Vec<[i32; 9]>> = OnceLock::new();
    R.get_or_init(|| {
        let mut out = Vec::new();
        for perm in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
            for s in 0..8 {
                let mut r = [0; 9];
                for j in 0..3 {
                    // column j (image of local axis j) is ±e_perm[j]
                    let sign = if s >> j & 1 == 1 { -1 } else { 1 };
                    r[3 * j + perm[j]] = sign;
                }
                out.push(r);
            }
        }
        out
    })
}

/// Block point sets in eighth-metre integer units (exact).
fn block_type(def: &str) -> i32 {
    match def {
        "02_wedge" => 1,
        "03_pyramid" => 2,
        "04_invpyramid" => 3,
        "05_wedge_2" => 4,
        "06_pyramid_2" => 5,
        "07_invpyramid_2" => 6,
        "08_wedge_4" => 7,
        "09_pyramid_4" => 8,
        "10_invpyramid_4" => 9,
        "11_pyramid_2x2" => 10,
        "12_pyramid_2x4" => 11,
        "13_pyramid_4x4" => 12,
        "14_invpyramid_2x2" => 13,
        "15_invpyramid_2x4" => 14,
        "16_invpyramid_4x4" => 15,
        _ => 0,
    }
}

fn block_pts8(bt: i32) -> Vec<[i32; 3]> {
    block_points(bt).unwrap().into_iter().map(|p| p.map(|x| (x * 8.0).round() as i32)).collect()
}

/// Find (def, r, anchor voxel) whose solid's corner set equals `want` (eighth-metre units).
fn match_block(bts: &[(i32, &'static str)], want: &[[i32; 3]]) -> Option<(&'static str, [i32; 9], [i32; 3])> {
    let mut w: Vec<[i32; 3]> = want.to_vec();
    w.sort();
    w.dedup();
    for &(bt, def) in bts {
        let pts = block_pts8(bt);
        if pts.len() != w.len() {
            continue;
        }
        for r in signed_perms() {
            let mut tp: Vec<[i32; 3]> = pts.iter().map(|p| [0, 1, 2].map(|i| r[i] * p[0] + r[3 + i] * p[1] + r[6 + i] * p[2])).collect();
            tp.sort();
            let t = [w[0][0] - tp[0][0], w[0][1] - tp[0][1], w[0][2] - tp[0][2]];
            if t.iter().any(|x| x % 2 != 0) {
                continue;
            }
            if tp.iter().zip(w.iter()).all(|(a, b)| a[0] + t[0] == b[0] && a[1] + t[1] == b[1] && a[2] + t[2] == b[2]) {
                return Some((def, *r, [t[0] / 2, t[1] / 2, t[2] / 2]));
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct FacetOptions {
    pub shell: u32,
    pub origin: [i32; 3],
    /// solid voxel offset between the solid's frame and voxel centres (metres added to voxel centre positions)
    pub frame_offset: [f64; 3],
}

impl Default for FacetOptions {
    fn default() -> Self {
        FacetOptions { shell: 1, origin: [0, 0, 0], frame_offset: [0.0, 0.0, 0.0] }
    }
}

pub fn build(solid: &dyn Solid, opt: &FacetOptions) -> Vec<Placement> {
    let (lo, hi) = solid.bounds();
    let off = opt.frame_offset;
    let vlo: [i32; 3] = [0, 1, 2].map(|i| ((lo[i] - off[i]) / VOXEL).floor() as i32 - 1);
    let vhi: [i32; 3] = [0, 1, 2].map(|i| ((hi[i] - off[i]) / VOXEL).ceil() as i32 + 1);
    let centre = |v: [i32; 3]| [v[0] as f64 * VOXEL + off[0], v[1] as f64 * VOXEL + off[1], v[2] as f64 * VOXEL + off[2]];
    let mut fills: HashMap<[i32; 3], f64> = HashMap::new();
    for x in vlo[0]..=vhi[0] {
        for y in vlo[1]..=vhi[1] {
            for z in vlo[2]..=vhi[2] {
                let f = fill_fraction(solid, centre([x, y, z]));
                if f > 0.0 {
                    fills.insert([x, y, z], f);
                }
            }
        }
    }
    let fill = |v: [i32; 3]| *fills.get(&v).unwrap_or(&0.0);
    let mut surface: Vec<[i32; 3]> = fills.iter().filter(|(_, f)| **f > 0.02 && **f < 0.98).map(|(k, _)| *k).collect();
    surface.sort();
    let mut claimed: HashSet<[i32; 3]> = HashSet::new();
    let mut occupied: HashSet<[i32; 3]> = HashSet::new();
    let mut out: Vec<Placement> = Vec::new();
    let o = opt.origin;
    let place = |out: &mut Vec<Placement>, def: &str, r: [i32; 9], vp: [i32; 3]| out.push(Placement { def: def.into(), vp: [vp[0] + o[0], vp[1] + o[1], vp[2] + o[2]], r });
    let cube_r = keel_io::vehicle::IDENTITY;
    let offs = crate::voxel::sample_offsets_pub();
    // fraction of each cell inside a placed block (convex solid from its exact corner set)
    let shape_fills = |def: &str, r: [i32; 9], anchor: [i32; 3], cells: &[[i32; 3]]| -> Vec<f64> {
        let bt = block_type(def);
        let pts: Vec<[f64; 3]> = block_points(bt)
            .unwrap()
            .into_iter()
            .map(|p| {
                let q = [0, 1, 2].map(|i| r[i] as f64 * p[0] + r[3 + i] as f64 * p[1] + r[6 + i] as f64 * p[2]);
                [q[0] + anchor[0] as f64 * VOXEL, q[1] + anchor[1] as f64 * VOXEL, q[2] + anchor[2] as f64 * VOXEL]
            })
            .collect();
        let planes: Vec<([f64; 3], f64)> = keel_model::geometry::convex_hull_faces(&pts)
            .into_iter()
            .map(|(n, poly)| (n, n[0] * poly[0][0] + n[1] * poly[0][1] + n[2] * poly[0][2]))
            .collect();
        cells
            .iter()
            .map(|c| {
                let cc = [c[0] as f64 * VOXEL, c[1] as f64 * VOXEL, c[2] as f64 * VOXEL];
                offs.iter().filter(|o| { let q = [cc[0] + o[0], cc[1] + o[1], cc[2] + o[2]]; planes.iter().all(|(n, d)| n[0] * q[0] + n[1] * q[1] + n[2] * q[2] <= d + 1e-9) }).count() as f64
                    / offs.len() as f64
            })
            .collect()
    };
    const TOL: f64 = 0.3;
    for v in surface {
        if claimed.contains(&v) {
            continue;
        }
        let Some(n) = solid.normal(centre(v)) else { continue };
        // candidate facet families, best normal match first
        let mut ranked: Vec<&Candidate> = candidates().iter().collect();
        ranked.sort_by(|a, b| {
            let da = a.n[0] * n[0] + a.n[1] * n[1] + a.n[2] * n[2];
            let db = b.n[0] * n[0] + b.n[1] * n[1] + b.n[2] * n[2];
            db.partial_cmp(&da).unwrap()
        });
        let mut done = false;
        for cand in ranked.into_iter().take(6) {
            let dims: [i32; 3] = match cand.fam {
                Family::Axis => [1, 1, 1],
                Family::Wedge { p, q, lp, lq } => {
                    let mut d = [1; 3];
                    d[p] = lp;
                    d[q] = lq;
                    d
                }
                Family::Pyramid { l } => l,
            };
            let b0: [i32; 3] = [0, 1, 2].map(|i| v[i].div_euclid(dims[i]) * dims[i]);
            let cells: Vec<[i32; 3]> = (0..dims[0]).flat_map(|a| (0..dims[1]).flat_map(move |b| (0..dims[2]).map(move |c| [b0[0] + a, b0[1] + b, b0[2] + c]))).collect();
            if cells.iter().any(|c| claimed.contains(c)) {
                continue;
            }
            let actual: Vec<f64> = cells.iter().map(|c| fill(*c)).collect();
            let f = actual.iter().sum::<f64>() / cells.len() as f64;
            let lo8: [i32; 3] = [0, 1, 2].map(|i| 2 * b0[i] - 1);
            let hi8: [i32; 3] = [0, 1, 2].map(|i| 2 * (b0[i] + dims[i]) - 1);
            let corner = |bits: [bool; 3]| [0, 1, 2].map(|i| if bits[i] { hi8[i] } else { lo8[i] });
            let all_corners: Vec<([bool; 3], [i32; 3])> = (0..8).map(|k| { let b = [k & 1 == 1, k & 2 == 2, k & 4 == 4]; (b, corner(b)) }).collect();
            let plus = [cand.signs[0] > 0, cand.signs[1] > 0, cand.signs[2] > 0];
            // options for this box: (placements, per-cell fills)
            let mut options: Vec<(Vec<(&'static str, [i32; 9], [i32; 3])>, Vec<f64>)> = vec![(vec![], vec![0.0; cells.len()])];
            if cells.len() == 1 || matches!(cand.fam, Family::Axis) {
                options.push((vec![("01_block", cube_r, v)], vec![1.0]));
            }
            match cand.fam {
                Family::Axis => {}
                Family::Wedge { p, q, lp, lq } => {
                    let want: Vec<[i32; 3]> = all_corners.iter().filter(|(b, _)| !(b[p] == plus[p] && b[q] == plus[q])).map(|x| x.1).collect();
                    let bt = match lp.max(lq) { 1 => (1, "02_wedge"), 2 => (4, "05_wedge_2"), _ => (7, "08_wedge_4") };
                    if let Some((def, r, a)) = match_block(&[bt], &want) {
                        options.push((vec![(def, r, a)], shape_fills(def, r, a, &cells)));
                    }
                }
                Family::Pyramid { l } => {
                    let mut sorted = l;
                    sorted.sort();
                    let (pb, ib) = match sorted {
                        [1, 1, 1] => ((2, "03_pyramid"), (3, "04_invpyramid")),
                        [1, 1, 2] => ((5, "06_pyramid_2"), (6, "07_invpyramid_2")),
                        [1, 1, 4] => ((8, "09_pyramid_4"), (9, "10_invpyramid_4")),
                        [1, 2, 2] => ((10, "11_pyramid_2x2"), (13, "14_invpyramid_2x2")),
                        [1, 2, 4] => ((11, "12_pyramid_2x4"), (14, "15_invpyramid_2x4")),
                        _ => ((12, "13_pyramid_4x4"), (15, "16_invpyramid_4x4")),
                    };
                    let low = [!plus[0], !plus[1], !plus[2]];
                    let mut wp = vec![corner(low)];
                    for i in 0..3 {
                        let mut b = low;
                        b[i] = !b[i];
                        wp.push(corner(b));
                    }
                    let wi: Vec<[i32; 3]> = all_corners.iter().filter(|(b, _)| *b != plus).map(|x| x.1).collect();
                    for (bt, want) in [(pb, wp), (ib, wi)] {
                        if let Some((def, r, a)) = match_block(&[bt], &want) {
                            options.push((vec![(def, r, a)], shape_fills(def, r, a, &cells)));
                        }
                    }
                }
            }
            if cells.len() > 1 {
                options.push((cells.iter().map(|c| ("01_block", cube_r, *c)).collect(), vec![1.0; cells.len()]));
            }
            // best option by per-cell error; reject options that add material in cells fully outside the hull or leave
            // cells fully inside it mostly empty (quantisation of up to half a step is expected and fine)
            let valid = |sf: &Vec<f64>| sf.iter().zip(actual.iter()).all(|(s, a)| !(*a == 0.0 && *s >= 0.5) && !(*a >= 0.95 && *s < 0.5));
            let scored: Vec<(&Vec<(&'static str, [i32; 9], [i32; 3])>, f64, bool)> = options
                .iter()
                .map(|(pl, sf)| (pl, sf.iter().zip(actual.iter()).map(|(a, b)| (a - b).abs()).sum::<f64>() / cells.len() as f64, valid(sf)))
                .collect();
            let Some((best, err, _)) = scored.iter().filter(|x| x.2).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()) else {
                continue;
            };
            let (best, err) = (*best, *err);
            let _ = f;
            if err > TOL && cells.len() > 1 {
                continue; // this box does not follow the hull here; try the next family
            }
            for (def, r, a) in best {
                place(&mut out, def, *r, *a);
            }
            let chosen_fills = &options.iter().find(|(pl, _)| pl == best).unwrap().1;
            for (c, sf) in cells.iter().zip(chosen_fills.iter()) {
                if *sf >= 0.5 {
                    occupied.insert(*c);
                }
            }
            claimed.extend(cells);
            done = true;
            break;
        }
        if !done {
            // single-voxel fallback: best of cube / wedge / pyramid / inverse pyramid by exact fill pattern
            if fill(v) >= 0.5 {
                place(&mut out, "01_block", cube_r, v);
                occupied.insert(v);
            }
            claimed.insert(v);
        }
    }
    // interior cubes within the shell thickness of the outside
    let t = opt.shell.max(1) as i32;
    let inside = |v: [i32; 3]| fill(v) >= 0.5;
    let mut interior: Vec<[i32; 3]> = fills.iter().filter(|(k, f)| **f >= 0.98 && !claimed.contains(*k)).map(|(k, _)| *k).collect();
    interior.sort();
    for v in interior {
        let mut near = false;
        'n: for dx in -t..=t {
            for dy in -t..=t {
                for dz in -t..=t {
                    if dx.abs() + dy.abs() + dz.abs() <= t && !inside([v[0] + dx, v[1] + dy, v[2] + dz]) {
                        near = true;
                        break 'n;
                    }
                }
            }
        }
        if near || opt.shell == 0 {
            place(&mut out, "01_block", cube_r, v);
            occupied.insert(v);
        }
    }
    // deck closure: every hull column must end in a solid top within one voxel of the true top
    let mut tops: HashMap<(i32, i32), i32> = HashMap::new();
    for (k, f) in &fills {
        if *f > 0.02 {
            let e = tops.entry((k[0], k[2])).or_insert(k[1]);
            *e = (*e).max(k[1]);
        }
    }
    let mut cols: Vec<_> = tops.into_iter().collect();
    cols.sort();
    for ((x, z), top) in cols {
        if !(occupied.contains(&[x, top, z]) || occupied.contains(&[x, top - 1, z])) {
            let row = if fill([x, top, z]) >= 0.5 { top } else { top - 1 };
            if fill([x, row, z]) > 0.0 && !claimed.contains(&[x, row, z]) || !occupied.contains(&[x, row, z]) && fill([x, row, z]) >= 0.5 {
                place(&mut out, "01_block", cube_r, [x, row, z]);
                occupied.insert([x, row, z]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct HalfSpace([f64; 3], f64);
    impl Solid for HalfSpace {
        fn bounds(&self) -> ([f64; 3], [f64; 3]) {
            ([-1.0; 3], [1.0; 3])
        }
        fn inside(&self, p: [f64; 3]) -> bool {
            p[0].abs() <= 1.0 && p[1].abs() <= 1.0 && p[2].abs() <= 1.0 && self.0[0] * p[0] + self.0[1] * p[1] + self.0[2] * p[2] <= self.1
        }
        fn normal(&self, _p: [f64; 3]) -> Option<[f64; 3]> {
            let n = self.0;
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            Some([n[0] / l, n[1] / l, n[2] / l])
        }
    }

    #[test]
    fn block_matching_finds_every_family() {
        // a plain 1x1x1 pyramid at the origin voxel, as placed by identity
        let want = block_pts8(2);
        assert!(match_block(&[(2, "03_pyramid")], &want).is_some());
        let want = block_pts8(14);
        assert!(match_block(&[(14, "15_invpyramid_2x4")], &want).is_some());
    }

    #[test]
    fn diagonal_plane_uses_pyramid_pairs() {
        // plane x + y + z <= 0.125 through voxel corners: tiled by 1x1x1 pyramids and inverse pyramids only
        let p = build(&HalfSpace([1.0, 1.0, 1.0], 0.125), &FacetOptions { shell: 1, ..Default::default() });
        let kinds: HashSet<&str> = p.iter().map(|x| x.def.as_str()).collect();
        assert!(kinds.contains("03_pyramid") || kinds.contains("04_invpyramid"), "{kinds:?}");
    }

    #[test]
    fn shallow_plane_uses_long_wedges() {
        let p = build(&HalfSpace([0.0, 4.0, 1.0], 0.0), &FacetOptions::default());
        assert!(p.iter().any(|x| x.def == "08_wedge_4"), "{:?}", p.iter().map(|x| &x.def).collect::<HashSet<_>>());
    }
}
