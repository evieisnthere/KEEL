//! Component catalogue loaded from the user's own `rom/data/definitions` folder (never bundled).
use keel_io::xml::{self, Element};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Surface {
    pub orientation: u8,
    pub rotation: u8,
    pub shape: u8,
    pub position: [i32; 3],
}

#[derive(Debug, Clone)]
pub struct Definition {
    /// File stem, e.g. `01_block`: the `d` attribute in vehicle files.
    pub id: String,
    pub name: String,
    pub category: i32,
    pub def_type: i32,
    pub mass: f64,
    pub tags: String,
    /// 0 cube … 15 inverse pyramid 4x4 for generated blocks (definition type 0); other parts use meshes
    pub block_type: i32,
    pub mesh_data_name: String,
    pub mesh_0_name: String,
    pub mesh_1_name: String,
    pub voxels: Vec<[i32; 3]>,
    /// voxels with flags bit 0 set: they block placement (others are non-solid extents)
    pub solid_voxels: Vec<[i32; 3]>,
    pub surfaces: Vec<Surface>,
    pub buoyancy_surfaces: Vec<Surface>,
    /// logic / electric / fluid / torque ports (RE-0008 §1)
    pub logic_nodes: Vec<LogicNode>,
}

/// A port on a component: `mode` 0 = output, 1 = input; `kind` 0 bool, 1 number, 2 torque, 3 fluid, 4 electric,
/// 5 composite, 6 video, 7 audio; `position` is the voxel within the component (before rotation).
#[derive(Debug, Clone, PartialEq)]
pub struct LogicNode {
    pub label: String,
    pub mode: i32,
    pub kind: i32,
    pub orientation: i32,
    pub position: [i32; 3],
}

fn v3(e: Option<&Element>) -> [i32; 3] {
    let mut v = [0; 3];
    if let Some(e) = e {
        for (i, a) in ["x", "y", "z"].iter().enumerate() {
            v[i] = e.get(a).and_then(|s| s.trim().parse::<f64>().ok()).map(|f| f.round() as i32).unwrap_or(0);
        }
    }
    v
}

fn num<T: std::str::FromStr + Default>(e: &Element, a: &str) -> T {
    e.get(a).and_then(|s| s.trim().parse::<T>().ok()).unwrap_or_default()
}

fn surfaces(root: &Element, tag: &str) -> Vec<Surface> {
    root.find(tag)
        .map(|s| {
            s.elements()
                .filter(|e| e.tag == "surface")
                .map(|e| Surface {
                    orientation: num(e, "orientation"),
                    rotation: num(e, "rotation"),
                    shape: num(e, "shape"),
                    position: v3(e.find("position")),
                })
                .collect()
        })
        .unwrap_or_default()
}

impl Definition {
    /// First port with this label (case-insensitive prefix match) and kind.
    pub fn node(&self, label: &str, kind: i32) -> Option<&LogicNode> {
        let l = label.to_lowercase();
        self.logic_nodes.iter().find(|n| n.kind == kind && n.label.to_lowercase().starts_with(&l))
    }
    pub fn from_xml(id: &str, src: &str) -> Result<Self, String> {
        let doc = xml::parse(src).map_err(|e| e.to_string())?;
        let r = &doc.root;
        if r.tag != "definition" {
            return Err(format!("root <{}> is not <definition>", r.tag));
        }
        let mut voxels: Vec<[i32; 3]> =
            r.find("voxels").map(|v| v.elements().filter(|e| e.tag == "voxel").map(|e| v3(e.find("position"))).collect()).unwrap_or_default();
        let mut solid_voxels: Vec<[i32; 3]> = r
            .find("voxels")
            .map(|v| v.elements().filter(|e| e.tag == "voxel" && num::<i32>(e, "flags") & 1 == 1).map(|e| v3(e.find("position"))).collect())
            .unwrap_or_default();
        if voxels.is_empty() {
            voxels.push([0, 0, 0]);
        }
        if solid_voxels.is_empty() {
            solid_voxels.push([0, 0, 0]);
        }
        Ok(Definition {
            id: id.to_string(),
            name: r.get("name").unwrap_or_else(|| id.to_string()),
            category: num(r, "category"),
            def_type: num(r, "type"),
            mass: num(r, "mass"),
            tags: r.get("tags").unwrap_or_default(),
            block_type: num(r, "block_type"),
            mesh_data_name: r.get("mesh_data_name").unwrap_or_default(),
            mesh_0_name: r.get("mesh_0_name").unwrap_or_default(),
            mesh_1_name: r.get("mesh_1_name").unwrap_or_default(),
            voxels,
            solid_voxels,
            surfaces: surfaces(r, "surfaces"),
            buoyancy_surfaces: surfaces(r, "buoyancy_surfaces"),
            logic_nodes: r
                .find("logic_nodes")
                .map(|n| {
                    n.elements()
                        .filter(|e| e.tag == "logic_node")
                        .map(|e| LogicNode { label: e.get("label").unwrap_or_default(), mode: num(e, "mode"), kind: num(e, "type"), orientation: num(e, "orientation"), position: v3(e.find("position")) })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct Catalogue {
    pub defs: BTreeMap<String, Definition>,
    pub errors: Vec<(String, String)>,
}

impl Catalogue {
    pub fn load_dir(dir: &Path) -> std::io::Result<Self> {
        let mut c = Catalogue::default();
        for entry in std::fs::read_dir(dir)? {
            let p = entry?.path();
            if p.extension().and_then(|e| e.to_str()) != Some("xml") {
                continue;
            }
            let id = p.file_stem().unwrap().to_string_lossy().to_string();
            match std::fs::read(&p).map(|b| String::from_utf8_lossy(&b).into_owned()) {
                Ok(src) => match Definition::from_xml(&id, &src) {
                    Ok(d) => {
                        c.defs.insert(id, d);
                    }
                    Err(e) => c.errors.push((id, e)),
                },
                Err(e) => c.errors.push((id, e.to_string())),
            }
        }
        Ok(c)
    }
    pub fn get(&self, id: &str) -> Option<&Definition> {
        self.defs.get(id)
    }
    /// Minimal built-in catalogue used when no definitions folder is configured: a unit block.
    pub fn fallback() -> Self {
        let mut c = Catalogue::default();
        let faces = (0..6u8).map(|o| Surface { orientation: o, rotation: 0, shape: 1, position: [0, 0, 0] }).collect::<Vec<_>>();
        c.defs.insert(
            "01_block".into(),
            Definition {
                id: "01_block".into(),
                name: "Block".into(),
                category: 0,
                def_type: 0,
                mass: 1.0,
                tags: "basic".into(),
                block_type: 0,
                mesh_data_name: String::new(),
                mesh_0_name: String::new(),
                mesh_1_name: String::new(),
                voxels: vec![[0, 0, 0]],
                solid_voxels: vec![[0, 0, 0]],
                surfaces: faces.clone(),
                buoyancy_surfaces: faces,
                logic_nodes: vec![],
            },
        );
        c
    }
}
