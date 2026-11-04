//! Label-consistent block tiling (docs/engineering/hull-to-blocks.md §3.5).
//!
//! The model solid is sampled at lattice corners (inside / outside labels). Any block, of any type and rotation,
//! whose own corner labels match the model's on every lattice corner of its footprint is a valid piece: blocks
//! that agree on the labels of a shared face join without holes. Many blocks fit the same labels. A 1 × 4 wedge has
//! the same labels as "flat, flat, flat, 1 × 1 wedge", so choosing the largest consistent blocks turns terraces into
//! smooth slopes wherever the labels allow.
use crate::lattice::ALL_BTS;
use crate::voxel::Placement;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// One block type in one rotation, anchored at voxel (0, 0, 0).
pub struct Template {
    pub def: &'static str,
    pub r: [i32; 9],
    /// voxel offsets of the footprint
    pub cells: Vec<[i32; 3]>,
    /// every lattice corner of the footprint (eighth-metre units, relative to the anchor) with its label
    pub corners: Vec<([i32; 3], bool)>,
    /// per footprint cell, the 8-bit corner mask (bit k = corner (k&1, k>>1&1, k>>2&1))
    pub masks: Vec<u8>,
    /// solid volume in voxels
    pub volume: f64,
}

fn signed_perms() -> Vec<[i32; 9]> {
    let mut out = Vec::new();
    for perm in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
        for s in 0..8 {
            let mut r = [0; 9];
            for j in 0..3 {
                r[3 * j + perm[j]] = if s >> j & 1 == 1 { -1 } else { 1 };
            }
            out.push(r);
        }
    }
    out
}

impl Template {
    /// Mirror-symmetric about the x = 0 plane through the anchor voxel (allowed in the shared centre column).
    pub fn symmetric_x(&self) -> bool {
        let set: HashSet<([i32; 3], bool)> = self.corners.iter().copied().collect();
        self.corners.iter().all(|(q, b)| set.contains(&([-q[0], q[1], q[2]], *b)))
    }
}

pub fn templates() -> &'static Vec<Template> {
    static T: OnceLock<Vec<Template>> = OnceLock::new();
    T.get_or_init(|| {
        let mut out: Vec<Template> = vec![];
        let mut seen: HashSet<Vec<([i32; 3], bool)>> = HashSet::new();
        for &(bt, def) in ALL_BTS.iter() {
            let base: Vec<[f64; 3]> = keel_model::geometry::block_points(bt).unwrap().into_iter().map(|p| p.map(|x| (x * 8.0).round())).collect();
            for r in signed_perms() {
                let pts: Vec<[f64; 3]> = base.iter().map(|p| [0, 1, 2].map(|i| r[i] as f64 * p[0] + r[3 + i] as f64 * p[1] + r[6 + i] as f64 * p[2])).collect();
                let faces = keel_model::geometry::convex_hull_faces(&pts);
                let inside = |q: [i32; 3]| faces.iter().all(|(n, f)| n[0] * (q[0] as f64 - f[0][0]) + n[1] * (q[1] as f64 - f[0][1]) + n[2] * (q[2] as f64 - f[0][2]) <= 1e-6);
                let lo = [0, 1, 2].map(|i| pts.iter().map(|p| p[i]).fold(f64::MAX, f64::min).round() as i32);
                let hi = [0, 1, 2].map(|i| pts.iter().map(|p| p[i]).fold(f64::MIN, f64::max).round() as i32);
                let cl = [0, 1, 2].map(|i| (lo[i] + 1) / 2);
                let ch = [0, 1, 2].map(|i| (hi[i] - 1) / 2);
                let mut cells = vec![];
                for x in cl[0]..=ch[0] {
                    for y in cl[1]..=ch[1] {
                        for z in cl[2]..=ch[2] {
                            cells.push([x, y, z]);
                        }
                    }
                }
                let mut corners = vec![];
                for x in (lo[0]..=hi[0]).step_by(2) {
                    for y in (lo[1]..=hi[1]).step_by(2) {
                        for z in (lo[2]..=hi[2]).step_by(2) {
                            corners.push(([x, y, z], inside([x, y, z])));
                        }
                    }
                }
                let mut key = corners.clone();
                key.sort();
                if !seen.insert(key) {
                    continue;
                }
                let masks = cells
                    .iter()
                    .map(|c| {
                        (0..8).fold(0u8, |m, k| {
                            let q = [2 * c[0] - 1 + 2 * (k & 1), 2 * c[1] - 1 + 2 * ((k >> 1) & 1), 2 * c[2] - 1 + 2 * ((k >> 2) & 1)];
                            if inside(q) { m | (1 << k) } else { m }
                        })
                    })
                    .collect();
                // volume of a convex polytope from its faces (divergence theorem), in voxels (8³ e-units)
                let mut vol = 0.0;
                for (n, f) in &faces {
                    let mut area = [0.0; 3];
                    for i in 0..f.len() {
                        let (a, b) = (f[i], f[(i + 1) % f.len()]);
                        area[0] += a[1] * b[2] - a[2] * b[1];
                        area[1] += a[2] * b[0] - a[0] * b[2];
                        area[2] += a[0] * b[1] - a[1] * b[0];
                    }
                    let a = 0.5 * (area[0] * n[0] + area[1] * n[1] + area[2] * n[2]);
                    vol += a * (n[0] * f[0][0] + n[1] * f[0][1] + n[2] * f[0][2]) / 3.0;
                }
                out.push(Template { def, r, cells, corners, masks, volume: vol / 8.0 });
            }
        }
        out
    })
}

/// Tile the shell (starboard half; labels are mirror-symmetric, so x is taken as |x|). `label(e)` is the model's inside
/// test at lattice corner e (eighth-metre units). `shell` lists the voxels with a corner inside. `cube_veto(v)` can
/// leave out shell cubes (open hull). `allowed(template, anchor)` can veto placements (e.g. pyramids in the centre
/// column). `align(cells)` scores how well a footprint sits on the model's tile grid (0..1), used to break ties between
/// equally large candidates. Returns placements with the voxels each covers.
///
/// 1. Exact pass: every block whose labels match the model, largest first.
/// 2. Builder pass: a long block (≥ 4 voxels) that disagrees with the model on at most two corners is placed anyway,
///    and those corner labels are changed to match it. A corner may only change when every voxel touching it is part
///    of the new block or still free, so all placed blocks stay consistent and the shell stays closed. This is where a
///    builder would put a 2 × 4 pyramid instead of a cluster of 1 × 1 pieces.
/// 3. The remaining partial voxels take the nearest 1-voxel block; full voxels on the shell become cubes.
pub fn tile(
    label: &dyn Fn([i32; 3]) -> bool,
    shell: &[[i32; 3]],
    cube_veto: &dyn Fn([i32; 3]) -> bool,
    allowed: &dyn Fn(&Template, [i32; 3]) -> bool,
    align: &dyn Fn(&[[i32; 3]]) -> f64,
) -> Vec<(Placement, Vec<[i32; 3]>)> {
    let key = |e: [i32; 3]| [e[0].abs(), e[1], e[2]];
    let mut lab: HashMap<[i32; 3], bool> = HashMap::new();
    let get = |lab: &mut HashMap<[i32; 3], bool>, e: [i32; 3]| -> bool { *lab.entry(key(e)).or_insert_with(|| label(key(e))) };
    let corner = |v: [i32; 3], k: i32| [2 * v[0] - 1 + 2 * (k & 1), 2 * v[1] - 1 + 2 * ((k >> 1) & 1), 2 * v[2] - 1 + 2 * ((k >> 2) & 1)];
    let mask_of = |lab: &mut HashMap<[i32; 3], bool>, v: [i32; 3]| -> u8 { (0..8).fold(0u8, |m, k| if get(lab, corner(v, k)) { m | (1 << k) } else { m }) };
    let ts = templates();
    let mut used: HashSet<[i32; 3]> = HashSet::new();
    let mut out = vec![];
    let cells_at = |t: &Template, a: [i32; 3]| -> Vec<[i32; 3]> { t.cells.iter().map(|c| [c[0] + a[0], c[1] + a[1], c[2] + a[2]]).collect() };
    for pass in 0..1 {
        let masks: HashMap<[i32; 3], u8> = shell.iter().map(|v| (*v, mask_of(&mut lab, *v))).collect();
        // candidates: a template placed so that one of its cells lands on a free partial shell voxel with the same mask
        let mut cands: Vec<(f64, usize, [i32; 3])> = vec![];
        let mut tried: HashSet<(usize, [i32; 3])> = HashSet::new();
        for v in shell {
            let m = masks[v];
            if m == 0 || m == 255 || used.contains(v) {
                continue;
            }
            for (ti, t) in ts.iter().enumerate() {
                if pass == 1 && t.cells.len() < 4 {
                    continue;
                }
                for (ci, c) in t.cells.iter().enumerate() {
                    if pass == 0 && t.masks[ci] != m {
                        continue;
                    }
                    if pass == 1 && (t.masks[ci] ^ m).count_ones() > 2 {
                        continue;
                    }
                    let anchor = [v[0] - c[0], v[1] - c[1], v[2] - c[2]];
                    if !tried.insert((ti, anchor)) || !allowed(t, anchor) {
                        continue;
                    }
                    let mism = t.corners.iter().filter(|(q, ins)| get(&mut lab, [q[0] + 2 * anchor[0], q[1] + 2 * anchor[1], q[2] + 2 * anchor[2]]) != *ins).count();
                    if mism == 0 && pass == 0 {
                        // bigger first; among equal sizes, footprints on the model's own tile grid first (a plane with
                        // equal gradients fits many shifted boxes, and mixing shifts leaves gaps)
                        let cells: Vec<[i32; 3]> = t.cells.iter().map(|c| [c[0] + anchor[0], c[1] + anchor[1], c[2] + anchor[2]]).collect();
                        cands.push((t.cells.len() as f64 + 0.5 * align(&cells) + 0.01 * t.volume, ti, anchor));
                    } else if pass == 1 && mism <= 2 {
                        cands.push((t.cells.len() as f64 - 1.5 * mism as f64 + 0.01 * t.volume, ti, anchor));
                    }
                }
            }
        }
        cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let n_before = out.len();
        let n_cands = cands.len();
        for (_, ti, anchor) in cands {
            let t = &ts[ti];
            let cells = cells_at(t, anchor);
            if cells.iter().any(|c| used.contains(c)) {
                continue;
            }
            // corners to change (pass 1): every voxel touching them must be one of ours or free and on the shell
            let flips: Vec<([i32; 3], bool)> = t
                .corners
                .iter()
                .map(|(q, ins)| ([q[0] + 2 * anchor[0], q[1] + 2 * anchor[1], q[2] + 2 * anchor[2]], *ins))
                .filter(|(q, ins)| get(&mut lab, *q) != *ins)
                .collect();
            if pass == 0 && !flips.is_empty() {
                continue;
            }
            let own: HashSet<[i32; 3]> = cells.iter().copied().collect();
            // every other voxel touching a changed corner must stay without volume after the change
            let flipped: HashMap<[i32; 3], bool> = flips.iter().map(|(q, b)| (key(*q), *b)).collect();
            let mut ok = true;
            for (q, _) in &flips {
                for k in 0..8 {
                    let v = [(q[0] + 1) / 2 - (k & 1), (q[1] + 1) / 2 - ((k >> 1) & 1), (q[2] + 1) / 2 - ((k >> 2) & 1)];
                    // the port half mirrors the starboard half: voxel -i is voxel i
                    let v = if v[0] < 0 { [-v[0], v[1], v[2]] } else { v };
                    if own.contains(&v) {
                        continue;
                    }
                    if used.contains(&v) {
                        ok = false;
                        break;
                    }
                    let m = (0..8).fold(0u8, |m, kk| {
                        let c = key(corner(v, kk));
                        let inside = match flipped.get(&c) { Some(b) => *b, None => get(&mut lab, c) };
                        if inside { m | (1 << kk) } else { m }
                    });
                    if !(m.count_ones() < 4 || [0x0f, 0xf0, 0x33, 0xcc, 0x55, 0xaa].contains(&m)) {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if !ok {
                continue;
            }
            for (q, ins) in flips {
                lab.insert(key(q), ins);
            }
            for c in &cells {
                used.insert(*c);
            }
            out.push((Placement { def: t.def.into(), vp: anchor, r: t.r }, cells));
        }
        if std::env::var_os("KEEL_TILE_DEBUG").is_some() {
            eprintln!("tiling pass {pass}: {n_cands} candidates, {} placed", out.len() - n_before);
        }
    }
    let masks: HashMap<[i32; 3], u8> = shell.iter().map(|v| (*v, mask_of(&mut lab, *v))).collect();
    // partial voxels left over (corner sets that are no block): the nearest allowed 1-voxel block, supersets first so
    // the shell stays closed
    for v in shell {
        let m = masks[v];
        if m == 0 || m == 255 || used.contains(v) || m.count_ones() < 4 || [0x0f, 0xf0, 0x33, 0xcc, 0x55, 0xaa].contains(&m) {
            continue;
        }
        let best = ts
            .iter()
            .filter(|t| t.cells.len() == 1 && allowed(t, *v))
            .min_by_key(|t| ((t.masks[0] ^ m).count_ones() * 2 + if t.masks[0] & m == m { 0 } else { 1 }, t.masks[0].count_ones()));
        if let Some(t) = best {
            used.insert(*v);
            out.push((Placement { def: t.def.into(), vp: *v, r: t.r }, vec![*v]));
        }
    }
    // full shell voxels: cubes (a face neighbour is not full)
    let nb = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
    for v in shell {
        if masks[v] != 255 || used.contains(v) || cube_veto(*v) {
            continue;
        }
        if nb.iter().all(|d| mask_of(&mut lab, [v[0] + d[0], v[1] + d[1], v[2] + d[2]]) == 255) {
            continue;
        }
        used.insert(*v);
        out.push((Placement { def: "01_block".into(), vp: *v, r: keel_io::vehicle::IDENTITY }, vec![*v]));
    }
    out
}
