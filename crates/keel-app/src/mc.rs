//! Microcontroller editor (Logic mode): pick a microcontroller in the vehicle, edit its node graph on a canvas
//! (components, wires, IO nodes, properties) and run it in the tick simulator. Every edit is written straight back to
//! the vehicle as one undo step, so Ctrl+Z works across modes.
use crate::icons::{self, Icon};
use crate::theme;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use keel_mc::graph::Graph;
use keel_mc::sim::{Sim, Value};
use keel_mc::{all_types, PropKind, Sig, GROUPS};
use keel_model::{CompId, Model};
use std::collections::HashSet;

pub fn sig_colour(s: Sig) -> Color32 {
    crate::wiring::kind_colour(s.link_kind())
}

#[derive(Default)]
enum Drag {
    #[default]
    None,
    Pan,
    Move {
        origin: Pos2,
        start: Vec<(i32, [f32; 2])>,
    },
    Wire {
        src: i32,
        out: usize,
    },
    Band {
        start: Pos2,
    },
}

pub struct McEditor {
    pub target: Option<CompId>,
    graph: Option<Graph>,
    loaded_gen: u64,
    pan: Vec2,
    zoom: f32,
    selected: HashSet<i32>,
    drag: Drag,
    pub sim: Sim,
    pub running: bool,
    group: usize,
    search: String,
    pub status: String,
    /// edits made this frame, written to the vehicle at the end of the frame
    changed: bool,
    prop_buf: std::collections::HashMap<(i32, &'static str), String>,
}

impl Default for McEditor {
    fn default() -> Self {
        McEditor {
            target: None,
            graph: None,
            loaded_gen: u64::MAX,
            pan: Vec2::ZERO,
            zoom: 1.0,
            selected: HashSet::new(),
            drag: Drag::None,
            sim: Sim::default(),
            running: false,
            group: 0,
            search: String::new(),
            status: String::new(),
            changed: false,
            prop_buf: Default::default(),
        }
    }
}

const ROW: f32 = 0.22;
const HEAD: f32 = 0.26;
const BOX_W: f32 = 1.0;

/// Microcontrollers in the vehicle: (id, name, footprint).
pub fn list(model: &Model) -> Vec<(CompId, String, (i32, i32))> {
    model
        .components()
        .into_iter()
        .filter(|(_, c)| c.def == "microprocessor")
        .map(|(id, _)| {
            let d = model.mc_definition(id);
            let name = d.as_ref().and_then(|d| d.get("name")).filter(|n| !n.is_empty()).unwrap_or_else(|| "(unnamed)".into());
            let size = d.map(|d| (d.get("width").and_then(|v| v.parse().ok()).unwrap_or(1), d.get("length").and_then(|v| v.parse().ok()).unwrap_or(1))).unwrap_or((1, 1));
            (id, name, size)
        })
        .collect()
}

impl McEditor {
    /// Re-read the graph when the vehicle changed underneath (undo, other modes).
    pub fn sync(&mut self, model: &Model) {
        if let Some(t) = self.target {
            if model.components().iter().all(|(id, c)| *id != t || c.def != "microprocessor") {
                self.target = None;
                self.graph = None;
            }
        }
        if self.target.is_none() {
            if let Some((id, ..)) = list(model).first() {
                self.open(*id, model);
            }
        }
        if self.loaded_gen != model.generation {
            if let Some(t) = self.target {
                self.graph = Some(model.mc_definition(t).map(|d| Graph::parse(&d)).unwrap_or_else(|| Graph::new_empty("New microcontroller", 2, 2)));
                self.selected.retain(|id| self.graph.as_ref().unwrap().comp(*id).is_some());
            }
            self.loaded_gen = model.generation;
            self.prop_buf.clear();
        }
    }

    pub fn open(&mut self, id: CompId, model: &Model) {
        self.target = Some(id);
        self.loaded_gen = u64::MAX;
        self.selected.clear();
        self.sim = Sim::default();
        self.running = false;
        self.sync(model);
        self.frame_all();
    }

    fn frame_all(&mut self) {
        if let Some(g) = &self.graph {
            if g.comps.is_empty() {
                self.pan = Vec2::ZERO;
                return;
            }
            let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
            for c in &g.comps {
                lo = lo.min(Vec2::new(c.pos[0], -c.pos[1]));
                hi = hi.max(Vec2::new(c.pos[0] + BOX_W, -c.pos[1] + 1.0));
            }
            self.pan = -(lo + hi) * 0.5;
            let span = (hi - lo).max(Vec2::splat(2.0));
            self.zoom = (9.0 / span.x).min(6.0 / span.y).clamp(0.3, 1.6);
        }
    }

    /// Write pending edits to the vehicle (one undo step).
    pub fn commit(&mut self, model: &mut Model) {
        if !self.changed {
            return;
        }
        self.changed = false;
        if let (Some(t), Some(g)) = (self.target, &self.graph) {
            model.set_mc_definition(t, g.to_element());
            self.loaded_gen = model.generation;
        }
    }

    // ---- left panel ----------------------------------------------------------------------------------------------

    /// Microcontroller list and component palette. Returns a request to create a new microcontroller.
    pub fn panel_ui(&mut self, ui: &mut egui::Ui, model: &Model) -> bool {
        let mut create = false;
        theme::section(ui, "Microcontrollers");
        let mcs = list(model);
        if mcs.is_empty() {
            ui.label(RichText::new("This vehicle has no microcontrollers yet.").color(theme::TEXT_DIM));
        }
        egui::ScrollArea::vertical().id_salt("mc_list").max_height(150.0).show(ui, |ui| {
            for (id, name, (w, l)) in &mcs {
                let sel = self.target == Some(*id);
                let r = ui.selectable_label(sel, format!("{name}   {w}×{l}"));
                if r.clicked() && !sel {
                    self.open(*id, model);
                }
            }
        });
        if ui.button("New microcontroller").on_hover_text("Add a 2×2 microcontroller to the vehicle and open it").clicked() {
            create = true;
        }
        ui.separator();
        theme::section(ui, "Components");
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
            icons::paint(ui.painter(), r, Icon::Search, theme::TEXT_DIM);
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search components").desired_width(f32::INFINITY));
        });
        if self.search.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                for (i, g) in GROUPS.iter().enumerate() {
                    if ui.selectable_label(self.group == i, *g).clicked() {
                        self.group = i;
                    }
                }
            });
        }
        ui.add_space(4.0);
        let q = self.search.to_lowercase();
        let types: Vec<_> = all_types().into_iter().filter(|t| if q.is_empty() { t.group == GROUPS[self.group] } else { t.name.to_lowercase().contains(&q) }).collect();
        egui::ScrollArea::vertical().id_salt("mc_pal").show(ui, |ui| {
            for t in types {
                let r = ui.add_enabled(self.graph.is_some(), egui::Button::new(RichText::new(t.name)).min_size(Vec2::new(ui.available_width(), 26.0)));
                if r.on_hover_text(t.about).clicked() {
                    self.add_at_centre(t.id);
                }
            }
            ui.add_space(6.0);
            theme::section(ui, "Inputs and outputs");
            ui.horizontal_wrapped(|ui| {
                for s in [Sig::Bool, Sig::Num, Sig::Comp, Sig::Video, Sig::Audio] {
                    for input in [true, false] {
                        if ui.add_enabled(self.graph.is_some(), egui::Button::new(RichText::new(format!("{} {}", s.name(), if input { "in" } else { "out" })).color(sig_colour(s)))).clicked() {
                            if let Some(g) = &mut self.graph {
                                let p = [(-self.pan.x - 0.5 + if input { -2.0 } else { 2.0 }) * 4.0, self.pan.y * 4.0].map(|v| v.round() / 4.0);
                                let n = g.nodes.len() + 1;
                                g.add_io(input, s, &format!("{} {n}", if input { "Input" } else { "Output" }), p);
                                self.changed = true;
                            }
                        }
                    }
                }
            });
        });
        create
    }

    fn add_at_centre(&mut self, ty: i32) {
        if let Some(g) = &mut self.graph {
            let mut p = [((-self.pan.x - BOX_W / 2.0) * 4.0).round() / 4.0, (self.pan.y * 4.0).round() / 4.0];
            // step down until free
            while g.comps.iter().any(|c| (c.pos[0] - p[0]).abs() < 0.9 && (c.pos[1] - p[1]).abs() < 0.5) {
                p[1] -= 0.5;
            }
            let id = g.add(ty, p);
            self.selected = [id].into_iter().collect();
            self.changed = true;
        }
    }

    // ---- canvas --------------------------------------------------------------------------------------------------

    fn to_screen(&self, rect: Rect, p: [f32; 2]) -> Pos2 {
        let u = 90.0 * self.zoom;
        rect.center() + (Vec2::new(p[0], -p[1]) + self.pan) * u
    }

    fn box_rect(&self, rect: Rect, g: &Graph, c: &keel_mc::graph::Comp) -> Rect {
        let t = g.ctype_of(c);
        let rows = t.inputs.len().max(t.outputs.len()).max(1) as f32;
        let u = 90.0 * self.zoom;
        let w = if c.bridge { 0.8 } else { BOX_W };
        Rect::from_min_size(self.to_screen(rect, c.pos), Vec2::new(w, HEAD + rows * ROW) * u)
    }


    pub fn canvas(&mut self, ui: &mut egui::Ui, model: &mut Model) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::CANVAS);
        let Some(g) = self.graph.clone() else {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Select or create a microcontroller on the left", egui::FontId::proportional(16.0), theme::TEXT_DIM);
            return;
        };
        let u = 90.0 * self.zoom;
        // grid: 0.25 minor, 1 major
        {
            let step = 0.25;
            let tl = (rect.min - rect.center()) / u - self.pan;
            let br = (rect.max - rect.center()) / u - self.pan;
            let mut x = (tl.x / step).floor() * step;
            while x <= br.x {
                let major = (x.round() - x).abs() < 1e-3;
                if major || self.zoom > 0.6 {
                    let sx = rect.center().x + (x + self.pan.x) * u;
                    painter.line_segment([Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())], Stroke::new(if major { 1.0 } else { 0.5 }, if major { theme::GRID.gamma_multiply(0.75) } else { theme::GRID.gamma_multiply(0.35) }));
                }
                x += step;
            }
            let mut y = (tl.y / step).floor() * step;
            while y <= br.y {
                let major = (y.round() - y).abs() < 1e-3;
                if major || self.zoom > 0.6 {
                    let sy = rect.center().y + (y + self.pan.y) * u;
                    painter.line_segment([Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)], Stroke::new(if major { 1.0 } else { 0.5 }, if major { theme::GRID.gamma_multiply(0.75) } else { theme::GRID.gamma_multiply(0.35) }));
                }
                y += step;
            }
        }
        // zoom and pan
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let before = ui.ctx().pointer_hover_pos().map(|p| (p - rect.center()) / u - self.pan);
                self.zoom = (self.zoom * (scroll * 0.002).exp()).clamp(0.2, 4.0);
                if let (Some(b), Some(p)) = (before, ui.ctx().pointer_hover_pos()) {
                    self.pan = (p - rect.center()) / (90.0 * self.zoom) - b;
                }
            }
        }
        let z = self.zoom;
        let u = 90.0 * z;
        let rects: Vec<(i32, Rect)> = g.comps.iter().map(|c| (c.id, self.box_rect(rect, &g, c))).collect();
        let rect_of = |id: i32| rects.iter().find(|r| r.0 == id).map(|r| r.1);
        let pointer = ui.ctx().pointer_hover_pos();
        // hit tests
        let hit_port = |p: Pos2, output: bool| -> Option<(i32, usize)> {
            for c in &g.comps {
                let r = rect_of(c.id)?;
                let t = g.ctype_of(c);
                let n = if output { t.outputs.len() } else { t.inputs.len() };
                for k in 0..n {
                    if port_at(z, r, k, output).distance(p) < (0.08 * u).max(7.0) {
                        return Some((c.id, k));
                    }
                }
            }
            None
        };
        let hit_box = |p: Pos2| rects.iter().rev().find(|(_, r)| r.contains(p)).map(|(id, _)| *id);

        // interaction
        let mods = ui.input(|i| i.modifiers);
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let p = resp.interact_pointer_pos().unwrap_or_default();
            if let Some((src, out)) = hit_port(p, true) {
                self.drag = Drag::Wire { src, out };
            } else if let Some(id) = hit_box(p) {
                if !self.selected.contains(&id) {
                    if !mods.shift {
                        self.selected.clear();
                    }
                    self.selected.insert(id);
                }
                self.drag = Drag::Move { origin: p, start: g.comps.iter().filter(|c| self.selected.contains(&c.id)).map(|c| (c.id, c.pos)).collect() };
            } else {
                self.drag = Drag::Band { start: p };
            }
        }
        if resp.drag_started_by(egui::PointerButton::Secondary) || resp.drag_started_by(egui::PointerButton::Middle) {
            self.drag = Drag::Pan;
        }
        let mut moved: Option<Vec<(i32, [f32; 2])>> = None;
        match &self.drag {
            Drag::Pan => self.pan += resp.drag_delta() / u,
            Drag::Move { origin, start } => {
                if let Some(p) = pointer {
                    let d = (p - *origin) / u;
                    moved = Some(start.iter().map(|(id, s)| (*id, [((s[0] + d.x) * 4.0).round() / 4.0, ((s[1] - d.y) * 4.0).round() / 4.0])).collect());
                }
            }
            _ => {}
        }
        // live positions while moving
        let mut g = g.clone();
        if let Some(m) = &moved {
            for (id, p) in m {
                if let Some(c) = g.comp_mut(*id) {
                    c.pos = *p;
                }
            }
        }
        let rects: Vec<(i32, Rect)> = g.comps.iter().map(|c| (c.id, self.box_rect(rect, &g, c))).collect();
        let rect_of = |id: i32| rects.iter().find(|r| r.0 == id).map(|r| r.1);

        // wires
        for c in &g.comps {
            let t = g.ctype_of(c);
            let Some(dr) = rect_of(c.id) else { continue };
            for (k, l) in c.inputs.iter().enumerate() {
                let Some(l) = l else { continue };
                let Some(sc) = g.comp(l.src) else { continue };
                let Some(sr) = rect_of(l.src) else { continue };
                let s = g.ctype_of(sc).outputs.get(l.out).map(|o| o.1).or(t.inputs.get(k).map(|i| i.1)).unwrap_or(Sig::Bool);
                let a = port_at(z, sr, l.out, true);
                let b = port_at(z, dr, k, false);
                let live = self.running || self.sim.tick > 0;
                let on = live && s == Sig::Bool && self.sim.out.get(&(l.src, l.out)).is_some_and(|v| v.b);
                let col = if on { Color32::WHITE } else { sig_colour(s) };
                let dx = ((b.x - a.x).abs() * 0.5).max(30.0 * self.zoom);
                painter.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke([a, a + Vec2::new(dx, 0.0), b - Vec2::new(dx, 0.0), b], false, Color32::TRANSPARENT, Stroke::new(if on { 2.6 } else { 1.8 } * self.zoom.sqrt(), col))));
            }
        }
        // boxes
        let font = egui::FontId::proportional((0.115 * u).clamp(7.0, 18.0));
        let small = egui::FontId::proportional((0.095 * u).clamp(6.0, 15.0));
        for c in &g.comps {
            let t = g.ctype_of(c);
            let r = rect_of(c.id).unwrap();
            if !rect.intersects(r) {
                continue;
            }
            let sel = self.selected.contains(&c.id);
            let head_col = if c.bridge { sig_colour(t.inputs.first().or(t.outputs.first()).map(|x| x.1).unwrap_or(Sig::Bool)).gamma_multiply(0.55) } else { theme::SURFACE_HI };
            painter.rect_filled(r, 6.0 * self.zoom, theme::PANEL);
            let hr = Rect::from_min_size(r.min, Vec2::new(r.width(), HEAD * u));
            painter.rect_filled(hr, egui::CornerRadius { nw: (6.0 * self.zoom) as u8, ne: (6.0 * self.zoom) as u8, sw: 0, se: 0 }, head_col);
            painter.rect_stroke(r, 6.0 * self.zoom, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { theme::CYAN } else { theme::LINE }), egui::StrokeKind::Outside);
            let title = if c.bridge { g.nodes.iter().find(|n| n.comp == c.id).map(|n| n.label.clone()).unwrap_or_default() } else { short(t.name) };
            painter.with_clip_rect(hr.shrink(2.0)).text(hr.left_center() + Vec2::new(6.0 * self.zoom, 0.0), egui::Align2::LEFT_CENTER, title, font.clone(), theme::TEXT);
            for (k, (label, s)) in t.inputs.iter().enumerate() {
                let p = port_at(z, r, k, false);
                let connected = c.inputs.get(k).is_some_and(|l| l.is_some());
                if connected {
                    painter.circle_filled(p, 0.045 * u, sig_colour(*s));
                } else {
                    painter.circle_stroke(p, 0.045 * u, Stroke::new(1.5, sig_colour(*s)));
                }
                if self.zoom > 0.55 && !c.bridge {
                    painter.with_clip_rect(r).text(p + Vec2::new(0.07 * u, 0.0), egui::Align2::LEFT_CENTER, short(label), small.clone(), theme::TEXT_DIM);
                }
            }
            for (k, (label, s)) in t.outputs.iter().enumerate() {
                let p = port_at(z, r, k, true);
                painter.circle_filled(p, 0.045 * u, sig_colour(*s));
                if self.zoom > 0.55 && !c.bridge && t.outputs.len() > 1 {
                    painter.with_clip_rect(r).text(p - Vec2::new(0.07 * u, 0.0), egui::Align2::RIGHT_CENTER, short(label), small.clone(), theme::TEXT_DIM);
                }
                if self.sim.tick > 0 {
                    if let Some(v) = self.sim.out.get(&(c.id, k)) {
                        let txt = v.show(*s);
                        let col = if *s == Sig::Bool && v.b { theme::CYAN } else { theme::TEXT };
                        painter.text(p + Vec2::new(0.07 * u, -0.09 * u), egui::Align2::LEFT_BOTTOM, txt, small.clone(), col);
                    }
                }
            }
            if let Some((_, why)) = self.sim.unsupported.iter().find(|(id, _)| *id == c.id) {
                painter.text(r.right_top() + Vec2::new(-4.0, 3.0), egui::Align2::RIGHT_TOP, "!", font.clone(), theme::ORANGE);
                if pointer.is_some_and(|p| r.contains(p)) {
                    egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), ui.scope_id().with("mc_unsup"), egui::PopupAnchor::Pointer).show(|ui| ui.label(why));
                }
            }
        }
        // in-progress wire / band
        match &self.drag {
            Drag::Wire { src, out } => {
                if let (Some(sr), Some(p)) = (rect_of(*src), pointer) {
                    let s = g.comp(*src).map(|c| g.ctype_of(c)).and_then(|t| t.outputs.get(*out).map(|o| o.1)).unwrap_or(Sig::Bool);
                    painter.line_segment([port_at(z, sr, *out, true), p], Stroke::new(2.0, sig_colour(s)));
                }
            }
            Drag::Band { start } => {
                if let Some(p) = pointer {
                    let b = Rect::from_two_pos(*start, p);
                    painter.rect_filled(b, 2.0, theme::CYAN.gamma_multiply(0.08));
                    painter.rect_stroke(b, 2.0, Stroke::new(1.0, theme::CYAN), egui::StrokeKind::Inside);
                }
            }
            _ => {}
        }
        // hover tooltips on ports
        if matches!(self.drag, Drag::None) {
            if let Some(p) = pointer.filter(|p| rect.contains(*p)) {
                let tip = hit_port(p, false).map(|x| (x, false)).or(hit_port(p, true).map(|x| (x, true)));
                if let Some(((id, k), output)) = tip {
                    if let Some(c) = g.comp(id) {
                        let t = g.ctype_of(c);
                        let port = if output { t.outputs.get(k) } else { t.inputs.get(k) };
                        if let Some((l, s)) = port {
                            let hint = if output { "drag to an input to wire" } else if c.inputs.get(k).is_some_and(|x| x.is_some()) { "click to disconnect" } else { "" };
                            painter.text(p + Vec2::new(14.0, -14.0), egui::Align2::LEFT_BOTTOM, format!("{l} ({}) {hint}", s.name()), egui::FontId::proportional(12.0), theme::TEXT);
                        }
                    }
                }
            }
        }
        // drag end
        if resp.drag_stopped() {
            match std::mem::take(&mut self.drag) {
                Drag::Move { .. } => {
                    if let Some(m) = moved {
                        if let Some(gg) = &mut self.graph {
                            let mut any = false;
                            for (id, p) in m {
                                if let Some(c) = gg.comp_mut(id) {
                                    any |= c.pos != p;
                                    c.pos = p;
                                }
                            }
                            self.changed |= any;
                        }
                    }
                }
                Drag::Wire { src, out } => {
                    if let Some((dst, inp)) = pointer.and_then(|p| hit_port(p, false)) {
                        if let Some(gg) = &mut self.graph {
                            match gg.connect(src, out, dst, inp) {
                                Ok(()) => {
                                    self.changed = true;
                                    self.status = "Wired".into();
                                }
                                Err(e) => self.status = e,
                            }
                        }
                    }
                }
                Drag::Band { start } => {
                    if let Some(p) = pointer {
                        let b = Rect::from_two_pos(start, p);
                        if !mods.shift {
                            self.selected.clear();
                        }
                        self.selected.extend(rects.iter().filter(|(_, r)| b.intersects(*r)).map(|(id, _)| *id));
                    }
                }
                _ => {}
            }
        }
        // clicks
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if let Some((dst, inp)) = hit_port(p, false) {
                    if let Some(gg) = &mut self.graph {
                        if gg.comp(dst).and_then(|c| c.inputs.get(inp)).is_some_and(|l| l.is_some()) {
                            gg.disconnect(dst, inp);
                            self.changed = true;
                            self.status = "Wire removed".into();
                        }
                    }
                } else if let Some(id) = hit_box(p) {
                    if mods.shift {
                        if !self.selected.remove(&id) {
                            self.selected.insert(id);
                        }
                    } else {
                        self.selected = [id].into_iter().collect();
                    }
                } else {
                    self.selected.clear();
                }
            }
        }
        // keys
        if resp.hovered() || resp.has_focus() || !ui.ctx().egui_wants_keyboard_input() {
            let (del, all, frame) = ui.input(|i| (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace), i.modifiers.command && i.key_pressed(egui::Key::A), i.key_pressed(egui::Key::F)));
            if !ui.ctx().egui_wants_keyboard_input() {
                if del && !self.selected.is_empty() {
                    let ids: Vec<i32> = self.selected.drain().collect();
                    if let Some(gg) = &mut self.graph {
                        gg.remove(&ids);
                        self.changed = true;
                        self.status = format!("Removed {} component(s)", ids.len());
                    }
                }
                if all {
                    self.selected = g.comps.iter().map(|c| c.id).collect();
                }
                if frame {
                    self.frame_all();
                }
            }
        }
        // simulation
        if self.running {
            if let Some(gg) = &self.graph {
                self.sim.step(gg);
            }
            ui.ctx().request_repaint();
        }
        // overlay: name and hints
        painter.text(rect.left_top() + Vec2::new(14.0, 12.0), egui::Align2::LEFT_TOP, format!("{}  ·  {}×{}  ·  {} components", g.name, g.width, g.length, g.comps.len()), egui::FontId::proportional(14.0), theme::TEXT_DIM);
        painter.text(rect.left_bottom() + Vec2::new(14.0, -12.0), egui::Align2::LEFT_BOTTOM, "Drag outputs to inputs to wire · click an input to unwire · right-drag pans · scroll zooms · Del removes · F frames", egui::FontId::proportional(12.0), theme::TEXT_DIM);
        if g.nested_groups > 0 {
            painter.text(rect.right_top() + Vec2::new(-14.0, 12.0), egui::Align2::RIGHT_TOP, format!("{} nested group(s) preserved but not shown", g.nested_groups), egui::FontId::proportional(13.0), theme::ORANGE);
        }
        self.commit(model);
    }

    // ---- right panel ---------------------------------------------------------------------------------------------

    pub fn inspector_ui(&mut self, ui: &mut egui::Ui, model: &mut Model) {
        let Some(g) = &mut self.graph else {
            ui.label(RichText::new("No microcontroller open.").color(theme::TEXT_DIM));
            return;
        };
        let mut changed = false;
        egui::ScrollArea::vertical().id_salt("mc_insp").show(ui, |ui| {
            theme::section(ui, "Microcontroller");
            egui::Grid::new("mc_props").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Name");
                changed |= ui.text_edit_singleline(&mut g.name).lost_focus();
                ui.end_row();
                ui.label("Description");
                changed |= ui.text_edit_singleline(&mut g.description).lost_focus();
                ui.end_row();
                ui.label("Size");
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut g.width).range(1..=6).suffix(" wide")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut g.length).range(1..=6).suffix(" long")).changed();
                });
                ui.end_row();
            });
            // footprint with IO cells
            ui.add_space(4.0);
            let cell = 26.0;
            let (fr, _) = ui.allocate_exact_size(Vec2::new(g.width as f32 * cell, g.length as f32 * cell), Sense::hover());
            for z in 0..g.length {
                for x in 0..g.width {
                    let r = Rect::from_min_size(fr.min + Vec2::new(x as f32 * cell, (g.length - 1 - z) as f32 * cell), Vec2::splat(cell - 2.0));
                    ui.painter().rect_filled(r, 4.0, theme::SURFACE);
                    if let Some(n) = g.nodes.iter().find(|n| n.pos == [x, z]) {
                        ui.painter().circle_filled(r.center(), 7.0, sig_colour(n.sig));
                        if !n.input {
                            ui.painter().circle_filled(r.center(), 3.0, theme::PANEL);
                        }
                    }
                }
            }
            ui.add_space(6.0);
            theme::section(ui, "Inputs and outputs");
            let mut remove = None;
            let mut rekind = None;
            for n in g.nodes.iter_mut() {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 5.0, sig_colour(n.sig));
                    changed |= ui.add(egui::TextEdit::singleline(&mut n.label).desired_width(110.0)).lost_focus();
                    let mut input = n.input;
                    let mut sig = n.sig;
                    egui::ComboBox::from_id_salt(("io_dir", n.id)).width(48.0).selected_text(if input { "in" } else { "out" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut input, true, "in");
                        ui.selectable_value(&mut input, false, "out");
                    });
                    egui::ComboBox::from_id_salt(("io_sig", n.id)).width(80.0).selected_text(sig.name()).show_ui(ui, |ui| {
                        for s in Sig::ALL {
                            ui.selectable_value(&mut sig, s, s.name());
                        }
                    });
                    if input != n.input || sig != n.sig {
                        rekind = Some((n.id, input, sig));
                    }
                    if icons::button(ui, false, Icon::Trash, "", "Remove this input/output").clicked() {
                        remove = Some(n.comp);
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.label(RichText::new("cell").small().color(theme::TEXT_DIM));
                    changed |= ui.add(egui::DragValue::new(&mut n.pos[0]).range(0..=5).prefix("x ")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut n.pos[1]).range(0..=5).prefix("z ")).changed();
                });
            }
            if let Some((id, i, s)) = rekind {
                g.set_io_kind(id, i, s);
                changed = true;
            }
            if let Some(c) = remove {
                g.remove(&[c]);
                changed = true;
            }
            // selected component
            if self.selected.len() == 1 {
                let id = *self.selected.iter().next().unwrap();
                if let Some(c) = g.comp(id).cloned() {
                    let t = g.ctype_of(&c);
                    ui.separator();
                    theme::section(ui, "Component");
                    ui.label(RichText::new(t.name).strong());
                    ui.label(RichText::new(t.about).small().color(theme::TEXT_DIM));
                    for p in &t.props {
                        let key = (id, p.key);
                        let buf = self.prop_buf.entry(key).or_insert_with(|| c.prop(p));
                        ui.label(RichText::new(p.label).small());
                        let r = match p.kind {
                            PropKind::Code => ui.add(egui::TextEdit::multiline(buf).code_editor().desired_rows(10).desired_width(f32::INFINITY)),
                            _ => ui.add(egui::TextEdit::singleline(buf).desired_width(f32::INFINITY)),
                        };
                        if p.key == "e" && p.store == keel_mc::Store::Attr {
                            let names = ["x", "y", "z", "w", "a", "b", "c", "d"];
                            let vars = names.iter().map(|n| (*n, 0.0)).collect();
                            if let Err(e) = keel_mc::expr::eval(buf, &vars, matches!(c.ty, 46 | 47)) {
                                ui.label(RichText::new(e).small().color(theme::ORANGE));
                            }
                        }
                        if p.kind == PropKind::Number && buf.trim().parse::<f64>().is_err() && !buf.trim().is_empty() {
                            ui.label(RichText::new("not a number").small().color(theme::ORANGE));
                        }
                        let commit = if p.kind == PropKind::Code { r.lost_focus() } else { r.lost_focus() || r.changed() && p.kind == PropKind::Number && buf.trim().parse::<f64>().is_ok() };
                        if commit && c.prop(p) != *buf {
                            let v = buf.clone();
                            if let Some(cm) = g.comp_mut(id) {
                                cm.set_prop(p, &v);
                            }
                            if p.key == "count" {
                                g.drop_invalid_links();
                                if let Some(cm) = g.comp_mut(id) {
                                    let n = 1 + cm.count();
                                    cm.inputs.resize(n, None);
                                }
                            }
                            changed = true;
                        }
                    }
                    if t.props.is_empty() {
                        ui.label(RichText::new("No settings.").small().color(theme::TEXT_DIM));
                    }
                }
            } else if self.selected.len() > 1 {
                ui.separator();
                ui.label(format!("{} components selected", self.selected.len()));
            }
            // simulator
            ui.separator();
            theme::section(ui, "Simulate");
            ui.horizontal(|ui| {
                if ui.button(if self.running { "Pause" } else { "Run" }).clicked() {
                    self.running = !self.running;
                }
                if ui.button("Step").on_hover_text("Advance one tick (1/60 s)").clicked() {
                    self.sim.step(g);
                }
                if ui.button("Reset").clicked() {
                    self.sim.reset();
                    self.running = false;
                }
                ui.label(RichText::new(format!("tick {}", self.sim.tick)).small().color(theme::TEXT_DIM));
            });
            for n in g.nodes.iter().filter(|n| n.input) {
                ui.horizontal(|ui| {
                    let v = self.sim.external.entry(n.id).or_default();
                    match n.sig {
                        Sig::Bool => {
                            let mut b = v.b;
                            if ui.toggle_value(&mut b, &n.label).changed() {
                                *v = Value::bool(b);
                            }
                        }
                        Sig::Num => {
                            let mut x = v.n;
                            ui.label(&n.label);
                            if ui.add(egui::DragValue::new(&mut x).speed(0.05)).changed() {
                                *v = Value::num(x);
                            }
                        }
                        s => {
                            ui.label(RichText::new(format!("{} ({}: not driven)", n.label, s.name())).small().color(theme::TEXT_DIM));
                        }
                    }
                });
            }
            for n in g.nodes.iter().filter(|n| !n.input) {
                let v = self.sim.output(g, n.id);
                ui.label(format!("{}:  {}", n.label, v.show(n.sig)));
            }
            ui.label(RichText::new("Each component adds one tick of delay, as in the game (RE-0013). Lua scripts, video and audio are not simulated.").small().color(theme::TEXT_DIM));
        });
        if changed {
            self.changed = true;
            self.commit(model);
        }
    }
}

fn port_at(zoom: f32, r: Rect, k: usize, output: bool) -> Pos2 {
    let u = 90.0 * zoom;
    Pos2::new(if output { r.right() } else { r.left() }, r.top() + (HEAD + (k as f32 + 0.5) * ROW) * u)
}

/// Shorten long component and port labels for the canvas.
fn short(s: &str) -> String {
    if s.chars().count() > 18 {
        s.chars().take(17).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}
