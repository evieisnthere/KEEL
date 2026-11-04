//! Graph view of a `microprocessor_definition` element. Each component and IO node keeps its original XML element;
//! writing back updates only what the editor owns (ids, type, position, wires, edited properties, IO node fields) so
//! everything else (Lua scripts, dropdown items, icon bitmap, unknown attributes) is preserved byte for byte.
use crate::{bridge_type, comp_type, CompType, Prop, PropKind, Sig, Store};
use keel_io::xml::Element;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    /// source component id
    pub src: i32,
    /// output index on the source (`node_index`)
    pub out: usize,
}

#[derive(Clone, Debug)]
pub struct Comp {
    pub id: i32,
    /// catalogue type, or the bridge type for IO bridges
    pub ty: i32,
    pub bridge: bool,
    /// editor position (0.25 grid; y up)
    pub pos: [f32; 2],
    pub inputs: Vec<Option<Link>>,
    el: Element,
}

#[derive(Clone, Debug)]
pub struct IoNode {
    pub id: i32,
    /// the bridge component's id
    pub comp: i32,
    pub label: String,
    pub description: String,
    pub input: bool,
    pub sig: Sig,
    /// cell on the microcontroller footprint (x, z)
    pub pos: [i32; 2],
    el: Element,
}

#[derive(Clone, Debug)]
pub struct Graph {
    pub name: String,
    pub description: String,
    pub width: i32,
    pub length: i32,
    pub nodes: Vec<IoNode>,
    pub comps: Vec<Comp>,
    id_counter: i32,
    id_counter_node: i32,
    root: Element,
    /// the definition has nested groups (shown read-only; preserved)
    pub nested_groups: usize,
}

/// The XML tag of input `k` (0-based). The multi-channel composite writers keep their composite input in `inc` and
/// their channels in `in1..inN`.
pub fn in_tag(ty: i32, bridge: bool, k: usize) -> String {
    if !bridge && matches!(ty, 40 | 41) {
        if k == 0 {
            "inc".into()
        } else {
            format!("in{k}")
        }
    } else {
        format!("in{}", k + 1)
    }
}

/// Input index (0-based) of an input tag, the inverse of [`in_tag`].
fn in_index(ty: i32, bridge: bool, tag: &str) -> Option<usize> {
    let multi = !bridge && matches!(ty, 40 | 41);
    if multi && tag == "inc" {
        return Some(0);
    }
    let k: usize = tag.strip_prefix("in")?.parse().ok()?;
    if k == 0 {
        return None;
    }
    Some(if multi { k } else { k - 1 })
}

fn num(e: Option<&Element>, k: &str) -> f32 {
    e.and_then(|e| e.get(k)).and_then(|v| v.trim().parse().ok()).unwrap_or(0.0)
}
fn int(e: &Element, k: &str) -> i32 {
    e.get(k).and_then(|v| v.trim().parse().ok()).unwrap_or(0)
}
fn set_or_clear(e: &mut Element, k: &str, v: f32) {
    if v == 0.0 {
        e.remove_attr(k);
    } else {
        e.set(k, &fmt_num(v as f64));
    }
}
/// Numbers written the way the game does: no trailing zeros.
pub fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v}");
        s
    }
}

impl Comp {
    fn obj(&self) -> Option<&Element> {
        self.el.find("object")
    }
    fn obj_mut(&mut self) -> &mut Element {
        if self.el.find("object").is_none() {
            self.el.push(Element::new("object"));
        }
        self.el.find_mut("object").unwrap()
    }
    /// Channel count of a multi-channel composite writer.
    pub fn count(&self) -> usize {
        self.obj().map(|o| int(o, "count")).unwrap_or(0).max(1) as usize
    }
    pub fn prop(&self, p: &Prop) -> String {
        let Some(o) = self.obj() else { return String::new() };
        match p.store {
            Store::Attr => o.get(p.key).unwrap_or_default(),
            Store::Val => o.find(p.key).and_then(|v| v.get("text").or_else(|| v.get("value"))).unwrap_or_default(),
        }
    }
    pub fn prop_num(&self, key: &str) -> f64 {
        let Some(o) = self.obj() else { return 0.0 };
        o.get(key)
            .or_else(|| o.find(key).and_then(|v| v.get("value").or_else(|| v.get("text"))))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0.0)
    }
    pub fn set_prop(&mut self, p: &Prop, v: &str) {
        let o = self.obj_mut();
        match p.store {
            Store::Attr => {
                if v.is_empty() || (p.kind == PropKind::Number && v.trim().parse::<f64>().ok() == Some(0.0)) {
                    o.remove_attr(p.key);
                } else {
                    o.set(p.key, v);
                }
            }
            Store::Val => {
                if o.find(p.key).is_none() {
                    o.push(Element::new(p.key));
                }
                let c = o.find_mut(p.key).unwrap();
                c.set("text", v);
                match v.trim().parse::<f64>() {
                    Ok(x) if x != 0.0 => c.set("value", &fmt_num(x)),
                    _ => c.remove_attr("value"),
                }
            }
        }
    }
    /// Text of a raw attribute (e.g. a Lua script).
    pub fn attr(&self, k: &str) -> Option<String> {
        self.obj().and_then(|o| o.get(k))
    }
}

impl Graph {
    /// An empty definition of the given footprint.
    pub fn new_empty(name: &str, width: i32, length: i32) -> Graph {
        let src = format!(
            "<microprocessor_definition name=\"{}\" description=\"\" width=\"{width}\" length=\"{length}\" id_counter=\"0\" id_counter_node=\"0\"><nodes/><group><data><inputs/><outputs/></data><components/><components_bridge/><groups/></group></microprocessor_definition>",
            keel_io::xml::escape_attr(name)
        );
        let doc = keel_io::xml::parse(&src).expect("static xml");
        Graph::parse(&doc.root)
    }

    pub fn parse(root: &Element) -> Graph {
        let mut g = Graph {
            name: root.get("name").unwrap_or_default(),
            description: root.get("description").unwrap_or_default(),
            width: int(root, "width").max(1),
            length: int(root, "length").max(1),
            nodes: vec![],
            comps: vec![],
            id_counter: int(root, "id_counter"),
            id_counter_node: int(root, "id_counter_node"),
            root: root.clone(),
            nested_groups: 0,
        };
        if let Some(ns) = root.find("nodes") {
            for n in ns.elements().filter(|e| e.tag == "n") {
                let node = n.find("node");
                let pos = node.and_then(|x| x.find("position"));
                g.nodes.push(IoNode {
                    id: int(n, "id"),
                    comp: int(n, "component_id"),
                    label: node.and_then(|x| x.get("label")).unwrap_or_default(),
                    description: node.and_then(|x| x.get("description")).unwrap_or_default(),
                    input: node.map(|x| int(x, "mode")).unwrap_or(0) == 1,
                    sig: Sig::from_link_kind(node.map(|x| int(x, "type")).unwrap_or(0)),
                    pos: [num(pos, "x") as i32, num(pos, "z") as i32],
                    el: n.clone(),
                });
            }
        }
        if let Some(grp) = root.find("group") {
            for (tag, bridge) in [("components", false), ("components_bridge", true)] {
                let Some(cs) = grp.find(tag) else { continue };
                for c in cs.elements().filter(|e| e.tag == "c") {
                    let o = c.find("object");
                    let ty = int(c, "type");
                    let mut inputs: Vec<Option<Link>> = vec![];
                    if let Some(o) = o {
                        for e in o.elements() {
                            let Some(k) = in_index(ty, bridge, &e.tag) else { continue };
                            if inputs.len() <= k {
                                inputs.resize(k + 1, None);
                            }
                            if let Some(src) = e.get("component_id").and_then(|v| v.parse().ok()) {
                                inputs[k] = Some(Link { src, out: e.get("node_index").and_then(|v| v.parse().ok()).unwrap_or(0) });
                            }
                        }
                    }
                    let mut comp = Comp { id: o.map(|o| int(o, "id")).unwrap_or(0), ty, bridge, pos: [num(o.and_then(|o| o.find("pos")), "x"), num(o.and_then(|o| o.find("pos")), "y")], inputs, el: c.clone() };
                    let n = g.ctype_of(&comp).inputs.len();
                    if comp.inputs.len() < n {
                        comp.inputs.resize(n, None);
                    }
                    g.comps.push(comp);
                }
            }
            g.nested_groups = grp.find("groups").map(|x| x.elements().count()).unwrap_or(0);
        }
        g.id_counter = g.id_counter.max(g.comps.iter().map(|c| c.id).max().unwrap_or(0));
        g.id_counter_node = g.id_counter_node.max(g.nodes.iter().map(|n| n.id).max().unwrap_or(0));
        g
    }

    pub fn ctype_of(&self, c: &Comp) -> CompType {
        if c.bridge {
            let label = self.nodes.iter().find(|n| n.comp == c.id).map(|n| n.label.clone()).unwrap_or_else(|| "IO".into());
            bridge_type(c.ty, &label)
        } else {
            comp_type(c.ty, c.count()).unwrap_or_else(|| crate::CompType { id: c.ty, name: "Unknown", group: "Other", about: "Unknown component type (preserved)", inputs: vec![], outputs: vec![], props: vec![] })
        }
    }

    pub fn comp(&self, id: i32) -> Option<&Comp> {
        self.comps.iter().find(|c| c.id == id)
    }
    pub fn comp_mut(&mut self, id: i32) -> Option<&mut Comp> {
        self.comps.iter_mut().find(|c| c.id == id)
    }

    /// Add a component of catalogue type `ty`; returns its id.
    pub fn add(&mut self, ty: i32, pos: [f32; 2]) -> i32 {
        self.id_counter += 1;
        let id = self.id_counter;
        let mut c = Element::new("c");
        if ty != 0 {
            c.set("type", &ty.to_string());
        }
        let mut o = Element::new("object");
        o.set("id", &id.to_string());
        o.push(Element::new("pos"));
        c.push(o);
        let mut comp = Comp { id, ty, bridge: false, pos, inputs: vec![], el: c };
        // sensible defaults so new parts behave as in the game's editor
        for (k, v) in match ty {
            11 | 12 | 19 => vec![("max", "1")],
            23 => vec![("kp", "1")],
            26 => vec![("ct", "1"), ("dt", "1")],
            27 => vec![("on", "1"), ("off", "1")],
            37 => vec![("i", "1"), ("max", "10")],
            40 | 41 => vec![("count", "1")],
            42 => vec![("e", "0.01")],
            10 => vec![("e", "x+y+z")],
            36 => vec![("e", "x")],
            45 => vec![("e", "x")],
            46 | 47 => vec![("e", "x&y")],
            56 => vec![("script", "function onTick()\n\tlocal x = input.getNumber(1)\n\toutput.setNumber(1, x)\nend\n")],
            _ => vec![],
        } {
            if let Some(p) = comp_type(ty, 1).and_then(|t| t.props.into_iter().find(|p| p.key == k)) {
                comp.set_prop(&p, v);
            }
        }
        let n = self.ctype_of(&comp).inputs.len();
        comp.inputs.resize(n, None);
        self.comps.push(comp);
        id
    }

    /// Add an IO node and its bridge; returns the node id.
    pub fn add_io(&mut self, input: bool, sig: Sig, label: &str, pos: [f32; 2]) -> i32 {
        self.id_counter += 1;
        self.id_counter_node += 1;
        let (cid, nid) = (self.id_counter, self.id_counter_node);
        let bt = sig.bridge_type(input);
        let mut c = Element::new("c");
        if bt != 0 {
            c.set("type", &bt.to_string());
        }
        let mut o = Element::new("object");
        o.set("id", &cid.to_string());
        o.push(Element::new("pos"));
        c.push(o);
        self.comps.push(Comp { id: cid, ty: bt, bridge: true, pos, inputs: if input { vec![] } else { vec![None] }, el: c });
        // first free footprint cell
        let used: Vec<[i32; 2]> = self.nodes.iter().map(|n| n.pos).collect();
        let cell = (0..self.length).flat_map(|z| (0..self.width).map(move |x| [x, z])).find(|c| !used.contains(c)).unwrap_or([0, 0]);
        let mut n = Element::new("n");
        n.set("id", &nid.to_string());
        n.set("component_id", &cid.to_string());
        n.push(Element::new("node"));
        self.nodes.push(IoNode { id: nid, comp: cid, label: label.into(), description: String::new(), input, sig, pos: cell, el: n });
        nid
    }

    /// Change an IO node's direction or type (its bridge follows; incompatible wires are dropped).
    pub fn set_io_kind(&mut self, node: i32, input: bool, sig: Sig) {
        let Some(n) = self.nodes.iter_mut().find(|n| n.id == node) else { return };
        n.input = input;
        n.sig = sig;
        let cid = n.comp;
        if let Some(c) = self.comp_mut(cid) {
            c.ty = sig.bridge_type(input);
            c.inputs = if input { vec![] } else { vec![c.inputs.first().copied().flatten()] };
        }
        self.drop_invalid_links();
    }

    /// Remove components (and IO nodes whose bridge is removed); wires from them are cut.
    pub fn remove(&mut self, ids: &[i32]) {
        self.comps.retain(|c| !ids.contains(&c.id));
        self.nodes.retain(|n| !ids.contains(&n.comp));
        for c in &mut self.comps {
            for i in c.inputs.iter_mut() {
                if i.is_some_and(|l| ids.contains(&l.src)) {
                    *i = None;
                }
            }
        }
    }

    /// Wire output `out` of `src` to input `inp` of `dst`, replacing what was there. Types must match.
    pub fn connect(&mut self, src: i32, out: usize, dst: i32, inp: usize) -> Result<(), String> {
        let s = self.comp(src).ok_or("no such source")?;
        let d = self.comp(dst).ok_or("no such destination")?;
        let so = self.ctype_of(s).outputs.get(out).map(|o| o.1).ok_or("no such output")?;
        let di = self.ctype_of(d).inputs.get(inp).map(|o| o.1).ok_or("no such input")?;
        if so != di {
            return Err(format!("Cannot wire {} to {}", so.name(), di.name()));
        }
        let d = self.comp_mut(dst).unwrap();
        if d.inputs.len() <= inp {
            d.inputs.resize(inp + 1, None);
        }
        d.inputs[inp] = Some(Link { src, out });
        Ok(())
    }

    pub fn disconnect(&mut self, dst: i32, inp: usize) {
        if let Some(i) = self.comp_mut(dst).and_then(|d| d.inputs.get_mut(inp)) {
            *i = None;
        }
    }

    /// Cut wires whose ends no longer exist or no longer match in type (after type or count changes).
    pub fn drop_invalid_links(&mut self) {
        let types: std::collections::HashMap<i32, CompType> = self.comps.iter().map(|c| (c.id, self.ctype_of(c))).collect();
        for c in &mut self.comps {
            let t = &types[&c.id];
            if c.inputs.len() < t.inputs.len() {
                c.inputs.resize(t.inputs.len(), None);
            }
            for (k, i) in c.inputs.iter_mut().enumerate() {
                if let Some(l) = i {
                    let ok = types.get(&l.src).and_then(|s| s.outputs.get(l.out)).map(|o| o.1) == t.inputs.get(k).map(|x| x.1);
                    if !ok {
                        *i = None;
                    }
                }
            }
        }
    }

    /// The definition element, with the editor's changes applied to a copy of the original.
    pub fn to_element(&self) -> Element {
        let mut root = self.root.clone();
        // attributes the original left out stay out while they hold their defaults
        let keep = |root: &mut Element, k: &str, v: String, default: &str| {
            if root.get(k).is_some() || v != default {
                root.set(k, &v);
            }
        };
        keep(&mut root, "name", self.name.clone(), "");
        keep(&mut root, "description", self.description.clone(), "");
        keep(&mut root, "width", self.width.to_string(), "");
        keep(&mut root, "length", self.length.to_string(), "");
        keep(&mut root, "id_counter", self.id_counter.to_string(), "0");
        keep(&mut root, "id_counter_node", self.id_counter_node.to_string(), "0");
        if root.find("nodes").is_none() {
            root.insert(0, Element::new("nodes"));
        }
        let nodes = root.find_mut("nodes").unwrap();
        nodes.retain_elements(|_| false);
        for n in &self.nodes {
            let mut e = n.el.clone();
            e.set("id", &n.id.to_string());
            e.set("component_id", &n.comp.to_string());
            if e.find("node").is_none() {
                e.push(Element::new("node"));
            }
            let node = e.find_mut("node").unwrap();
            if node.get("label").is_some() || !n.label.is_empty() {
                node.set("label", &n.label);
            }
            set_or_clear(node, "mode", if n.input { 1.0 } else { 0.0 });
            set_or_clear(node, "type", n.sig.link_kind() as f32);
            if n.description.is_empty() {
                node.remove_attr("description");
            } else {
                node.set("description", &n.description);
            }
            if n.pos != [0, 0] {
                if node.find("position").is_none() {
                    node.push(Element::new("position"));
                }
                let p = node.find_mut("position").unwrap();
                set_or_clear(p, "x", n.pos[0] as f32);
                set_or_clear(p, "z", n.pos[1] as f32);
            } else {
                node.retain_elements(|c| c.tag != "position");
            }
            nodes.push(e);
        }
        if root.find("group").is_none() {
            let doc = keel_io::xml::parse("<group><data><inputs/><outputs/></data><components/><components_bridge/><groups/></group>").unwrap();
            root.push(doc.root);
        }
        let grp = root.find_mut("group").unwrap();
        for (tag, bridge) in [("components", false), ("components_bridge", true)] {
            if grp.find(tag).is_none() {
                grp.push(Element::new(tag));
            }
            let cs = grp.find_mut(tag).unwrap();
            cs.retain_elements(|_| false);
            for c in self.comps.iter().filter(|c| c.bridge == bridge) {
                cs.push(write_comp(c));
            }
        }
        root
    }
}

fn write_comp(c: &Comp) -> Element {
    let mut c2 = c.clone();
    set_or_clear(&mut c2.el, "type", c.ty as f32);
    let o = c2.obj_mut();
    o.set("id", &c.id.to_string());
    if o.find("pos").is_none() {
        o.insert(0, Element::new("pos"));
    }
    let p = o.find_mut("pos").unwrap();
    set_or_clear(p, "x", c.pos[0]);
    set_or_clear(p, "y", c.pos[1]);
    // wires: update existing inN elements (keeping extra attributes such as typed default values), add or drop others
    let pos_idx = o.children.iter().position(|n| matches!(n, keel_io::xml::Node::Element(e) if e.tag == "pos")).unwrap_or(0);
    let mut insert_at = pos_idx + 1;
    for (k, l) in c.inputs.iter().enumerate() {
        let tag = in_tag(c.ty, c.bridge, k);
        match (o.find_mut(&tag), l) {
            (Some(e), Some(l)) => {
                e.set("component_id", &l.src.to_string());
                set_or_clear(e, "node_index", l.out as f32);
            }
            (Some(e), None) => {
                e.remove_attr("component_id");
                e.remove_attr("node_index");
            }
            (None, Some(l)) => {
                let mut e = Element::new(&tag);
                e.set("component_id", &l.src.to_string());
                set_or_clear(&mut e, "node_index", l.out as f32);
                o.insert(insert_at, e);
            }
            (None, None) => {}
        }
        if let Some(i) = o.children.iter().position(|n| matches!(n, keel_io::xml::Node::Element(e) if e.tag == tag)) {
            insert_at = i + 1;
        }
    }
    // inputs that no longer exist; wire elements emptied by the editor (the original's empty ones stay)
    let n = c.inputs.len();
    let orig: Vec<String> = c.obj().map(|o| o.elements().filter(|e| e.attrs.is_empty()).map(|e| e.tag.clone()).collect()).unwrap_or_default();
    let (ty, bridge) = (c.ty, c.bridge);
    o.retain_elements(|e| match in_index(ty, bridge, &e.tag) {
        Some(k) => k < n && (!e.attrs.is_empty() || orig.contains(&e.tag)),
        None => true,
    });
    c2.el
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<microprocessor_definition name="Test" width="2" length="1" id_counter="9" id_counter_node="2" sym0="5"><nodes><n id="1" component_id="5"><node label="In" mode="1" description="x"/></n><n id="2" component_id="6"><node label="Out"><position x="1"/></node></n></nodes><group><data><inputs/><outputs/></data><components><c><object id="7"><pos x="-1"/><in1 component_id="5"/></object></c><c type="15"><object id="8"><pos y="0.5"/><n text="3" value="3"/></object></c></components><components_bridge><c><object id="5"><pos x="-3"/></object></c><c type="1"><object id="6"><pos x="1"/><in1 component_id="7"/></object></c></components_bridge><groups/></group></microprocessor_definition>"#;

    #[test]
    fn round_trip_is_lossless_and_edits_apply() {
        let doc = keel_io::xml::parse(SAMPLE).unwrap();
        let g = Graph::parse(&doc.root);
        assert_eq!(g.comps.len(), 4);
        assert_eq!(g.nodes.len(), 2);
        let mut s = String::new();
        g.to_element().write(&mut s);
        let g2 = Graph::parse(&keel_io::xml::parse(&s).unwrap().root);
        assert_eq!(g2.comps.iter().map(|c| (c.id, c.ty, c.pos, c.inputs.clone())).collect::<Vec<_>>(), g.comps.iter().map(|c| (c.id, c.ty, c.pos, c.inputs.clone())).collect::<Vec<_>>());
        assert!(s.contains("sym0=\"5\""));
        assert!(s.contains(r#"<n text="3" value="3"/>"#));

        let mut g = g;
        let add = g.add(6, [0.0, 1.0]);
        assert!(g.connect(8, 0, add, 0).is_ok());
        assert!(g.connect(7, 0, add, 1).is_err(), "on/off into number");
        g.remove(&[7]);
        assert_eq!(g.comp(6).unwrap().inputs[0], None);
        let mut s = String::new();
        g.to_element().write(&mut s);
        let g3 = Graph::parse(&keel_io::xml::parse(&s).unwrap().root);
        assert_eq!(g3.comp(add).unwrap().inputs[0], Some(Link { src: 8, out: 0 }));
        assert!(!s.contains("<in1/>"));
    }
}
