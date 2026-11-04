//! Vehicle render geometry.
//!
//! * Generated blocks (definition type 0, `block_type` 0–15) are exact solids. Types 0–9 are the convex hulls the game
//!   builds in `0x14031ab90` (emulated, E1). Types 10–15 (2x2–4x4 pyramids) follow the same corner-pyramid rule, laid
//!   out to fit their voxel sets (E1-inferred). Each hull face is clipped per voxel and painted with the colour of the
//!   definition surface that faces that way in that voxel (`sc` list, index = surface order, `x` → `bc`).
//! * Every other part is drawn from its `.mesh` (`mesh_data_name`, plus moving parts in `mesh_0/1_name`). Vertex colour
//!   keys select paint slots: (255,125,0) → `bc`, (154,125,0) → `bc2`, (55,125,0) → `bc3` (E1-inferred from the corpus).
//! * Coincident block pieces between neighbours are culled. A z-buffered renderer is assumed for everything else.
use crate::catalogue::{Catalogue, Definition, Surface};
use crate::edit::{CompId, CompInfo, Model};
use keel_io::mesh::Mesh;
use keel_io::vehicle::rotate;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SHAPES_JSON: &str = include_str!("../../../research/extracted-metadata/surface_shapes.json");
const CONV_JSON: &str = include_str!("../../../research/extracted-metadata/surface_conversion.json");

/// Buoyancy surface tables (RE-0004). Used by hydrostatics, not by rendering.
pub struct Tables {
    pub polys: Vec<Vec<[i32; 3]>>,
    pub conv: HashMap<(u8, u8, u8), Vec<usize>>,
}

pub fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let s: Value = serde_json::from_str(SHAPES_JSON).expect("surface_shapes.json");
        let mut polys = vec![Vec::new(); 58];
        for sh in s["shapes"].as_array().unwrap() {
            let id = sh["shape"].as_u64().unwrap() as usize;
            if id >= polys.len() {
                polys.resize(id + 1, Vec::new());
            }
            polys[id] = sh["vertices_halfvox"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    let a = v.as_array().unwrap();
                    [a[0].as_i64().unwrap() as i32, a[1].as_i64().unwrap() as i32, a[2].as_i64().unwrap() as i32]
                })
                .collect();
        }
        let c: Value = serde_json::from_str(CONV_JSON).expect("surface_conversion.json");
        let mut conv = HashMap::new();
        for (k, v) in c["conv"].as_object().unwrap() {
            let p: Vec<u8> = k.split(',').filter_map(|x| x.parse().ok()).collect();
            if p.len() == 3 {
                let types = v["types"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
                conv.insert((p[0], p[1], p[2]), types);
            }
        }
        Tables { polys, conv }
    })
}

/// Buoyancy polygons of one surface, half-voxel integer units in the vehicle frame.
pub fn surface_polys(s: &Surface, vp: [i32; 3], r: &[i32; 9]) -> Vec<Vec<[i32; 3]>> {
    let t = tables();
    let Some(types) = t.conv.get(&(s.orientation, s.rotation, s.shape)) else { return vec![] };
    let w = rotate(r, s.position);
    let centre = [2 * (vp[0] + w[0]), 2 * (vp[1] + w[1]), 2 * (vp[2] + w[2])];
    types
        .iter()
        .filter_map(|&ty| t.polys.get(ty))
        .filter(|p| p.len() >= 3)
        .map(|p| p.iter().map(|q| { let q = rotate(r, *q); [centre[0] + q[0], centre[1] + q[1], centre[2] + q[2]] }).collect())
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------------
// output

/// One visible convex polygon in vehicle space (metres, Y up).
#[derive(Debug, Clone)]
pub struct Face {
    pub comp: CompId,
    pub verts: Vec<[f32; 3]>,
    pub normal: [f32; 3],
    /// RGBA; alpha < 255 = glass
    pub colour: [u8; 4],
    /// false for generated block faces (outward normal, back face culled like the game); true for meshes
    pub two_sided: bool,
}

// ---------------------------------------------------------------------------------------------------------------------
// colours

pub const DEFAULT_PAINT: [u8; 3] = [222, 222, 218];

pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim();
    if s.is_empty() || s.len() > 6 || s.eq_ignore_ascii_case("x") {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Paint of one component: per-surface `sc` and the `bc`/`bc2`/`bc3`/`ac` slots.
#[derive(Debug, Clone, Default)]
pub struct Paint {
    pub surfaces: Vec<Option<[u8; 3]>>,
    pub bc: Option<[u8; 3]>,
    pub bc2: Option<[u8; 3]>,
    pub bc3: Option<[u8; 3]>,
    pub ac: Option<[u8; 3]>,
    /// paintable-sign pixels (9x9, `gc`), drawn on the part's local +y face
    pub pixels: Option<Vec<Option<[u8; 3]>>>,
}

impl Paint {
    pub fn from_info(c: &CompInfo) -> Self {
        let mut p = Paint {
            bc: c.attrs.get("bc").and_then(|s| parse_hex(s)),
            bc2: c.attrs.get("bc2").and_then(|s| parse_hex(s)),
            bc3: c.attrs.get("bc3").and_then(|s| parse_hex(s)),
            ac: c.attrs.get("ac").and_then(|s| parse_hex(s)),
            pixels: c.attrs.get("gc").map(|g| g.split(',').map(parse_hex).collect::<Vec<_>>()).filter(|v| v.len() >= SIGN_RES * SIGN_RES),
            ..Default::default()
        };
        if let Some(sc) = &c.sc {
            p.surfaces = sc.split(',').skip(1).map(parse_hex).collect();
        }
        p
    }
    pub fn surface(&self, i: usize) -> [u8; 3] {
        self.surfaces.get(i).copied().flatten().or(self.bc).unwrap_or(DEFAULT_PAINT)
    }
    fn key(&self, c: [u8; 4]) -> [u8; 4] {
        let slot = match (c[0], c[1], c[2]) {
            (255, 125, 0) => Some(self.bc.unwrap_or(DEFAULT_PAINT)),
            (154, 125, 0) => Some(self.bc2.or(self.bc).unwrap_or(DEFAULT_PAINT)),
            (55, 125, 0) => Some(self.bc3.or(self.bc).unwrap_or(DEFAULT_PAINT)),
            _ => None,
        };
        match slot {
            Some(s) => [s[0], s[1], s[2], c[3]],
            None => c,
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// generated block solids

const H: f64 = 0.125;
pub const SIGN_RES: usize = 9;

/// Split an axis-aligned ±y piece of a unit voxel into the 9x9 paintable-sign pixel quads. Index = row * 9 + col;
/// on the +y face columns run along local +x and rows along local +z (E1-inferred: hull lettering reads correctly in
/// the game's left-handed view).
/// The −y face shows the same picture as seen from its own side (columns mirrored).
fn sign_pixels(poly: &[[f64; 3]], top: bool) -> Vec<(usize, Vec<[f64; 3]>)> {
    let y = poly[0][1];
    let (cx, cz) = (poly.iter().map(|p| p[0]).sum::<f64>() / poly.len() as f64, poly.iter().map(|p| p[2]).sum::<f64>() / poly.len() as f64);
    let step = 2.0 * H / SIGN_RES as f64;
    let mut out = Vec::new();
    for row in 0..SIGN_RES {
        for col in 0..SIGN_RES {
            let c = if top { col } else { SIGN_RES - 1 - col };
            let (xa, za) = (cx - H + c as f64 * step, cz - H + row as f64 * step);
            out.push((row * SIGN_RES + col, vec![[xa, y, za], [xa, y, za + step], [xa + step, y, za + step], [xa + step, y, za]]));
        }
    }
    out
}

/// Convex point set of a generated block (metres, voxel-0-centred, before component rotation).
pub fn block_points(bt: i32) -> Option<Vec<[f64; 3]>> {
    let (family, lx, lz) = match bt {
        0 => (0, 1, 1),
        1 | 4 | 7 => (1, 1, [1, 2, 4][((bt - 1) / 3) as usize]),
        2 | 5 | 8 => (2, 1, [1, 2, 4][((bt - 2) / 3) as usize]),
        3 | 6 | 9 => (3, 1, [1, 2, 4][((bt - 3) / 3) as usize]),
        10 => (2, 2, 2),
        11 => (2, 2, 4),
        12 => (2, 4, 4),
        13 => (3, 2, 2),
        14 => (3, 2, 4),
        15 => (3, 4, 4),
        _ => return None,
    };
    let wide = lx > 1;
    let (x_lo, x_hi) = if wide { (H - 0.25 * lx as f64, H) } else { (-H, H) };
    let (z_lo, z_hi) = (H - 0.25 * lz as f64, H);
    let corners: Vec<[f64; 3]> =
        [x_lo, x_hi].iter().flat_map(|&x| [-H, H].into_iter().flat_map(move |y| [z_lo, z_hi].into_iter().map(move |z| [x, y, z]))).collect();
    // the corner the slopes fall away from is at +x for every pyramid: that is where the definitions put the
    // triangular side surfaces (the collision hulls in 0x14031ab90 are built mirrored in x)
    let (cx, fx) = match std::env::var("KEEL_DEBUG_CORNER").as_deref() {
        Ok("lo") => (x_lo, x_hi),
        Ok("narrow_lo") if !wide => (x_lo, x_hi),
        Ok("wide_lo") if wide => (x_lo, x_hi),
        _ => (x_hi, x_lo),
    };
    let pts = match family {
        0 => corners,
        1 if std::env::var_os("KEEL_DEBUG_LONGFLIP").is_some() && lz > 1 => corners.into_iter().filter(|p| !(p[1] > 0.0 && p[2] == z_hi)).collect(),
        1 => corners.into_iter().filter(|p| !(p[1] > 0.0 && p[2] == z_lo)).collect(),
        2 => vec![[cx, -H, z_hi], [fx, -H, z_hi], [cx, H, z_hi], [cx, -H, z_lo]],
        _ => corners.into_iter().filter(|p| !(p[0] == fx && p[1] > 0.0 && p[2] == z_lo)).collect(),
    };
    // debug: mirror 1-voxel blocks to test conventions (KEEL_DEBUG_MIRROR = "sx,sy,sz")
    if let (Some(m), true) = (std::env::var("KEEL_DEBUG_MIRROR").ok(), lz == 1 && lx == 1) {
        let f: Vec<f64> = m.split(',').filter_map(|x| x.parse().ok()).collect();
        return Some(pts.into_iter().map(|p| [p[0] * f[0], p[1] * f[1], p[2] * f[2]]).collect());
    }
    Some(pts)
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = dot(a, a).sqrt();
    (l > 1e-12).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

/// Faces of the convex hull of a small point set: (outward normal, polygon in CCW order seen from outside).
pub fn convex_hull_faces(pts: &[[f64; 3]]) -> Vec<([f64; 3], Vec<[f64; 3]>)> {
    let n = pts.len();
    let c = [0, 1, 2].map(|i| pts.iter().map(|p| p[i]).sum::<f64>() / n as f64);
    let mut planes: Vec<([f64; 3], f64)> = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let Some(mut nrm) = normalize(cross(sub(pts[j], pts[i]), sub(pts[k], pts[i]))) else { continue };
                let mut d = dot(nrm, pts[i]);
                if dot(nrm, c) > d {
                    nrm = [-nrm[0], -nrm[1], -nrm[2]];
                    d = -d;
                }
                if pts.iter().all(|p| dot(nrm, *p) <= d + 1e-9) && !planes.iter().any(|(m, e)| dot(*m, nrm) > 1.0 - 1e-9 && (e - d).abs() < 1e-9) {
                    planes.push((nrm, d));
                }
            }
        }
    }
    planes
        .into_iter()
        .map(|(nrm, d)| {
            let on: Vec<[f64; 3]> = pts.iter().copied().filter(|p| (dot(nrm, *p) - d).abs() < 1e-9).collect();
            let fc = [0, 1, 2].map(|i| on.iter().map(|p| p[i]).sum::<f64>() / on.len() as f64);
            let u = normalize(sub(on[0], fc)).unwrap_or([1.0, 0.0, 0.0]);
            let v = cross(nrm, u);
            let mut on = on;
            on.sort_by(|a, b| {
                let (pa, pb) = (sub(*a, fc), sub(*b, fc));
                dot(pa, v).atan2(dot(pa, u)).partial_cmp(&dot(pb, v).atan2(dot(pb, u))).unwrap()
            });
            (nrm, on)
        })
        .collect()
}

/// Sutherland–Hodgman clip of a convex polygon to an axis-aligned box.
pub fn clip_to_box(poly: &[[f64; 3]], lo: [f64; 3], hi: [f64; 3]) -> Vec<[f64; 3]> {
    let mut out = poly.to_vec();
    for axis in 0..3 {
        for (bound, keep_below) in [(lo[axis], false), (hi[axis], true)] {
            if out.is_empty() {
                return out;
            }
            let inside = |p: &[f64; 3]| if keep_below { p[axis] <= bound + 1e-9 } else { p[axis] >= bound - 1e-9 };
            let input = std::mem::take(&mut out);
            for i in 0..input.len() {
                let (a, b) = (input[i], input[(i + 1) % input.len()]);
                let (ia, ib) = (inside(&a), inside(&b));
                if ia {
                    out.push(a);
                }
                if ia != ib {
                    let t = (bound - a[axis]) / (b[axis] - a[axis]);
                    out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]);
                }
            }
        }
    }
    out
}

const DIRS: [[f64; 3]; 6] = [[1., 0., 0.], [-1., 0., 0.], [0., 1., 0.], [0., -1., 0.], [0., 0., 1.], [0., 0., -1.]];

/// Local pieces of a generated block: (polygon, outward normal, index of the definition surface that paints it).
#[derive(Clone)]
struct Piece {
    poly: Vec<[f64; 3]>,
    normal: [f64; 3],
    surface: Option<usize>,
}

/// Diagnostic: (pieces, pieces matched to a definition surface, definition surfaces left unused).
pub fn block_piece_stats(d: &Definition) -> (usize, usize, usize) {
    let p = block_pieces(d);
    let used: std::collections::HashSet<usize> = p.iter().filter_map(|x| x.surface).collect();
    let real = d.surfaces.iter().filter(|s| s.shape != 0).count();
    (p.len(), p.iter().filter(|x| x.surface.is_some()).count(), real - used.len().min(real))
}

fn block_pieces(d: &Definition) -> Vec<Piece> {
    let Some(pts) = block_points(d.block_type) else { return vec![] };
    let mut out = Vec::new();
    let mut cells = Vec::new();
    for (nrm, poly) in convex_hull_faces(&pts) {
        for v in &d.voxels {
            let c = [v[0] as f64 * 0.25, v[1] as f64 * 0.25, v[2] as f64 * 0.25];
            let piece = clip_to_box(&poly, [c[0] - H, c[1] - H, c[2] - H], [c[0] + H, c[1] + H, c[2] + H]);
            if piece.len() < 3 || area(&piece) < 1e-8 {
                continue;
            }
            out.push(Piece { poly: piece, normal: nrm, surface: None });
            cells.push(*v);
        }
    }
    // pass 1: axis-aligned pieces take the surface facing the same way in their voxel;
    // pass 2: sloped pieces take the best-facing surface not already used in that voxel
    let mut used = std::collections::HashSet::new();
    for pass in 0..2 {
        for (k, pc) in out.iter_mut().enumerate() {
            let axis = pc.normal.iter().any(|x| x.abs() > 0.999);
            if (pass == 0) != axis {
                continue;
            }
            let v = cells[k];
            pc.surface = d
                .surfaces
                .iter()
                .enumerate()
                .filter(|(i, s)| s.position == v && s.shape != 0 && (s.orientation as usize) < 6 && (pass == 0 || !used.contains(i)))
                .map(|(i, s)| (i, dot(pc.normal, DIRS[s.orientation as usize])))
                .filter(|(_, k)| if pass == 0 { *k > 0.999 } else { *k > 0.05 })
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|(i, _)| i);
            if let Some(i) = pc.surface {
                used.insert(i);
            }
        }
    }
    out
}

fn area(p: &[[f64; 3]]) -> f64 {
    let mut a = [0.0; 3];
    for i in 1..p.len() - 1 {
        let c = cross(sub(p[i], p[0]), sub(p[i + 1], p[0]));
        a = [a[0] + c[0], a[1] + c[1], a[2] + c[2]];
    }
    dot(a, a).sqrt() / 2.0
}

// ---------------------------------------------------------------------------------------------------------------------
// meshes

/// Loads `.mesh` files on demand from the game's `rom` folder (mesh names are `meshes/<file>.mesh`).
#[derive(Default)]
pub struct MeshLibrary {
    pub rom: Option<PathBuf>,
    cache: HashMap<String, Option<std::sync::Arc<Mesh>>>,
    pub missing: Vec<String>,
}

impl MeshLibrary {
    pub fn new(rom: Option<PathBuf>) -> Self {
        MeshLibrary { rom, ..Default::default() }
    }
    /// Guess the rom folder from a definitions folder (`rom/data/definitions`).
    pub fn rom_from_definitions(defs: &Path) -> Option<PathBuf> {
        let rom = defs.parent()?.parent()?.to_path_buf();
        rom.join("meshes").is_dir().then_some(rom)
    }
    pub fn get(&mut self, name: &str) -> Option<std::sync::Arc<Mesh>> {
        if name.is_empty() {
            return None;
        }
        if let Some(m) = self.cache.get(name) {
            return m.clone();
        }
        let m = self.rom.as_ref().and_then(|r| std::fs::read(r.join(name)).ok()).and_then(|b| keel_io::mesh::parse(&b).ok()).map(std::sync::Arc::new);
        if m.is_none() {
            self.missing.push(name.to_string());
        }
        self.cache.insert(name.to_string(), m.clone());
        m
    }
}

/// Moving parts and alternate meshes that should be shown with the base mesh.
fn extra_meshes(d: &Definition) -> Vec<&str> {
    [d.mesh_0_name.as_str(), d.mesh_1_name.as_str()]
        .into_iter()
        .filter(|n| !n.is_empty() && !n.contains("_hat_") && !n.contains("arrow_"))
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------------
// scene

fn mat(r: &[i32; 9]) -> [[f64; 3]; 3] {
    let mut m = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = r[3 * j + i] as f64;
        }
    }
    m
}

/// Diagnostic: for components whose rotation is not self-inverse, (block pieces, pieces culled against a neighbour).
pub fn pairing_stats(model: &Model, cat: &Catalogue) -> (usize, usize) {
    let faces = build_faces(model, cat);
    let asym: std::collections::HashSet<CompId> = model
        .components()
        .into_iter()
        .filter(|(_, c)| { let r = c.rotation; (std::env::var_os("KEEL_DEBUG_SLOPES").is_some() || std::env::var_os("KEEL_DEBUG_ALL").is_some() || (0..3).any(|i| (0..3).any(|j| r[3 * i + j] != r[3 * j + i]))) && r.iter().all(|x| x.abs() <= 1) && !c.attrs.contains_key("gc") && cat.get(&c.def).is_some_and(|d| d.mesh_data_name.is_empty() && !d.surfaces.is_empty() && (std::env::var_os("KEEL_DEBUG_SLOPES").is_none() || std::env::var("KEEL_DEBUG_SLOPES").unwrap().split(',').any(|b| b.parse::<i32>().ok() == Some(d.block_type)))) })
        .map(|(id, _)| id)
        .collect();
    let mut tmp = MeshLibrary::default();
    let _ = &mut tmp;
    let total: usize = model
        .components()
        .into_iter()
        .filter(|(id, _)| asym.contains(id))
        .map(|(_, c)| cat.get(&c.def).filter(|d| d.mesh_data_name.is_empty() && !d.surfaces.is_empty()).map(|d| block_pieces(d).len()).unwrap_or(0))
        .sum();
    let visible = faces.iter().filter(|f| asym.contains(&f.comp)).count();
    (total, total.saturating_sub(visible))
}
fn apply(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn shade_normal(m: &[[f64; 3]; 3], n: [f64; 3]) -> Option<[f32; 3]> {
    // inverse-transpose for axis scales: divide by squared column lengths
    let mut adj = [0.0; 3];
    for j in 0..3 {
        let l2 = m[0][j] * m[0][j] + m[1][j] * m[1][j] + m[2][j] * m[2][j];
        adj[j] = if l2 > 0.0 { n[j] / l2 } else { 0.0 };
    }
    normalize(apply(m, adj)).map(|v| v.map(|x| x as f32))
}

/// Build the full scene. `meshes` may have no rom folder, in which case mesh parts are drawn as their voxel boxes.
pub fn build_faces_with(model: &Model, cat: &Catalogue, meshes: &mut MeshLibrary) -> Vec<Face> {
    let mut pieces_cache: HashMap<String, Vec<Piece>> = HashMap::new();
    let mut blocks: Vec<(CompId, Vec<[f64; 3]>, [f32; 3], [u8; 4])> = Vec::new();
    let mut out: Vec<Face> = Vec::new();
    for (id, c) in model.components() {
        let m = mat(&c.rotation);
        let origin = [c.voxel[0] as f64 * 0.25, c.voxel[1] as f64 * 0.25, c.voxel[2] as f64 * 0.25];
        let paint = Paint::from_info(&c);
        let place = |p: [f64; 3]| {
            let w = apply(&m, p);
            [origin[0] + w[0], origin[1] + w[1], origin[2] + w[2]]
        };
        let def = cat.get(&c.def);
        let is_block = def.is_some_and(|d| d.mesh_data_name.is_empty() && !d.surfaces.is_empty() && block_points(d.block_type).is_some());
        if is_block || def.is_none() {
            let pieces = match def {
                Some(d) => pieces_cache.entry(d.id.clone()).or_insert_with(|| block_pieces(d)).clone(),
                None => block_pieces(&Catalogue::fallback().defs["01_block"]),
            };
            for pc in pieces {
                let Some(n) = shade_normal(&m, pc.normal) else { continue };
                if let Some(px) = &paint.pixels {
                    if pc.normal[1].abs() > 0.999 && pc.poly.len() == 4 {
                        for (i, quad) in sign_pixels(&pc.poly, pc.normal[1] > 0.0) {
                            let col = px[i].or(paint.bc).unwrap_or(DEFAULT_PAINT);
                            // tag pixels so culling never pairs them with a neighbour's face
                            blocks.push((id, quad.iter().map(|p| place(*p)).collect(), n, [col[0], col[1], col[2], 254]));
                        }
                        continue;
                    }
                }
                let col = pc.surface.map(|i| paint.surface(i)).unwrap_or(paint.bc.unwrap_or(DEFAULT_PAINT));
                blocks.push((id, pc.poly.iter().map(|p| place(*p)).collect(), n, [col[0], col[1], col[2], 255]));
            }
            continue;
        }
        let d = def.unwrap();
        let mut names = vec![d.mesh_data_name.as_str()];
        names.extend(extra_meshes(d));
        let mut drew = false;
        for name in names {
            let Some(mesh) = meshes.get(name) else { continue };
            drew = true;
            for sm in &mesh.submeshes {
                let glass = sm.shader & 0x10000 != 0;
                let (a, b) = (sm.index_start as usize, (sm.index_start + sm.index_count) as usize);
                for t in mesh.indices.get(a..b.min(mesh.indices.len())).unwrap_or(&[]).chunks_exact(3) {
                    let vs: Vec<[f64; 3]> = t.iter().map(|&i| { let p = mesh.positions[i as usize]; place([p[0] as f64, p[1] as f64, p[2] as f64]) }).collect();
                    let geo = cross(sub(vs[1], vs[0]), sub(vs[2], vs[0]));
                    let Some(mut gn) = normalize(geo) else { continue };
                    // meshes carry explicit back-face copies (often coloured for the inside): draw each triangle
                    // one-sided, facing along its stored vertex normals
                    let vn = t.iter().fold([0.0f64; 3], |a, &i| { let n = mesh.normals[i as usize]; [a[0] + n[0] as f64, a[1] + n[1] as f64, a[2] + n[2] as f64] });
                    let one_sided = match shade_normal(&m, vn) {
                        Some(wn) => {
                            if dot(gn, wn.map(|x| x as f64)) < 0.0 {
                                gn = [-gn[0], -gn[1], -gn[2]];
                            }
                            true
                        }
                        None => false,
                    };
                    let cs: Vec<[u8; 4]> = t.iter().map(|&i| paint.key(mesh.colours[i as usize])).collect();
                    let avg = [0, 1, 2, 3].map(|k| (cs.iter().map(|c| c[k] as u32).sum::<u32>() / 3) as u8);
                    let alpha = if glass { avg[3].min(140) } else { 255 };
                    out.push(Face {
                        comp: id,
                        verts: vs.iter().map(|v| v.map(|x| x as f32)).collect(),
                        normal: gn.map(|x| x as f32),
                        colour: [avg[0], avg[1], avg[2], alpha],
                        two_sided: !one_sided || glass,
                    });
                }
            }
        }
        // block surfaces declared by mesh parts (propeller housings, enclosed pipes, …): the game draws these as
        // generated faces next to the mesh. Shapes covered by the game's conversion table (full, triangle, slopes).
        let mut surf_drawn = false;
        // shape 3 is a connection face (pipe/port opening). On enclosed blocks (parts that also declare full faces)
        // it is drawn as a full face; on bare pipes the mesh supplies the end.
        let block_like = d.surfaces.iter().any(|s| s.shape == 1);
        for (i, sf) in d.surfaces.iter().enumerate() {
            if sf.shape == 0 || (sf.shape == 3 && !block_like) {
                continue;
            }
            let sf = if sf.shape == 3 { Surface { shape: 1, rotation: 0, ..sf.clone() } } else { sf.clone() };
            for poly in surface_polys(&sf, c.voxel, &c.rotation) {
                let w: Vec<[f64; 3]> = poly.iter().map(|v| [v[0] as f64 * 0.125, v[1] as f64 * 0.125, v[2] as f64 * 0.125]).collect();
                let Some(n) = normalize(cross(sub(w[1], w[0]), sub(w[2], w[0]))) else { continue };
                // orient outward: surface normal should point away from the surface's voxel centre
                let vc = { let q = rotate(&c.rotation, sf.position); [0, 1, 2].map(|k| (c.voxel[k] + q[k]) as f64 * 0.25) };
                let fc = [0, 1, 2].map(|k| w.iter().map(|p| p[k]).sum::<f64>() / w.len() as f64);
                let n = if dot(n, sub(fc, vc)) < 0.0 { [-n[0], -n[1], -n[2]] } else { n };
                let col = paint.surface(i);
                blocks.push((id, w, n.map(|x| x as f32), [col[0], col[1], col[2], 255]));
                surf_drawn = true;
            }
        }
        if !drew && !surf_drawn {
            // no mesh available: draw the part's voxels as boxes so it is visible and pickable
            for v in &d.voxels {
                for (nrm, poly) in convex_hull_faces(&block_points(0).unwrap()) {
                    let off = [v[0] as f64 * 0.25, v[1] as f64 * 0.25, v[2] as f64 * 0.25];
                    let Some(n) = shade_normal(&m, nrm) else { continue };
                    let col = paint.bc.unwrap_or([150, 150, 160]);
                    blocks.push((id, poly.iter().map(|p| place([p[0] + off[0], p[1] + off[1], p[2] + off[2]])).collect(), n, [col[0], col[1], col[2], 255]));
                }
            }
        }
    }
    // cull coincident block pieces (internal walls); keys are vertex sets quantised to 1/256 m
    let key = |b: usize, p: &[[f64; 3]]| {
        let mut k: Vec<[i64; 3]> = p.iter().map(|v| v.map(|x| (x * 256.0).round() as i64)).collect();
        k.sort();
        k.dedup();
        (b, k)
    };
    let mut count: HashMap<(usize, Vec<[i64; 3]>), u32> = HashMap::new();
    for (id, p, _, _) in &blocks {
        *count.entry(key(id.body, p)).or_default() += 1;
    }
    for (id, p, n, col) in blocks {
        let sign_pixel = col[3] == 254;
        if (sign_pixel || count[&key(id.body, &p)] == 1) && area(&p) > 1e-9 {
            let col = [col[0], col[1], col[2], 255];
            out.push(Face { comp: id, verts: p.iter().map(|v| v.map(|x| x as f32)).collect(), normal: n, colour: col, two_sided: false });
        }
    }
    out
}

/// Scene without mesh files (blocks exact, other parts as boxes).
pub fn build_faces(model: &Model, cat: &Catalogue) -> Vec<Face> {
    build_faces_with(model, cat, &mut MeshLibrary::default())
}

/// Solid voxel cells occupied by a component (vehicle frame, voxel units).
pub fn occupied_voxels(def: Option<&crate::Definition>, vp: [i32; 3], r: &[i32; 9]) -> Vec<[i32; 3]> {
    let vox = def.map(|d| d.solid_voxels.clone()).unwrap_or_else(|| vec![[0, 0, 0]]);
    vox.into_iter().map(|v| { let w = rotate(r, v); [vp[0] + w[0], vp[1] + w[1], vp[2] + w[2]] }).collect()
}
