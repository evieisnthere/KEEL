//! Editable vehicle on top of the lossless XML tree. Every edit touches only the elements it changes, so unknown data
//! survives (plan exit criterion). Undo/redo snapshots the document (vehicles are at most a few MB).
use keel_io::vehicle::{DEFAULT_ROTATION, IDENTITY};
use keel_io::xml::{self, Document, Element, Node};

/// World voxel of a port at `pos` (component-local) on a component at `vp` with rotation `r` (column-major,
/// M[i][j] = r[3j + i], as everywhere in KEEL).
pub fn port_voxel(vp: [i32; 3], r: &[i32; 9], pos: [i32; 3]) -> [i32; 3] {
    [0, 1, 2].map(|i| vp[i] + r[i] * pos[0] + r[3 + i] * pos[1] + r[6 + i] * pos[2])
}

/// A generated part: definition, voxel, rotation and optional paint (`bc` block colour, `ac` additive / light colour).
#[derive(Debug, Clone, PartialEq)]
pub struct GenPart {
    pub def: String,
    pub vp: [i32; 3],
    pub r: [i32; 9],
    pub bc: Option<String>,
    pub ac: Option<String>,
}

/// A generated wire (`logic_node_link`) between two port voxels. `kind` as in [`crate::catalogue::LogicNode`].
#[derive(Debug, Clone, PartialEq)]
pub struct GenLink {
    pub kind: i32,
    pub a: [i32; 3],
    pub b: [i32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CompId {
    pub body: usize,
    pub comp: usize,
}

#[derive(Debug, Clone)]
pub struct CompInfo {
    pub def: String,
    pub voxel: [i32; 3],
    pub rotation: [i32; 9],
    pub sc: Option<String>,
    /// paint and other small attributes of `<o>` (bc, bc2, bc3, ac, …)
    pub attrs: std::collections::HashMap<String, String>,
}

pub struct Model {
    pub doc: Document,
    undo: Vec<Document>,
    redo: Vec<Document>,
    pub generation: u64,
    pub dirty: bool,
}

const UNDO_LIMIT: usize = 200;

fn v3(e: Option<&Element>) -> [i32; 3] {
    let mut v = [0; 3];
    if let Some(e) = e {
        for (i, a) in ["x", "y", "z"].iter().enumerate() {
            v[i] = e.get(a).and_then(|s| s.trim().parse::<f64>().ok()).map(|f| f.round() as i32).unwrap_or(0);
        }
    }
    v
}

fn rot_of(o: &Element) -> [i32; 9] {
    o.get("r")
        .and_then(|s| {
            let v: Vec<i32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() == 9).then(|| {
                let mut r = [0; 9];
                r.copy_from_slice(&v);
                r
            })
        })
        .unwrap_or(DEFAULT_ROTATION)
}

/// M[i][j] = r[3j + i]; world = M·v (keel_io::vehicle::rotate).
fn mat(r: &[i32; 9]) -> [[i32; 3]; 3] {
    let mut m = [[0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = r[3 * j + i];
        }
    }
    m
}
fn unmat(m: &[[i32; 3]; 3]) -> [i32; 9] {
    let mut r = [0; 9];
    for i in 0..3 {
        for j in 0..3 {
            r[3 * j + i] = m[i][j];
        }
    }
    r
}
fn mul(a: &[[i32; 3]; 3], b: &[[i32; 3]; 3]) -> [[i32; 3]; 3] {
    let mut m = [[0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}
/// +90° about axis 0 = X, 1 = Y, 2 = Z (right-handed).
pub fn quarter_turn(axis: usize) -> [[i32; 3]; 3] {
    match axis {
        0 => [[1, 0, 0], [0, 0, -1], [0, 1, 0]],
        1 => [[0, 0, 1], [0, 1, 0], [-1, 0, 0]],
        _ => [[0, -1, 0], [1, 0, 0], [0, 0, 1]],
    }
}

fn set_vec(o: &mut Element, tag: &str, v: [i32; 3]) {
    if o.find(tag).is_none() {
        if v == [0, 0, 0] {
            return;
        }
        o.push(Element::new(tag));
    }
    let e = o.find_mut(tag).unwrap();
    for (i, a) in ["x", "y", "z"].iter().enumerate() {
        if v[i] == 0 {
            e.remove_attr(a);
        } else {
            e.set(a, &v[i].to_string());
        }
    }
}

impl Model {
    pub fn from_document(doc: Document) -> Result<Self, String> {
        if doc.root.tag != "vehicle" {
            return Err(format!("not a vehicle file (root <{}>)", doc.root.tag));
        }
        Ok(Model { doc, undo: vec![], redo: vec![], generation: 0, dirty: false })
    }
    pub fn parse(src: &str) -> Result<Self, String> {
        Self::from_document(xml::parse(src).map_err(|e| e.to_string())?)
    }
    pub fn new_empty() -> Self {
        Self::parse("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<vehicle data_version=\"3\" bodies_id=\"1\">\n<authors/>\n<bodies>\n<body unique_id=\"0\">\n<components>\n</components>\n</body>\n</bodies>\n<logic_node_links/>\n</vehicle>\n").unwrap()
    }
    pub fn to_xml(&self) -> String {
        self.doc.to_string_lossless()
    }

    fn bodies(&self) -> Vec<&Element> {
        self.doc.root.find("bodies").map(|b| b.elements().filter(|e| e.tag == "body").collect()).unwrap_or_default()
    }
    pub fn body_count(&self) -> usize {
        self.bodies().len()
    }
    pub fn components(&self) -> Vec<(CompId, CompInfo)> {
        let mut out = Vec::new();
        for (bi, b) in self.bodies().iter().enumerate() {
            if let Some(cs) = b.find("components") {
                for (ci, c) in cs.elements().filter(|e| e.tag == "c").enumerate() {
                    let Some(o) = c.find("o") else { continue };
                    out.push((
                        CompId { body: bi, comp: ci },
                        CompInfo {
                            def: c.get("d").unwrap_or_else(|| "01_block".into()),
                            voxel: v3(o.find("vp")),
                            rotation: rot_of(o),
                            sc: o.get("sc"),
                            attrs: ["bc", "bc2", "bc3", "ac", "gc"].iter().filter_map(|k| o.get(k).map(|v| (k.to_string(), v))).collect(),
                        },
                    ));
                }
            }
        }
        out
    }
    pub fn component_element(&self, id: CompId) -> Option<&Element> {
        self.bodies().get(id.body)?.find("components")?.elements().filter(|e| e.tag == "c").nth(id.comp)
    }
    fn components_el_mut(&mut self, body: usize) -> Option<&mut Element> {
        self.doc.root.find_mut("bodies")?.elements_mut().filter(|e| e.tag == "body").nth(body)?.find_mut("components")
    }
    fn comp_mut(&mut self, id: CompId) -> Option<&mut Element> {
        self.components_el_mut(id.body)?.elements_mut().filter(|e| e.tag == "c").nth(id.comp)
    }

    // ---- history -------------------------------------------------------------------------------------------------
    fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.generation += 1;
        self.dirty = true;
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(d) => {
                self.redo.push(std::mem::replace(&mut self.doc, d));
                self.generation += 1;
                self.dirty = true;
                true
            }
            None => false,
        }
    }
    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(d) => {
                self.undo.push(std::mem::replace(&mut self.doc, d));
                self.generation += 1;
                self.dirty = true;
                true
            }
            None => false,
        }
    }

    // ---- edits ---------------------------------------------------------------------------------------------------
    pub fn translate(&mut self, ids: &[CompId], d: [i32; 3]) {
        if ids.is_empty() || d == [0, 0, 0] {
            return;
        }
        self.checkpoint();
        for &id in ids {
            if let Some(o) = self.comp_mut(id).and_then(|c| c.find_mut("o")) {
                let v = v3(o.find("vp"));
                set_vec(o, "vp", [v[0] + d[0], v[1] + d[1], v[2] + d[2]]);
            }
        }
    }

    /// Rotate a selection by +90° about `axis` around the integer voxel `pivot`.
    pub fn rotate_quarter(&mut self, ids: &[CompId], axis: usize, pivot: [i32; 3]) {
        if ids.is_empty() {
            return;
        }
        self.checkpoint();
        let q = quarter_turn(axis);
        for &id in ids {
            if let Some(o) = self.comp_mut(id).and_then(|c| c.find_mut("o")) {
                let r = unmat(&mul(&q, &mat(&rot_of(o))));
                if r == DEFAULT_ROTATION {
                    o.remove_attr("r");
                } else {
                    o.set("r", &r.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
                }
                let v = v3(o.find("vp"));
                let rel = [v[0] - pivot[0], v[1] - pivot[1], v[2] - pivot[2]];
                let n: Vec<i32> = (0..3).map(|i| (0..3).map(|k| q[i][k] * rel[k]).sum::<i32>() + pivot[i]).collect();
                set_vec(o, "vp", [n[0], n[1], n[2]]);
            }
        }
    }

    /// Per-local-axis scale currently baked into a component's transform (XML scale edit); 1 = unscaled.
    pub fn scale_of(r: &[i32; 9]) -> [i32; 3] {
        let mut out = [1; 3];
        for (j, o) in out.iter_mut().enumerate() {
            let col = &r[3 * j..3 * j + 3];
            *o = col.iter().map(|x| x.abs()).max().unwrap_or(0);
        }
        out
    }

    /// Apply a user scale (integer factor per local axis, 0 allowed) the same way workshop XML edits do: by scaling
    /// the columns of `r`. Replaces any existing scale. **Sealing is only guaranteed at the block's origin voxel**:
    /// the buoyancy shell is built from the definition's surfaces, which do not move with the scale (UI must warn).
    pub fn set_scale(&mut self, ids: &[CompId], scale: [i32; 3]) {
        if ids.is_empty() {
            return;
        }
        self.checkpoint();
        for &id in ids {
            if let Some(o) = self.comp_mut(id).and_then(|c| c.find_mut("o")) {
                let mut r = rot_of(o);
                for j in 0..3 {
                    let col = [r[3 * j], r[3 * j + 1], r[3 * j + 2]];
                    let m = col.iter().map(|x| x.abs()).max().unwrap_or(0);
                    // recover the unit direction; a zero column (scaled to nothing) falls back to the identity axis
                    let unit: [i32; 3] = if m == 0 { [IDENTITY[3 * j], IDENTITY[3 * j + 1], IDENTITY[3 * j + 2]] } else { col.map(|x| x / m) };
                    for i in 0..3 {
                        r[3 * j + i] = unit[i] * scale[j];
                    }
                }
                if r == DEFAULT_ROTATION {
                    o.remove_attr("r");
                } else {
                    o.set("r", &r.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
                }
            }
        }
    }

    /// Remove components. Returns how many were removed.
    pub fn delete(&mut self, ids: &[CompId]) -> usize {
        if ids.is_empty() {
            return 0;
        }
        self.checkpoint();
        let mut ids = ids.to_vec();
        ids.sort();
        ids.dedup();
        let mut removed = 0;
        for id in ids.into_iter().rev() {
            let Some(cs) = self.components_el_mut(id.body) else { continue };
            let mut seen = 0;
            let mut at = None;
            for (k, n) in cs.children.iter().enumerate() {
                if let Node::Element(e) = n {
                    if e.tag == "c" {
                        if seen == id.comp {
                            at = Some(k);
                            break;
                        }
                        seen += 1;
                    }
                }
            }
            if let Some(k) = at {
                cs.children.remove(k);
                if k > 0 && matches!(&cs.children[k - 1], Node::Raw(s) if s.trim().is_empty()) {
                    cs.children.remove(k - 1);
                }
                removed += 1;
            }
        }
        removed
    }

    /// Duplicate components with an offset; returns the new ids (appended to their bodies).
    pub fn duplicate(&mut self, ids: &[CompId], offset: [i32; 3]) -> Vec<CompId> {
        if ids.is_empty() {
            return vec![];
        }
        self.checkpoint();
        let clones: Vec<(usize, Element)> = ids.iter().filter_map(|&id| self.comp_mut(id).map(|c| (id.body, c.clone()))).collect();
        let mut out = Vec::new();
        for (body, mut c) in clones {
            if let Some(o) = c.find_mut("o") {
                let v = v3(o.find("vp"));
                set_vec(o, "vp", [v[0] + offset[0], v[1] + offset[1], v[2] + offset[2]]);
            }
            if let Some(cs) = self.components_el_mut(body) {
                let n = cs.elements().filter(|e| e.tag == "c").count();
                cs.push(c);
                out.push(CompId { body, comp: n });
            }
        }
        out
    }

    /// Replace generated content in one undo step: remove components matching `remove` (definition, voxel,
    /// rotation) from `body`, then add `add` (definition, voxel, rotation, base colour).
    pub fn replace_generated(&mut self, body: usize, remove: &[(String, [i32; 3], [i32; 9])], add: &[(String, [i32; 3], [i32; 9], String)]) -> (usize, usize) {
        if self.components_el_mut(body).is_none() {
            return (0, 0);
        }
        self.checkpoint();
        let want: std::collections::HashSet<(String, [i32; 3], [i32; 9])> = remove.iter().cloned().collect();
        let ids: Vec<CompId> = self
            .components()
            .into_iter()
            .filter(|(id, c)| id.body == body && want.contains(&(c.def.clone(), c.voxel, c.rotation)))
            .map(|(id, _)| id)
            .collect();
        let removed = ids.len();
        let undo_len = self.undo.len();
        self.delete(&ids);
        self.undo.truncate(undo_len); // keep a single checkpoint for the whole operation
        let mut added = 0;
        for (def, vp, r, bc) in add {
            if self.add_inner(body, def, *vp, *r, Some(bc)).is_some() {
                added += 1;
            }
        }
        (removed, added)
    }

    /// Apply a generated build as one undo step: remove the components listed in `remove` (def, voxel, rotation), add
    /// `parts` and the wires `links`. Returns (removed, added parts, added links).
    pub fn apply_build(&mut self, body: usize, remove: &[(String, [i32; 3], [i32; 9])], parts: &[GenPart], links: &[GenLink]) -> (usize, usize, usize) {
        self.apply_build_replacing(body, remove, &[], parts, links)
    }

    /// As [`Model::apply_build`], also removing the wires `remove_links` (a previous generation's).
    pub fn apply_build_replacing(&mut self, body: usize, remove: &[(String, [i32; 3], [i32; 9])], remove_links: &[GenLink], parts: &[GenPart], links: &[GenLink]) -> (usize, usize, usize) {
        if self.components_el_mut(body).is_none() {
            return (0, 0, 0);
        }
        self.checkpoint();
        let want: std::collections::HashSet<(String, [i32; 3], [i32; 9])> = remove.iter().cloned().collect();
        let ids: Vec<CompId> = self
            .components()
            .into_iter()
            .filter(|(id, c)| id.body == body && want.contains(&(c.def.clone(), c.voxel, c.rotation)))
            .map(|(id, _)| id)
            .collect();
        let removed = ids.len();
        let undo_len = self.undo.len();
        self.delete(&ids);
        self.undo.truncate(undo_len);
        let mut added = 0;
        for p in parts {
            if let Some(id) = self.add_inner(body, &p.def, p.vp, p.r, p.bc.as_deref()) {
                added += 1;
                if let Some(ac) = &p.ac {
                    if let Some(o) = self.component_o_mut(id) {
                        o.set("ac", ac);
                    }
                }
            }
        }
        let mut nl = 0;
        if self.doc.root.find("logic_node_links").is_none() {
            self.doc.root.push(Element::new("logic_node_links"));
        }
        if let Some(ls) = self.doc.root.find_mut("logic_node_links") {
            if !remove_links.is_empty() {
                let pos = |e: &Element, tag: &str| -> [i32; 3] {
                    let p = e.find(tag);
                    [0, 1, 2].map(|i| p.and_then(|p| p.get(["x", "y", "z"][i])).and_then(|v| v.parse().ok()).unwrap_or(0))
                };
                let gone: std::collections::HashSet<(i32, [i32; 3], [i32; 3])> = remove_links.iter().map(|l| (l.kind, l.a, l.b)).collect();
                ls.children.retain(|c| match c {
                    keel_io::xml::Node::Element(e) if e.tag == "logic_node_link" => {
                        let k = e.get("type").and_then(|v| v.parse().ok()).unwrap_or(0);
                        !gone.contains(&(k, pos(e, "voxel_pos_0"), pos(e, "voxel_pos_1")))
                    }
                    _ => true,
                });
            }
            for l in links {
                let mut e = Element::new("logic_node_link");
                e.set("type", &l.kind.to_string());
                for (tag, v) in [("voxel_pos_0", l.a), ("voxel_pos_1", l.b)] {
                    let mut p = Element::new(tag);
                    for (i, a) in ["x", "y", "z"].iter().enumerate() {
                        if v[i] != 0 {
                            p.set(a, &v[i].to_string());
                        }
                    }
                    e.push(p);
                }
                ls.push(e);
                nl += 1;
            }
        }
        self.generation += 1;
        (removed, added, nl)
    }

    fn component_o_mut(&mut self, id: CompId) -> Option<&mut Element> {
        let cs = self.components_el_mut(id.body)?;
        cs.elements_mut().filter(|e| e.tag == "c").nth(id.comp)?.find_mut("o")
    }

    /// Add a new component of definition `def` at voxel `vp` (body `body`).
    pub fn add(&mut self, body: usize, def: &str, vp: [i32; 3], rotation: [i32; 9]) -> Option<CompId> {
        self.components_el_mut(body)?;
        self.checkpoint();
        self.add_inner(body, def, vp, rotation, None)
    }

    fn add_inner(&mut self, body: usize, def: &str, vp: [i32; 3], rotation: [i32; 9], bc: Option<&str>) -> Option<CompId> {
        let mut c = Element::new("c");
        if def != "01_block" {
            c.set("d", def);
        }
        let mut o = Element::new("o");
        if rotation != DEFAULT_ROTATION {
            o.set("r", &rotation.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
        }
        o.set("sc", "6,x,x,x,x,x,x");
        if let Some(bc) = bc {
            o.set("bc", bc);
            o.set("ac", bc);
        }
        let mut vpe = Element::new("vp");
        for (i, a) in ["x", "y", "z"].iter().enumerate() {
            if vp[i] != 0 {
                vpe.set(a, &vp[i].to_string());
            }
        }
        o.push(vpe);
        c.push(o);
        let cs = self.components_el_mut(body)?;
        let n = cs.elements().filter(|e| e.tag == "c").count();
        cs.push(c);
        Some(CompId { body, comp: n })
    }
}

#[cfg(test)]
mod apply_build_tests {
    use super::*;

    #[test]
    fn apply_build_adds_parts_paint_and_links_in_one_undo_step() {
        let mut m = Model::new_empty();
        let parts = vec![
            GenPart { def: "small_light".into(), vp: [1, 2, 3], r: IDENTITY, bc: None, ac: Some("FFD9A0".into()) },
            GenPart { def: "button_push".into(), vp: [0, 2, 3], r: IDENTITY, bc: None, ac: None },
        ];
        let links = vec![GenLink { kind: 0, a: [0, 2, 3], b: [1, 2, 3] }];
        let (_, added, nl) = m.apply_build(0, &[], &parts, &links);
        assert_eq!((added, nl), (2, 1));
        let x = m.to_xml();
        assert!(x.contains("ac=\"FFD9A0\""), "{x}");
        assert!(x.contains("<logic_node_link type=\"0\">"), "{x}");
        assert!(m.undo());
        assert!(!m.to_xml().contains("small_light"));
    }

    #[test]
    fn port_voxel_rotates_column_major() {
        // quarter turn about y: local +x maps to world -z
        let r = [0, 0, -1, 0, 1, 0, 1, 0, 0];
        assert_eq!(port_voxel([5, 0, 0], &r, [1, 0, 0]), [5, 0, -1]);
    }
}

