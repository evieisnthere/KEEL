//! Smooth block hulls: build a hull the way a careful Stormworks builder does, rather than fitting voxels.
//!
//! Each transverse section (one per voxel row along the length) is snapped to a path on the voxel-corner lattice
//! from the keel to the sheer. The path only takes steps that standard blocks can make: flat (1,0), vertical (0,1),
//! 1:1, 2:1 / 1:2 and 4:1 / 1:4. Each sloped step becomes exactly one wedge (02_wedge, 05_wedge_2, 08_wedge_4), so
//! sloped faces meet edge-to-edge with no teeth. The interior behind the contour is filled to the shell thickness,
//! a deck plate closes the top, and the port side mirrors the starboard side.
use crate::hull::Hull;
use crate::voxel::{fill_fraction, oriented_block, rotations, Placement, Solid, VOXEL};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone)]
pub struct BlockHullOptions {
    /// interior thickness behind the sloped skin, in voxels (0 = skin only)
    pub shell: u32,
    /// lay a deck plate at the sheer
    pub deck: bool,
    /// voxel offset added to every placement
    pub origin: [i32; 3],
    /// smooth the steps between sections with fore-aft wedges, pyramids and inverse pyramids
    pub smooth: bool,
    /// build the bow from waterline contours (plan-view slopes)
    pub waterline_bow: bool,
}

impl Default for BlockHullOptions {
    fn default() -> Self {
        BlockHullOptions { shell: 1, deck: true, origin: [0, 0, 0], smooth: true, waterline_bow: true }
    }
}

const MOVES: [(i32, i32); 7] = [(1, 0), (0, 1), (1, 1), (2, 1), (1, 2), (4, 1), (1, 4)];

/// Distance from a point to a polyline, plus the index of the nearest segment and its unit tangent.
fn nearest(curve: &[[f64; 2]], p: [f64; 2]) -> (f64, usize, [f64; 2]) {
    let mut best = (f64::MAX, 0, [1.0, 0.0]);
    for (k, w) in curve.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let l2 = d[0] * d[0] + d[1] * d[1];
        if l2 < 1e-12 {
            continue;
        }
        let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0);
        let q = [a[0] + d[0] * t, a[1] + d[1] * t];
        let dist = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt();
        if dist < best.0 {
            let l = l2.sqrt();
            best = (dist, k, [d[0] / l, d[1] / l]);
        }
    }
    best
}

/// Lattice path for one section. `curve` is in lattice units (u = x/voxel − 0.5, v = y/voxel) from keel to sheer.
fn section_path(curve: &[[f64; 2]], v_end: i32) -> Vec<[i32; 2]> {
    let start_v = curve[0][1].round() as i32;
    let mut p = [0i32, start_v];
    let mut path = vec![p];
    let mut seg = 0usize;
    for _ in 0..4000 {
        if p[1] >= v_end {
            break;
        }
        let (_, k0, tan) = nearest(&curve[seg..], [p[0] as f64, p[1] as f64]);
        let k0 = k0 + seg;
        let mut best: Option<(f64, (i32, i32))> = None;
        for &(a, b) in &MOVES {
            let e = [p[0] + a, p[1] + b];
            if e[1] > v_end {
                continue;
            }
            let (d, k, _) = nearest(&curve[seg..], [e[0] as f64, e[1] as f64]);
            if k + seg + 1 < k0 {
                continue; // never go backwards along the curve
            }
            let len = ((a * a + b * b) as f64).sqrt();
            let align = (a as f64 * tan[0] + b as f64 * tan[1]) / len;
            // stay on the curve, follow its direction, prefer long smooth steps
            let score = d + 0.6 * (1.0 - align) - 0.04 * len;
            if best.is_none_or(|x| score < x.0) {
                best = Some((score, (a, b)));
            }
        }
        let Some((_, (a, b))) = best else { break };
        p = [p[0] + a, p[1] + b];
        seg = nearest(curve, [p[0] as f64, p[1] as f64]).1.saturating_sub(1).max(seg);
        path.push(p);
    }
    path
}

fn mat_apply(r: &[i32; 9], v: [i32; 3]) -> [i32; 3] {
    [0, 1, 2].map(|i| r[i] * v[0] + r[3 + i] * v[1] + r[6 + i] * v[2])
}

/// Rotation and anchor for a wedge of length `len` covering `cells` (voxel coords) with outward normal `n`.
fn place_wedge(len: i32, cells: &[[i32; 3]], n: [f64; 3]) -> Option<Placement> {
    let def = match len {
        1 => "02_wedge",
        2 => "05_wedge_2",
        _ => "08_wedge_4",
    };
    let ln = [0.0, len as f64, -1.0];
    let ll = (ln[1] * ln[1] + 1.0).sqrt();
    let want: HashSet<[i32; 3]> = cells.iter().copied().collect();
    for r in rotations() {
        let wn = [0, 1, 2].map(|i| (r[3 + i] as f64 * ln[1] + r[6 + i] as f64 * ln[2]) / ll);
        if wn[0] * n[0] + wn[1] * n[1] + wn[2] * n[2] < 0.999 {
            continue;
        }
        for anchor in cells {
            let cover: HashSet<[i32; 3]> = (0..len).map(|i| { let o = mat_apply(r, [0, 0, -i]); [anchor[0] + o[0], anchor[1] + o[1], anchor[2] + o[2]] }).collect();
            if cover == want {
                return Some(Placement { def: def.to_string(), vp: *anchor, r: *r });
            }
        }
    }
    None
}

pub fn build(hull: &Hull, opt: &BlockHullOptions) -> Vec<Placement> {
    let p = &hull.params;
    let mut out = Vec::new();
    let mut solid3: HashSet<[i32; 3]> = HashSet::new();
    let mut interior3: HashSet<[i32; 3]> = HashSet::new();
    let mut skin3: Vec<(i32, Placement)> = Vec::new();
    let mut prev_path: Option<Vec<[i32; 2]>> = None;
    let nz = (p.length / VOXEL).round() as i32;
    let z0 = -(nz / 2);
    // bow region: from the start of the entrance forward, the surface faces mostly forward, so it is built from
    // waterline (plan-view) contours instead of sections
    let zb = (((1.0 - p.entrance.clamp(0.05, 0.95)) - 0.5) * p.length / VOXEL).round() as i32;
    let zb = if opt.waterline_bow { zb } else { i32::MAX };
    for zi in 0..nz {
        let zv = z0 + zi;
        if zv >= zb {
            break;
        }
        let z = zv as f64 * VOXEL;
        let Some(sec) = hull.section_at(z, 48) else { continue };
        let curve: Vec<[f64; 2]> = sec.iter().map(|q| [(q[0] / VOXEL - 0.5).max(0.0), q[1] / VOXEL]).collect();
        let v_end = (sec[sec.len() - 1][1] / VOXEL).round() as i32;
        let start_v = curve[0][1].round() as i32;
        if v_end <= start_v {
            continue;
        }
        // coherence along the length: keep the previous section's contour while it still fits this section,
        // so runs of identical sections form long smooth planks instead of shingles
        let path = match &prev_path {
            Some(pp) if pp.first().map(|q| q[1]) == Some(start_v) && pp.last().map(|q| q[1]) == Some(v_end) && pp.iter().all(|q| nearest(&curve, [q[0] as f64, q[1] as f64]).0 < 0.6) => pp.clone(),
            _ => section_path(&curve, v_end),
        };
        prev_path = Some(path.clone());
        // boundary wedges and the per-row inner limit of the sloped skin
        let mut skin: HashSet<[i32; 2]> = HashSet::new();
        let mut row_limit: BTreeMap<i32, i32> = BTreeMap::new(); // row -> first lattice u reached by the path
        let mut wedges: Vec<(i32, Vec<[i32; 2]>, [f64; 2])> = Vec::new();
        for w in path.windows(2) {
            let (a, b) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let (i, j) = (w[0][0], w[0][1]);
            for row in j..j.max(j + b).max(j + 1) {
                let e = row_limit.entry(row).or_insert(i32::MAX);
                *e = (*e).min(i);
            }
            if a > 0 && b > 0 {
                let len = a.max(b);
                let cells: Vec<[i32; 2]> = if a >= b { (0..a).map(|k| [i + k + 1, j]).collect() } else { (0..b).map(|k| [i + 1, j + k]).collect() };
                let l = ((a * a + b * b) as f64).sqrt();
                wedges.push((len, cells.clone(), [b as f64 / l, -a as f64 / l]));
                skin.extend(cells);
            }
        }
        // interior cells: from the centre column out to the path in each row (voxel x = lattice u + 1 boundary)
        let mut interior: HashSet<[i32; 2]> = HashSet::new();
        for (&row, &u) in &row_limit {
            if row >= v_end {
                continue;
            }
            for c in 0..=u {
                if !skin.contains(&[c, row]) {
                    interior.insert([c, row]);
                }
            }
        }
        let deck_row = v_end - 1;
        for &c in &interior {
            if !opt.deck && c[1] == deck_row {
                continue;
            }
            for side in if c[0] == 0 { vec![1] } else { vec![1, -1] } {
                let v = [c[0] * side, c[1], zv];
                interior3.insert(v);
                solid3.insert(v);
            }
        }
        for (len, cells, n2) in wedges {
            for side in [1, -1] {
                let cells3: Vec<[i32; 3]> = cells.iter().map(|c| [c[0] * side, c[1], zv]).collect();
                solid3.extend(cells3.iter().copied());
                let n = [n2[0] * side as f64, n2[1], 0.0];
                match place_wedge(len, &cells3, n) {
                    Some(pl) => skin3.push((len, pl)),
                    None => {
                        if std::env::var_os("KEEL_DEBUG_BLOCKHULL").is_some() {
                            eprintln!("wedge failed len={len} side={side} cells={cells3:?} n={n:?}");
                        }
                    }
                }
            }
        }
    }

    if zb < z0 + nz {
        waterline_bow(hull, zb, z0 + nz, &mut solid3, &mut interior3, &mut skin3);
    }

    // 3D shell: keep interior cells within `shell` steps of the outside (across sections too, so neighbouring
    // sections whose contours differ never leave gaps)
    let t = opt.shell.max(1) as i32;
    let near_outside = |c: [i32; 3]| -> bool {
        for dx in -t..=t {
            for dy in -t..=t {
                for dz in -t..=t {
                    if dx.abs() + dy.abs() + dz.abs() > t {
                        continue;
                    }
                    if !solid3.contains(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        return true;
                    }
                }
            }
        }
        false
    };
    let exposed_z = |c: [i32; 3]| !solid3.contains(&[c[0], c[1], c[2] + 1]) || !solid3.contains(&[c[0], c[1], c[2] - 1]);
    let solid = crate::HullSolid::new(hull);
    let world = |c: [i32; 3]| [c[0] as f64 * VOXEL, c[1] as f64 * VOXEL + 0.125, c[2] as f64 * VOXEL]; // hull frame: row j spans y = j..j+1 voxels
    let o = opt.origin;
    let at = |c: [i32; 3]| [c[0] + o[0], c[1] + o[1], c[2] + o[2]];
    let id = keel_io::vehicle::IDENTITY;
    let mut cubes: Vec<[i32; 3]> = interior3.iter().copied().filter(|c| opt.shell == 0 || near_outside(*c)).collect();
    cubes.sort();
    for c in cubes {
        // longitudinal smoothing: a cube whose fore/aft face is exposed follows the hull's true surface instead
        if opt.smooth && exposed_z(c) {
            let w = world(c);
            if let Some(n) = solid.normal(w) {
                if n[2].abs() > 0.25 {
                    let f = fill_fraction(&solid, w);
                    let mut a = n.map(f64::abs);
                    a.sort_by(|x, y| y.partial_cmp(x).unwrap());
                    let corner = a[2] > 0.3;
                    let choice = if corner {
                        if f < 0.08 { None } else if f < 0.45 { Some("03_pyramid") } else if f < 0.94 { Some("04_invpyramid") } else { Some("01_block") }
                    } else if f < 0.2 {
                        None
                    } else if f < 0.85 {
                        Some("02_wedge")
                    } else {
                        Some("01_block")
                    };
                    match choice {
                        Some("01_block") => out.push(Placement { def: "01_block".into(), vp: at(c), r: id }),
                        Some(d) => out.push(Placement { def: d.into(), vp: at(c), r: oriented_block(d, n) }),
                        None => {}
                    }
                    continue;
                }
            }
        }
        out.push(Placement { def: "01_block".into(), vp: at(c), r: id });
    }
    for (len, mut pl) in skin3 {
        if opt.smooth && len == 1 && exposed_z(pl.vp) {
            let w = world(pl.vp);
            if let Some(n) = solid.normal(w) {
                let mut a = n.map(f64::abs);
                a.sort_by(|x, y| y.partial_cmp(x).unwrap());
                if n[2].abs() > 0.25 && a[2] > 0.3 && fill_fraction(&solid, w) < 0.4 {
                    pl = Placement { def: "03_pyramid".into(), vp: pl.vp, r: oriented_block("03_pyramid", n) };
                }
            }
        }
        pl.vp = at(pl.vp);
        out.push(pl);
    }
    out
}


/// Bow from waterlines. For each height row the planform half-breadth x(z), from the bow-region start to the stem, is
/// snapped to a lattice path in the (z, inward-x) plane with the same slope steps as sections. Each sloped step is
/// one wedge lying in the horizontal plane. Cells behind the path (toward midship and the centreline) are interior.
fn waterline_bow(hull: &Hull, zb: i32, z_end: i32, solid3: &mut HashSet<[i32; 3]>, interior3: &mut HashSet<[i32; 3]>, skin3: &mut Vec<(i32, Placement)>) {
    let p = &hull.params;
    let rows_top = ((p.depth + p.sheer_bow.max(0.0)) / VOXEL).ceil() as i32 + 1;
    // sections along the bow region, cached
    let secs: Vec<(i32, Option<Vec<[f64; 2]>>)> = (zb..=z_end + 1).map(|zv| (zv, hull.section_at(zv as f64 * VOXEL, 48))).collect();
    let breadth = |y: f64, zv: i32| -> Option<f64> {
        let (_, s) = secs.iter().find(|(z, _)| *z == zv)?;
        let s = s.as_ref()?;
        if y < s[0][1] || y > s[s.len() - 1][1] {
            return None;
        }
        Hull::half_breadth(s, y)
    };
    for j in 0..rows_top {
        let y = (j as f64 + 0.5) * VOXEL; // centre of row j in the hull frame
        // planform samples (lattice w = z/voxel + 0.5, u = x/voxel - 0.5)
        let mut pts: Vec<[f64; 2]> = Vec::new();
        let mut zstart = None;
        let mut sub = 0.0;
        while sub <= (z_end - zb) as f64 + 0.999 {
            let zf = zb as f64 + sub;
            let zv = zf.floor() as i32;
            let frac = zf - zv as f64;
            let b0 = breadth(y, zv);
            let b1 = breadth(y, zv + 1);
            if let (Some(a), Some(b)) = (b0, b1) {
                let b = a + (b - a) * frac;
                zstart.get_or_insert(zf);
                pts.push([zf + 0.5, (b / VOXEL - 0.5).max(-0.5)]);
            } else if zstart.is_some() {
                break;
            }
            sub += 0.25;
        }
        if pts.len() < 2 {
            continue;
        }
        // map to path frame: p = w - w0 (forward), q = u0 - u (inward); start on the lattice
        let w0 = pts[0][0].round();
        let u0 = pts[0][1].round();
        let curve: Vec<[f64; 2]> = pts.iter().map(|q| [q[0] - w0, u0 - q[1]]).collect();
        let q_end = (u0 + 0.5).round() as i32; // stem: half-breadth reaches the centre column edge
        let path = section_path(&curve, q_end);
        // interior: for each inward row q (fixed x cell), the cells behind the path in z
        let mut row_limit: BTreeMap<i32, i32> = BTreeMap::new();
        let mut wedges: Vec<(i32, Vec<[i32; 2]>, [f64; 2])> = Vec::new();
        let mut skin2: HashSet<[i32; 2]> = HashSet::new();
        for w in path.windows(2) {
            let (a, b) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let (i, k) = (w[0][0], w[0][1]);
            for row in k..k.max(k + b).max(k + 1) {
                let e = row_limit.entry(row).or_insert(i32::MAX);
                *e = (*e).min(i);
            }
            if a > 0 && b > 0 {
                let len = a.max(b);
                let cells: Vec<[i32; 2]> = if a >= b { (0..a).map(|t| [i + t, k]).collect() } else { (0..b).map(|t| [i, k + t]).collect() };
                let l = ((a * a + b * b) as f64).sqrt();
                wedges.push((len, cells.clone(), [b as f64 / l, -a as f64 / l]));
                skin2.extend(cells);
            }
        }
        let w0i = w0 as i32;
        let u0i = u0 as i32;
        // (p, q) cell -> voxel (x = u0 - q, z = w0 + p); cell q spans u in [u0-q-1, u0-q] => voxel x = u0 - q
        let to_vox = |c: [i32; 2], side: i32| [(u0i - c[1]) * side, j, w0i + c[0]];
        for (&q, &plim) in &row_limit {
            let x = u0i - q;
            if x < 0 {
                continue;
            }
            for zc in (zb - w0i)..plim {
                let c = [zc, q];
                if skin2.contains(&c) {
                    continue;
                }
                for side in if x == 0 { vec![1] } else { vec![1, -1] } {
                    let v = to_vox(c, side);
                    interior3.insert(v);
                    solid3.insert(v);
                }
            }
        }
        for (len, cells, n2) in wedges {
            for side in [1, -1] {
                let cells3: Vec<[i32; 3]> = cells.iter().map(|c| to_vox(*c, side)).collect();
                if cells3.iter().any(|c| c[0] * side < 0) {
                    continue;
                }
                solid3.extend(cells3.iter().copied());
                // outward normal: +p is +z, +q is -x (inward)  =>  n = (side * a, 0, b) with (b, -a) in (p, q)
                let n = [-n2[1] * side as f64, 0.0, n2[0]];
                if let Some(pl) = place_wedge(len, &cells3, n) {
                    skin3.push((len, pl));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_follows_a_45_degree_line_with_wedges_only() {
        let curve: Vec<[f64; 2]> = (0..=20).map(|i| [i as f64 * 0.5, i as f64 * 0.5]).collect();
        let path = section_path(&curve, 10);
        for w in path.windows(2) {
            assert_eq!((w[1][0] - w[0][0], w[1][1] - w[0][1]), (1, 1));
        }
    }

    #[test]
    fn shallow_bottom_uses_long_wedges() {
        let curve: Vec<[f64; 2]> = (0..=40).map(|i| [i as f64 * 0.5, i as f64 * 0.125]).collect();
        let path = section_path(&curve, 5);
        assert!(path.windows(2).all(|w| (w[1][0] - w[0][0], w[1][1] - w[0][1]) == (4, 1)), "{path:?}");
    }

    #[test]
    fn default_hull_builds_with_wedges_and_deck() {
        let h = crate::hull::HullParams::default().build().unwrap();
        let p = build(&h, &BlockHullOptions::default());
        assert!(p.len() > 500);
        assert!(p.iter().any(|x| x.def == "08_wedge_4"));
        assert!(p.iter().any(|x| x.def == "02_wedge"));
        // no two placements share an origin voxel
        let mut seen = HashSet::new();
        assert!(p.iter().all(|x| seen.insert(x.vp)));
    }
}
