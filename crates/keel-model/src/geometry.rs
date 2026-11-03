//! Block geometry from the game's own surface tables (RE-0004): each definition surface converts to one or more of
//! the 58 internal surface polygons. Polygons that coincide back-to-back between neighbouring blocks are culled, which
//! leaves the visible hull skin.
use crate::catalogue::{Catalogue, Surface};
use crate::edit::{CompId, Model};
use keel_io::vehicle::rotate;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

const SHAPES_JSON: &str = include_str!("../../../research/extracted-metadata/surface_shapes.json");
const CONV_JSON: &str = include_str!("../../../research/extracted-metadata/surface_conversion.json");

pub struct Tables {
    /// polygon vertices per surface type, half-voxel units relative to the voxel centre
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

/// One visible polygon in world space (metres, Y up), with its owning component.
#[derive(Debug, Clone)]
pub struct Face {
    pub comp: CompId,
    pub verts: Vec<[f32; 3]>,
    pub normal: [f32; 3],
    pub colour: [u8; 3],
}

/// Polygons of one surface, in half-voxel integer units in the vehicle frame.
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

fn cube_surfaces(voxels: &[[i32; 3]]) -> Vec<Surface> {
    voxels.iter().flat_map(|&p| (0..6u8).map(move |o| Surface { orientation: o, rotation: 0, shape: 1, position: p })).collect()
}

fn parse_colour(sc: Option<String>) -> Option<[u8; 3]> {
    let sc = sc?;
    let parts: Vec<&str> = sc.split(',').collect();
    parts.iter().skip(1).find(|p| p.len() == 6).and_then(|h| {
        let v = u32::from_str_radix(h, 16).ok()?;
        Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
    })
}

fn default_colour(def: &str) -> [u8; 3] {
    if def.starts_with("01_block") || def.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return [205, 205, 200];
    }
    let mut h: u32 = 2166136261;
    for b in def.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    [90 + (h & 0x7f) as u8, 90 + ((h >> 8) & 0x7f) as u8, 90 + ((h >> 16) & 0x7f) as u8]
}

/// Build the visible skin of every body. Coincident back-to-back polygons are removed.
pub fn build_faces(model: &Model, cat: &Catalogue) -> Vec<Face> {
    let mut raw: Vec<(CompId, Vec<[i32; 3]>, [u8; 3])> = Vec::new();
    for (id, c) in model.components() {
        let r = c.rotation;
        let colour = parse_colour(c.sc.clone()).unwrap_or_else(|| default_colour(&c.def));
        let surfs = match cat.get(&c.def) {
            Some(d) if !d.surfaces.is_empty() => d.surfaces.clone(),
            Some(d) => cube_surfaces(&d.voxels),
            None => cube_surfaces(&[[0, 0, 0]]),
        };
        for s in &surfs {
            for p in surface_polys(s, c.voxel, &r) {
                raw.push((id, p, colour));
            }
        }
    }
    // cull coincident pairs per body
    let mut count: HashMap<(usize, Vec<[i32; 3]>), u32> = HashMap::new();
    let key = |b: usize, p: &Vec<[i32; 3]>| {
        let mut k = p.clone();
        k.sort();
        (b, k)
    };
    for (id, p, _) in &raw {
        *count.entry(key(id.body, p)).or_default() += 1;
    }
    raw.into_iter()
        .filter(|(id, p, _)| count[&key(id.body, p)] == 1)
        .filter_map(|(comp, p, colour)| {
            let verts: Vec<[f32; 3]> = p.iter().map(|v| [v[0] as f32 * 0.125, v[1] as f32 * 0.125, v[2] as f32 * 0.125]).collect();
            let normal = newell(&verts)?; // zero-size (scaled-to-nothing) polygons are skipped
            Some(Face { comp, verts, normal, colour })
        })
        .collect()
}

fn newell(v: &[[f32; 3]]) -> Option<[f32; 3]> {
    let mut n = [0f32; 3];
    for i in 0..v.len() {
        let a = v[i];
        let b = v[(i + 1) % v.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    (l > 1e-9).then(|| [n[0] / l, n[1] / l, n[2] / l])
}

/// Solid voxel cells occupied by a component (vehicle frame, voxel units).
pub fn occupied_voxels(def: Option<&crate::Definition>, vp: [i32; 3], r: &[i32; 9]) -> Vec<[i32; 3]> {
    let vox = def.map(|d| d.solid_voxels.clone()).unwrap_or_else(|| vec![[0, 0, 0]]);
    vox.into_iter().map(|v| { let w = rotate(r, v); [vp[0] + w[0], vp[1] + w[1], vp[2] + w[2]] }).collect()
}
