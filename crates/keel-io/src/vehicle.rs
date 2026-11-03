//! Vehicle view over the lossless XML tree (data_version 3). See docs/reverse-engineering/0002.

use crate::xml::{Document, Element};

pub const DEFAULT_DEFINITION: &str = "01_block";
pub const IDENTITY: [i32; 9] = [1, 0, 0, 0, 1, 0, 0, 0, 1];
pub const GRID_M: f64 = 0.25;

fn vec3(el: Option<&Element>) -> [i32; 3] {
    let mut v = [0; 3];
    if let Some(e) = el {
        for (i, a) in ["x", "y", "z"].iter().enumerate() {
            v[i] = e.get(a).and_then(|s| s.parse::<f64>().ok()).map(|f| f as i32).unwrap_or(0);
        }
    }
    v
}

/// Column-major component rotation: world = rᵀ · v (E1-inferred).
pub fn rotate(r: &[i32; 9], v: [i32; 3]) -> [i32; 3] {
    let mut o = [0; 3];
    for (i, out) in o.iter_mut().enumerate() {
        *out = r[i] * v[0] + r[3 + i] * v[1] + r[6 + i] * v[2];
    }
    o
}

#[derive(Debug, Clone)]
pub struct ComponentRef<'a> {
    pub c: &'a Element,
    pub o: &'a Element,
}

impl<'a> ComponentRef<'a> {
    pub fn definition(&self) -> String {
        self.c.get("d").unwrap_or_else(|| DEFAULT_DEFINITION.to_string())
    }
    pub fn voxel(&self) -> [i32; 3] {
        vec3(self.o.find("vp"))
    }
    pub fn rotation(&self) -> [i32; 9] {
        match self.o.get("r") {
            Some(s) => {
                let v: Vec<i32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if v.len() == 9 {
                    let mut r = [0; 9];
                    r.copy_from_slice(&v);
                    r
                } else {
                    IDENTITY
                }
            }
            None => IDENTITY,
        }
    }
}

pub struct Vehicle {
    pub doc: Document,
}

impl Vehicle {
    pub fn from_document(doc: Document) -> Result<Self, String> {
        if doc.root.tag != "vehicle" {
            return Err(format!("not a vehicle file (root <{}>)", doc.root.tag));
        }
        Ok(Vehicle { doc })
    }
    pub fn data_version(&self) -> Option<String> {
        self.doc.root.get("data_version")
    }
    pub fn bodies(&self) -> Vec<&Element> {
        self.doc.root.find("bodies").map(|b| b.elements().filter(|e| e.tag == "body").collect()).unwrap_or_default()
    }
    pub fn components(&self) -> Vec<ComponentRef<'_>> {
        let mut out = Vec::new();
        for b in self.bodies() {
            if let Some(cs) = b.find("components") {
                for c in cs.elements().filter(|e| e.tag == "c") {
                    if let Some(o) = c.find("o") {
                        out.push(ComponentRef { c, o });
                    }
                }
            }
        }
        out
    }
}
