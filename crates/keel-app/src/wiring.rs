//! Wiring tool: ports drawn over the 3D view, coloured by type; click an output then an input (or two electric /
//! composite / video / audio ports) to add a wire; click a wire to select it, Delete removes it.
use crate::theme;
use crate::viewport::View;
use eframe::egui::{self, Color32, Pos2, Stroke};
use keel_model::catalogue::Catalogue;
use keel_model::edit::{port_voxel, GenLink};
use keel_model::{CompId, Model};

#[derive(Debug, Clone)]
pub struct Port {
    pub comp: CompId,
    pub def: String,
    pub label: String,
    pub kind: i32,
    pub mode: i32,
    pub voxel: [i32; 3],
}

pub fn kind_name(k: i32) -> &'static str {
    match k {
        0 => "on/off",
        1 => "number",
        4 => "electric",
        5 => "composite",
        6 => "video",
        7 => "audio",
        _ => "other",
    }
}

pub fn kind_colour(k: i32) -> Color32 {
    match k {
        0 => Color32::from_rgb(0x6a, 0xe0, 0x8a),
        1 => Color32::from_rgb(0x5a, 0xa8, 0xff),
        4 => Color32::from_rgb(0xff, 0xd0, 0x4a),
        5 => Color32::from_rgb(0xc0, 0x80, 0xff),
        6 => Color32::from_rgb(0xff, 0x7a, 0xb0),
        7 => Color32::from_rgb(0x9a, 0xe8, 0xe8),
        _ => theme::TEXT_DIM,
    }
}

#[derive(Default)]
pub struct Wiring {
    pub pending: Option<Port>,
    pub selected: Option<GenLink>,
    /// show the ports of every part (otherwise only the selected parts')
    pub all: bool,
    cache_gen: u64,
    ports: Vec<Port>,
    pub status: String,
}

fn centre(v: [i32; 3]) -> [f32; 3] {
    [v[0] as f32 * 0.25, v[1] as f32 * 0.25, v[2] as f32 * 0.25]
}

impl Wiring {
    fn refresh(&mut self, model: &Model, cat: &Catalogue) {
        if self.cache_gen == model.generation && !self.ports.is_empty() {
            return;
        }
        self.cache_gen = model.generation;
        self.ports.clear();
        for (id, c) in model.components() {
            // a microcontroller's ports come from its own definition: one per IO node, at its footprint cell
            if c.def == "microprocessor" {
                if let Some(d) = model.mc_definition(id) {
                    for n in keel_mc::graph::Graph::parse(&d).nodes {
                        self.ports.push(Port { comp: id, def: c.def.clone(), label: n.label.clone(), kind: n.sig.link_kind(), mode: if n.input { 1 } else { 0 }, voxel: port_voxel(c.voxel, &c.rotation, [n.pos[0], 0, n.pos[1]]) });
                    }
                }
                continue;
            }
            let Some(d) = cat.get(&c.def) else { continue };
            for n in d.logic_nodes.iter().filter(|n| !matches!(n.kind, 2 | 3)) {
                self.ports.push(Port { comp: id, def: c.def.clone(), label: n.label.clone(), kind: n.kind, mode: n.mode, voxel: port_voxel(c.voxel, &c.rotation, n.position) });
            }
        }
    }

    /// Wires of a component (for the inspector).
    pub fn links_of(&mut self, model: &Model, cat: &Catalogue, comp: CompId) -> Vec<(GenLink, String)> {
        self.refresh(model, cat);
        let mine: Vec<&Port> = self.ports.iter().filter(|p| p.comp == comp).collect();
        model
            .links()
            .into_iter()
            .filter_map(|l| {
                let p = mine.iter().find(|p| p.kind == l.kind && (p.voxel == l.a || p.voxel == l.b))?;
                let other = if p.voxel == l.a { l.b } else { l.a };
                let o = self.ports.iter().find(|q| q.kind == l.kind && q.voxel == other && q.comp != comp);
                let desc = format!("{} to {}", p.label, o.map(|o| format!("{} ({})", o.label, o.def)).unwrap_or_else(|| format!("{other:?} (nothing there)")));
                Some((l, desc))
            })
            .collect()
    }

    /// Draw ports and wires; handle a click. Returns a wire to add, if completed.
    pub fn overlay(&mut self, ui: &egui::Ui, view: &View, model: &Model, cat: &Catalogue, selection: &std::collections::HashSet<CompId>, click: Option<Pos2>) -> Option<GenLink> {
        self.refresh(model, cat);
        let painter = ui.painter();
        let links = model.links();
        // wires
        for l in &links {
            let (Some((a, _)), Some((b, _))) = (view.project(centre(l.a)), view.project(centre(l.b))) else { continue };
            let sel = self.selected.as_ref() == Some(l);
            painter.line_segment([a, b], Stroke::new(if sel { 3.0 } else { 1.5 }, if sel { theme::CYAN } else { kind_colour(l.kind).gamma_multiply(0.8) }));
        }
        // ports
        let mut hover: Option<(usize, f32)> = None;
        let pointer = ui.ctx().pointer_hover_pos();
        let mut shown = vec![];
        for (i, p) in self.ports.iter().enumerate() {
            if !self.all && !selection.contains(&p.comp) && self.pending.as_ref().is_none_or(|q| q.comp != p.comp) {
                continue;
            }
            let Some((s, _)) = view.project(centre(p.voxel)) else { continue };
            shown.push((i, s));
            if let Some(m) = pointer {
                let d = m.distance(s);
                if d < 9.0 && hover.is_none_or(|h| d < h.1) {
                    hover = Some((i, d));
                }
            }
        }
        for (i, s) in &shown {
            let p = &self.ports[*i];
            let compatible = self.pending.as_ref().is_none_or(|q| q.kind == p.kind && q.voxel != p.voxel);
            let col = if compatible { kind_colour(p.kind) } else { kind_colour(p.kind).gamma_multiply(0.25) };
            let r = if hover.map(|h| h.0) == Some(*i) { 6.5 } else { 4.5 };
            // outputs are filled, inputs ringed
            if p.mode == 0 {
                painter.circle_filled(*s, r, col);
            } else {
                painter.circle_stroke(*s, r, Stroke::new(2.0, col));
            }
        }
        if let Some((i, _)) = hover {
            let p = &self.ports[i];
            let name = cat.get(&p.def).map(|d| d.name.clone()).unwrap_or_default();
            if let Some(m) = pointer {
                painter.text(m + egui::vec2(12.0, -12.0), egui::Align2::LEFT_BOTTOM, format!("{name}: {} ({}, {})", p.label, if p.mode == 0 { "out" } else { "in" }, kind_name(p.kind)), egui::FontId::proportional(12.0), theme::TEXT);
            }
        }
        if let (Some(q), Some(m)) = (&self.pending, pointer) {
            if let Some((a, _)) = view.project(centre(q.voxel)) {
                painter.line_segment([a, m], Stroke::new(1.5, kind_colour(q.kind)));
            }
        }
        let click = click?;
        if let Some((i, _)) = hover {
            let p = self.ports[i].clone();
            match self.pending.take() {
                None => {
                    self.status = format!("Wiring from {} ({}); click a {} port", p.label, kind_name(p.kind), kind_name(p.kind));
                    self.pending = Some(p);
                }
                Some(q) if q.kind == p.kind && q.voxel != p.voxel => {
                    // logic and number wires join an output to an input
                    if p.kind <= 1 && q.mode == p.mode {
                        self.status = "A logic wire joins an output to an input".into();
                        self.pending = Some(q);
                        return None;
                    }
                    let (a, b) = if q.mode == 0 { (q.voxel, p.voxel) } else { (p.voxel, q.voxel) };
                    let l = GenLink { kind: p.kind, a, b };
                    if links.contains(&l) {
                        self.status = "Already wired".into();
                        return None;
                    }
                    self.status = format!("Wired {} to {}", q.label, p.label);
                    return Some(l);
                }
                Some(_) => {
                    self.status = "Different port types: pick a matching port".into();
                    self.pending = None;
                }
            }
            return None;
        }
        // a click near a wire selects it
        let near = links.iter().filter_map(|l| {
            let (a, _) = view.project(centre(l.a))?;
            let (b, _) = view.project(centre(l.b))?;
            let ab = b - a;
            let t = ((click - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
            Some((l.clone(), click.distance(a + ab * t)))
        });
        self.selected = near.filter(|(_, d)| *d < 6.0).min_by(|x, y| x.1.partial_cmp(&y.1).unwrap()).map(|(l, _)| l);
        self.pending = None;
        None
    }
}
