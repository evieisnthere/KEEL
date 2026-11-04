//! Decks mode: boat type and room programme, deck heights, superstructure tiers, lighting, and the live floor plan
//! (a CAD-style top view per deck, in its own window).
use crate::theme;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Stroke, Vec2};
use keel_geom::hull::{Hull, HullParams};
use keel_geom::hullblocks::HullBlocks;
use keel_layout::electrics::{Circuit, LightingOptions};
use keel_layout::plan::{AccessKind, Cell, Plan, PlanOptions};
use keel_layout::programme::{BoatType, ProgrammeInput, RoomKind, RoomReq};
use keel_layout::superstructure::{Superstructure, Tier};
use keel_layout::{LayoutOptions, Layout};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Interior design settings, saved in the `.keel.json` sidecar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct InteriorDoc {
    pub boat: BoatType,
    pub crew: usize,
    pub passengers: usize,
    /// edited room programme (None = the boat type's default)
    pub rooms: Option<Vec<RoomReq>>,
    /// hull deck floor rows (None = suggested)
    pub hull_floors: Option<Vec<i32>>,
    pub superstructure: Option<Superstructure>,
    pub tiers: usize,
    pub corridor_width: i32,
    /// None = automatic, Some(true) stairs, Some(false) ladders
    pub stairs: Option<bool>,
    /// ship-wide colour temperature (K)
    pub kelvin: f64,
    /// per-room light overrides: room id → (kelvin, brightness)
    pub room_light: BTreeMap<usize, (Option<f64>, Option<f64>)>,
    /// per-light kelvin overrides: "x,y,z" → kelvin
    pub light_kelvin: BTreeMap<String, f64>,
    /// what the last generation added (removed again on regenerate)
    pub generated: Vec<(String, [i32; 3], [i32; 9])>,
    pub generated_links: Vec<(i32, [i32; 3], [i32; 3])>,
}

impl Default for InteriorDoc {
    fn default() -> Self {
        InteriorDoc {
            boat: BoatType::Workboat,
            crew: 0,
            passengers: 0,
            rooms: None,
            hull_floors: None,
            superstructure: None,
            tiers: 2,
            corridor_width: 15,
            stairs: None,
            kelvin: 3000.0,
            room_light: BTreeMap::new(),
            light_kelvin: BTreeMap::new(),
            generated: vec![],
            generated_links: vec![],
        }
    }
}

pub struct Interior {
    pub doc: InteriorDoc,
    hull_key: Option<(HullParams, [i32; 3])>,
    pub hull: Option<HullBlocks>,
    plan_key: String,
    pub plan: Option<Plan>,
    pub circuits: Vec<Circuit>,
    pub window_open: bool,
    pub docked: bool,
    deck_sel: usize,
    room_sel: Option<usize>,
    light_sel: Option<[i32; 3]>,
    zoom: f32,
    pan: Vec2,
    pub status: String,
}

impl Default for Interior {
    fn default() -> Self {
        Interior {
            doc: InteriorDoc::default(),
            hull_key: None,
            hull: None,
            plan_key: String::new(),
            plan: None,
            circuits: vec![],
            window_open: false,
            docked: false,
            deck_sel: 0,
            room_sel: None,
            light_sel: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            status: String::new(),
        }
    }
}

/// A finished interior to write into the vehicle.
pub struct Generated {
    pub layout: Layout,
}

fn tiers_for(b: BoatType) -> usize {
    match b {
        BoatType::CruiseShip => 4,
        BoatType::RoPaxFerry => 3,
        _ => 2,
    }
}

fn room_colour(k: RoomKind) -> Color32 {
    use RoomKind::*;
    let (r, g, b) = match k {
        Bridge => (0x3f, 0xd6, 0xe6),
        EngineRoom | Electrical | Workshop => (0xff, 0x9a, 0x3c),
        CarDeck | Store | Armoury => (0x9a, 0x8a, 0x6a),
        Head | Laundry => (0x7a, 0xb8, 0xff),
        Galley | CrewMess | Restaurant | Bar => (0xff, 0xd0, 0x6a),
        Medical | SurvivorBay => (0xff, 0x6c, 0x8c),
        Saloon | Lounge | Reception | Shop | Gym | PassengerSeating => (0xb0, 0x8c, 0xff),
        _ => (0x6a, 0xd0, 0x8a),
    };
    Color32::from_rgb(r, g, b)
}

fn hex_colour(h: &str) -> Color32 {
    let v = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("80"), 16).unwrap_or(128);
    Color32::from_rgb(v(0), v(2), v(4))
}

impl Interior {
    pub fn with_docked(docked: bool) -> Self {
        Interior { docked, window_open: docked, ..Default::default() }
    }

    fn layout_options(&self) -> Option<LayoutOptions> {
        let h = self.hull.as_ref()?;
        let p = &self.hull_key.as_ref()?.0;
        let mut lighting = LightingOptions { kelvin: self.doc.kelvin, ..Default::default() };
        lighting.room = self.doc.room_light.clone();
        for (k, v) in &self.doc.light_kelvin {
            let c: Vec<i32> = k.split(',').filter_map(|s| s.parse().ok()).collect();
            if c.len() == 3 {
                lighting.light.insert([c[0], c[1], c[2]], *v);
            }
        }
        let sup = self.doc.superstructure.clone().or_else(|| {
            let half = (p.beam / 2.0 / keel_geom::voxel::VOXEL) as i32;
            let zl = (p.length / 2.0 / keel_geom::voxel::VOXEL) as i32;
            let o = h.origin;
            (self.doc.tiers > 0).then(|| Superstructure::default_for(keel_layout::deck_row(h), half, o[2] - zl, o[2] + zl, self.doc.tiers))
        });
        Some(LayoutOptions {
            programme: ProgrammeInput { boat: self.doc.boat, length: p.length, beam: p.beam, crew: self.doc.crew, passengers: self.doc.passengers },
            hull_floors: self.doc.hull_floors.clone(),
            rooms: self.doc.rooms.clone(),
            superstructure: sup,
            plan: PlanOptions { corridor_width: self.doc.corridor_width, stairs: self.doc.stairs },
            finish: Default::default(),
            lighting,
        })
    }

    /// Rebuild the hull blocks when the hull changed, and the plan when anything changed.
    pub fn refresh(&mut self, hull: Option<&Hull>, params: &HullParams, origin: [i32; 3]) {
        let key = (params.clone(), origin);
        if self.hull_key.as_ref() != Some(&key) {
            self.hull = hull.map(|h| keel_geom::hullblocks::build(h, &keel_geom::hullblocks::HullBlockOptions { origin, deck: true }));
            self.hull_key = Some(key);
            self.plan_key.clear();
        }
        let pk = serde_json::to_string(&(&self.doc, &self.hull_key.as_ref().map(|k| k.1))).unwrap_or_default() + &format!("{:?}", self.hull_key.as_ref().map(|k| &k.0));
        if pk == self.plan_key {
            return;
        }
        self.plan_key = pk;
        let (Some(h), Some(opt)) = (self.hull.as_ref(), self.layout_options()) else {
            self.plan = None;
            return;
        };
        let t = std::time::Instant::now();
        let plan = keel_layout::plan_only(h, &opt);
        let (_, circuits) = keel_layout::electrics::wire(&plan, &Default::default(), &opt.lighting);
        self.status = format!(
            "{} decks, {} rooms{} · planned in {:.0} ms",
            plan.decks.len(),
            plan.rooms.len(),
            if plan.unplaced.is_empty() { String::new() } else { format!(", {} did not fit", plan.unplaced.len()) },
            t.elapsed().as_secs_f64() * 1000.0
        );
        self.circuits = circuits;
        self.deck_sel = self.deck_sel.min(plan.decks.len().saturating_sub(1));
        self.plan = Some(plan);
    }

    /// Generate the full interior (blocks, parts, wires).
    pub fn generate(&mut self) -> Option<Generated> {
        let opt = self.layout_options()?;
        let h = self.hull.as_ref()?;
        let layout = keel_layout::generate(h, &opt);
        Some(Generated { layout })
    }

    /// Left panel. Returns a generated interior to apply when the user asks for it.
    pub fn panel_ui(&mut self, ui: &mut egui::Ui) -> Option<Generated> {
        let mut out = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            theme::section(ui, "Boat");
            egui::ComboBox::from_id_salt("boat").selected_text(self.doc.boat.name()).show_ui(ui, |ui| {
                for b in BoatType::ALL {
                    if ui.selectable_label(self.doc.boat == b, b.name()).clicked() && self.doc.boat != b {
                        self.doc.boat = b;
                        self.doc.rooms = None;
                        self.doc.tiers = tiers_for(b);
                        self.doc.superstructure = None;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Crew");
                ui.add(egui::DragValue::new(&mut self.doc.crew).range(0..=2000)).on_hover_text("0 = estimate from size");
                ui.label("Passengers");
                ui.add(egui::DragValue::new(&mut self.doc.passengers).range(0..=5000)).on_hover_text("0 = estimate from size");
            });

            theme::section(ui, "Rooms");
            let mut rooms = self.layout_options().map(|o| o.room_list()).unwrap_or_default();
            let mut changed = false;
            egui::Grid::new("rooms").striped(true).num_columns(3).show(ui, |ui| {
                ui.label(RichText::new("Room").color(theme::TEXT_DIM));
                ui.label(RichText::new("Count").color(theme::TEXT_DIM));
                ui.label(RichText::new("Area m²").color(theme::TEXT_DIM));
                ui.end_row();
                for q in rooms.iter_mut() {
                    let placed = self.plan.as_ref().map(|p| p.rooms.iter().filter(|r| r.kind == q.kind).count()).unwrap_or(0);
                    let lbl = RichText::new(q.kind.name()).color(if placed < q.count { theme::ORANGE } else { theme::TEXT });
                    ui.label(lbl).on_hover_text(format!("{placed} of {} placed", q.count));
                    changed |= ui.add(egui::DragValue::new(&mut q.count).range(0..=999)).changed();
                    changed |= ui.add(egui::DragValue::new(&mut q.area).range(1.0..=2000.0).speed(0.2).max_decimals(1)).changed();
                    ui.end_row();
                }
            });
            if changed {
                self.doc.rooms = Some(rooms);
            }
            if self.doc.rooms.is_some() && ui.button("Reset rooms to the boat type's defaults").clicked() {
                self.doc.rooms = None;
            }

            theme::section(ui, "Hull decks");
            let suggested = self.hull.as_ref().map(|h| keel_layout::HullSpace::new(h).suggest_floors()).unwrap_or_default();
            let deck_y = self.hull.as_ref().map(keel_layout::deck_row).unwrap_or(0);
            let base = self.hull_key.as_ref().map(|k| k.1[1]).unwrap_or(0);
            let mut floors = self.doc.hull_floors.clone().unwrap_or(suggested.clone());
            let mut fchanged = false;
            let mut remove = None;
            ui.label(RichText::new(format!("Weather deck at {:.2} m (row {})", (deck_y + 1 - base) as f64 * 0.25, deck_y)).small().color(theme::TEXT_DIM));
            for (i, f) in floors.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Deck {}", i + 1));
                    fchanged |= ui.add(egui::DragValue::new(f).range(base - 4..=deck_y - 2).prefix("row ")).changed();
                    ui.label(RichText::new(format!("floor {:.2} m", (*f + 1 - base) as f64 * 0.25)).small().color(theme::TEXT_DIM));
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                floors.remove(i);
                fchanged = true;
            }
            ui.horizontal(|ui| {
                if ui.button("Add deck").clicked() {
                    let low = floors.iter().copied().min().unwrap_or(deck_y);
                    floors.push(low - 9);
                    fchanged = true;
                }
                if ui.button("Suggest").on_hover_text("Every 9 rows (8 rows headroom) down from the weather deck").clicked() {
                    self.doc.hull_floors = None;
                }
            });
            if fchanged {
                self.doc.hull_floors = Some(floors);
            }

            theme::section(ui, "Superstructure");
            ui.horizontal(|ui| {
                ui.label("Tiers");
                if ui.add(egui::DragValue::new(&mut self.doc.tiers).range(0..=8)).changed() {
                    self.doc.superstructure = None;
                }
                if self.doc.superstructure.is_none() && self.doc.tiers > 0 && ui.button("Customise").clicked() {
                    self.doc.superstructure = self.layout_options().and_then(|o| o.superstructure);
                }
                if self.doc.superstructure.is_some() && ui.button("Reset").clicked() {
                    self.doc.superstructure = None;
                }
            });
            if let Some(s) = self.doc.superstructure.as_mut() {
                let mut del = None;
                for (k, t) in s.tiers.iter_mut().enumerate() {
                    egui::CollapsingHeader::new(format!("Tier {}", k + 1)).default_open(k == 0).show(ui, |ui| tier_ui(ui, t));
                    if ui.small_button(format!("Remove tier {}", k + 1)).clicked() {
                        del = Some(k);
                    }
                }
                if let Some(k) = del {
                    s.tiers.remove(k);
                }
                if ui.button("Add tier on top").clicked() {
                    if let Some(t) = s.tiers.last().cloned() {
                        s.tiers.push(Tier { half_width: (t.half_width - 2).max(3), aft: t.aft + (t.fwd - t.aft) / 4, ..t });
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Colour");
                    ui.text_edit_singleline(&mut s.colour);
                });
            }

            theme::section(ui, "Layout");
            ui.horizontal(|ui| {
                ui.label("Corridor from width");
                ui.add(egui::DragValue::new(&mut self.doc.corridor_width).range(7..=99).suffix(" vox"));
            });
            ui.horizontal(|ui| {
                ui.label("Access");
                for (v, n) in [(None, "Auto"), (Some(true), "Stairs"), (Some(false), "Ladders")] {
                    if ui.selectable_label(self.doc.stairs == v, n).clicked() {
                        self.doc.stairs = v;
                    }
                }
            });

            theme::section(ui, "Lighting");
            ui.horizontal(|ui| {
                ui.label("Ship-wide");
                ui.add(egui::Slider::new(&mut self.doc.kelvin, 1800.0..=6500.0).suffix(" K").step_by(50.0));
                let c = hex_colour(&keel_layout::light::light_colour(self.doc.kelvin, 1.0));
                let (r, p) = ui.allocate_painter(Vec2::new(18.0, 18.0), egui::Sense::hover());
                p.rect_filled(r.rect, 3.0, c);
            });
            ui.label(RichText::new("Rooms keep their own brightness (cabins dim, galley and engine room bright). Override a room or a single light in the floor plan window.").small().color(theme::TEXT_DIM));

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(if self.window_open { "Close floor plan" } else { "Floor plan window" }).clicked() {
                    self.window_open = !self.window_open;
                }
                if ui.button(RichText::new("Generate interior").strong()).on_hover_text("Floors, walls, doors, stairs, furniture, lights, switches and wiring, in one undo step").clicked() {
                    out = self.generate();
                }
            });
            if !self.status.is_empty() {
                ui.label(RichText::new(&self.status).small().color(theme::CYAN));
            }
            if let Some(p) = &self.plan {
                if !p.unplaced.is_empty() {
                    let mut names: BTreeMap<&str, usize> = BTreeMap::new();
                    for k in &p.unplaced {
                        *names.entry(k.name()).or_default() += 1;
                    }
                    let s: Vec<String> = names.iter().map(|(n, c)| format!("{c}× {n}")).collect();
                    ui.label(RichText::new(format!("Did not fit: {}", s.join(", "))).small().color(theme::ORANGE));
                }
            }
        });
        out
    }

    /// The floor plan in its own native window (or docked when `docked`).
    pub fn plan_window(&mut self, ctx: &egui::Context) {
        if !self.window_open || self.docked {
            return;
        }
        let id = egui::ViewportId::from_hash_of("keel_floor_plan");
        let builder = egui::ViewportBuilder::default().with_title("KEEL — Floor plan").with_inner_size([1100.0, 700.0]);
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                self.window_open = false;
            }
            self.plan_ui(ui);
        });
    }

    /// Floor plan contents: deck list and room inspector on the left, the drawing in the middle.
    pub fn plan_ui(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("plan_side").frame(theme::side_frame()).default_size(260.0).show(ui, |ui| self.plan_side(ui));
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::CANVAS)).show(ui, |ui| self.plan_canvas(ui));
    }

    fn plan_side(&mut self, ui: &mut egui::Ui) {
        let Some(plan) = &self.plan else {
            ui.label("Design a hull first (Hull mode).");
            return;
        };
        theme::section(ui, "Decks");
        for (i, d) in plan.decks.iter().enumerate().rev() {
            let n = plan.rooms.iter().filter(|r| r.deck == i).count();
            if ui.selectable_label(self.deck_sel == i, format!("{} · {} rooms · {:.0} m² · headroom {:.2} m", d.name(), n, d.area_m2(), d.clear as f64 * 0.25)).clicked() {
                self.deck_sel = i;
                self.room_sel = None;
            }
        }
        theme::section(ui, "Room");
        let Some(rid) = self.room_sel else {
            ui.label(RichText::new("Click a room in the plan.").color(theme::TEXT_DIM));
            return;
        };
        let Some(room) = plan.rooms.iter().find(|r| r.id == rid) else { return };
        ui.label(RichText::new(room.kind.name()).strong());
        let (x0, x1, z0, z1) = room.bbox();
        ui.label(format!("{:.1} m² · {:.2} × {:.2} m", room.area_m2(), (x1 - x0 + 1) as f64 * 0.25, (z1 - z0 + 1) as f64 * 0.25));
        ui.label(format!("{} door(s)", room.doors.len()));
        let (dk, db) = room.kind.light();
        let entry = self.doc.room_light.get(&rid).copied().unwrap_or((None, None));
        let mut k = entry.0.or(dk).unwrap_or(self.doc.kelvin);
        let mut br = entry.1.unwrap_or(db);
        ui.label(RichText::new("Lighting").color(theme::TEXT_DIM));
        let kc = ui.add(egui::Slider::new(&mut k, 1800.0..=6500.0).suffix(" K").step_by(50.0)).changed();
        let bc = ui.add(egui::Slider::new(&mut br, 0.05..=1.0).text("brightness")).changed();
        if kc || bc {
            let e = self.doc.room_light.entry(rid).or_insert((None, None));
            if kc {
                e.0 = Some(k);
            }
            if bc {
                e.1 = Some(br);
            }
        }
        if self.doc.room_light.contains_key(&rid) && ui.button("Use defaults").clicked() {
            self.doc.room_light.remove(&rid);
        }
        let c = hex_colour(&keel_layout::light::light_colour(k, br));
        let (r, p) = ui.allocate_painter(Vec2::new(60.0, 14.0), egui::Sense::hover());
        p.rect_filled(r.rect, 3.0, c);
        if let Some(l) = self.light_sel {
            theme::section(ui, "Light");
            let key = format!("{},{},{}", l[0], l[1], l[2]);
            let mut lk = self.doc.light_kelvin.get(&key).copied().unwrap_or(k);
            if ui.add(egui::Slider::new(&mut lk, 1800.0..=6500.0).suffix(" K").step_by(50.0)).changed() {
                self.doc.light_kelvin.insert(key.clone(), lk);
            }
            if self.doc.light_kelvin.contains_key(&key) && ui.button("Use the room's colour").clicked() {
                self.doc.light_kelvin.remove(&key);
            }
        }
    }

    pub fn plan_canvas(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, theme::CANVAS);
        let Some(plan) = &self.plan else { return };
        let Some(deck) = plan.decks.get(self.deck_sel) else { return };
        let grid = &plan.grids[self.deck_sel];
        let Some((z0, z1)) = deck.z_range() else { return };
        let xm = deck.cells.iter().map(|c| c.0.abs()).max().unwrap_or(1);
        // fit: bow to the right (z → screen x), starboard down (x → screen y)
        let fit = ((rect.width() - 60.0) / (z1 - z0 + 3) as f32).min((rect.height() - 80.0) / (2 * xm + 3) as f32).max(1.0);
        if resp.dragged() {
            self.pan += resp.drag_delta();
        }
        if resp.hovered() {
            let s = ui.input(|i| i.smooth_scroll_delta.y);
            if s != 0.0 {
                self.zoom = (self.zoom * (1.0 + s * 0.002)).clamp(0.3, 12.0);
            }
        }
        let s = fit * self.zoom;
        let c = rect.center() + self.pan;
        let zc = (z0 + z1) as f32 / 2.0;
        let to = |x: f32, z: f32| Pos2::new(c.x + (z - zc) * s, c.y + x * s);
        let cell_rect = |x: i32, z: i32| Rect::from_min_max(to(x as f32 - 0.5, z as f32 - 0.5), to(x as f32 + 0.5, z as f32 + 0.5));
        let from = |p: Pos2| -> (i32, i32) { (((p.y - c.y) / s).round() as i32, ((p.x - c.x) / s + zc).round() as i32) };
        // 1 m grid
        let gm = 4;
        for z in (z0 - 2..=z1 + 2).filter(|z| z.rem_euclid(gm) == 0) {
            painter.line_segment([to(-(xm as f32) - 1.5, z as f32 - 0.5), to(xm as f32 + 1.5, z as f32 - 0.5)], Stroke::new(1.0, theme::GRID));
        }
        for x in (-xm - 1..=xm + 1).filter(|x| x.rem_euclid(gm) == 0) {
            painter.line_segment([to(x as f32 - 0.5, z0 as f32 - 2.0), to(x as f32 - 0.5, z1 as f32 + 2.0)], Stroke::new(1.0, theme::GRID));
        }
        // cells
        for &(x, z) in &deck.cells {
            let col = match grid.get(&(x, z)) {
                Some(Cell::Room(id)) => {
                    let k = plan.rooms.iter().find(|r| r.id == *id).map(|r| r.kind).unwrap_or(RoomKind::Store);
                    let base = room_colour(k);
                    let a = if self.room_sel == Some(*id) { 120 } else { 60 };
                    Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), a)
                }
                Some(Cell::Corridor) => Color32::from_rgb(0x2a, 0x2f, 0x58),
                Some(Cell::Core) => Color32::from_rgba_unmultiplied(0xff, 0x9a, 0x3c, 90),
                Some(Cell::Wall) => Color32::from_rgb(0xd8, 0xdc, 0xf0),
                Some(Cell::Void) => {
                    // hatched: unused space
                    if (x + z).rem_euclid(3) == 0 { Color32::from_rgb(0x2c, 0x30, 0x50) } else { Color32::from_rgb(0x16, 0x19, 0x34) }
                }
                None => Color32::from_rgb(0x1c, 0x20, 0x40),
            };
            painter.rect_filled(cell_rect(x, z), 0.0, col);
        }
        // hull / deckhouse outline
        let outline = Stroke::new(2.0, theme::CYAN);
        for &(x, z) in &deck.cells {
            for (dx, dz, a, b) in [(1, 0, (0.5, -0.5), (0.5, 0.5)), (-1, 0, (-0.5, -0.5), (-0.5, 0.5)), (0, 1, (-0.5, 0.5), (0.5, 0.5)), (0, -1, (-0.5, -0.5), (0.5, -0.5))] {
                if !deck.cells.contains(&(x + dx, z + dz)) {
                    painter.line_segment([to(x as f32 + a.0, z as f32 + a.1), to(x as f32 + b.0, z as f32 + b.1)], outline);
                }
            }
        }
        // doors: an opening with a swing arc
        for r in plan.rooms.iter().filter(|r| r.deck == self.deck_sel) {
            for d in &r.doors {
                for &(x, z) in &d.cells {
                    painter.rect_filled(cell_rect(x, z), 0.0, Color32::from_rgb(0x2a, 0x2f, 0x58));
                }
                let (a, b) = (d.cells[0], d.cells[d.cells.len() - 1]);
                painter.line_segment([to(a.0 as f32, a.1 as f32), to(b.0 as f32, b.1 as f32)], Stroke::new(2.0, Color32::from_rgb(0x6a, 0xe0, 0x8a)));
            }
        }
        // access core
        if let (Some((x0, x1, cz0, cz1)), true) = (plan.core, plan.core_decks.contains(&self.deck_sel)) {
            let r = Rect::from_min_max(to(x0 as f32 - 0.5, cz0 as f32 - 0.5), to(x1 as f32 + 0.5, cz1 as f32 + 0.5));
            let label = if plan.access == AccessKind::Stairs { "STAIRS" } else { "LADDER" };
            if plan.access == AccessKind::Stairs {
                let mut z = cz0;
                while z <= cz1 {
                    painter.line_segment([to(x0 as f32 - 0.5, z as f32), to(x1 as f32 + 0.5, z as f32)], Stroke::new(1.0, theme::ORANGE));
                    z += 2;
                }
            }
            painter.text(r.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional((s * 0.9).clamp(8.0, 13.0)), theme::ORANGE);
        }
        // lights and switches on this deck
        let ceiling = deck.floor_y + deck.clear;
        for c in &self.circuits {
            for l in c.lights.iter().filter(|l| l[1] == ceiling && deck.cells.contains(&(l[0], l[2]))) {
                let col = hex_colour(&keel_layout::light::light_colour(self.doc.light_kelvin.get(&format!("{},{},{}", l[0], l[1], l[2])).copied().unwrap_or(c.kelvin), c.brightness));
                let p = to(l[0] as f32, l[2] as f32);
                painter.circle_filled(p, (s * 0.4).clamp(2.5, 7.0), col);
                let sel = self.light_sel == Some(*l);
                painter.circle_stroke(p, (s * 0.4).clamp(2.5, 7.0), Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { theme::CYAN } else { Color32::BLACK }));
            }
            if c.button[1] == deck.floor_y + 4 && deck.cells.contains(&(c.button[0], c.button[2])) {
                painter.rect_filled(Rect::from_center_size(to(c.button[0] as f32, c.button[2] as f32), Vec2::splat((s * 0.6).clamp(3.0, 8.0))), 1.0, theme::RED);
            }
        }
        // room labels
        for r in plan.rooms.iter().filter(|r| r.deck == self.deck_sel) {
            let (x0, x1, z0r, z1r) = r.bbox();
            let p = to((x0 + x1) as f32 / 2.0, (z0r + z1r) as f32 / 2.0);
            let fs = (s * 1.1).clamp(8.0, 14.0);
            painter.text(p, egui::Align2::CENTER_BOTTOM, r.kind.name(), egui::FontId::proportional(fs), theme::TEXT);
            painter.text(p, egui::Align2::CENTER_TOP, format!("{:.1} m²", r.area_m2()), egui::FontId::proportional(fs * 0.85), theme::TEXT_DIM);
        }
        // header: deck name and dimensions; bow marker
        painter.text(rect.left_top() + Vec2::new(12.0, 10.0), egui::Align2::LEFT_TOP, format!("{} · floor {:.2} m · headroom {:.2} m · {:.1} × {:.1} m", deck.name(), (deck.floor_y + 1) as f64 * 0.25, deck.clear as f64 * 0.25, (z1 - z0 + 1) as f64 * 0.25, (2 * xm + 1) as f64 * 0.25), egui::FontId::proportional(13.0), theme::TEXT);
        painter.text(to(0.0, z1 as f32 + 2.5), egui::Align2::LEFT_CENTER, "BOW ▶", egui::FontId::proportional(12.0), theme::TEXT_DIM);
        painter.text(to(xm as f32 + 2.0, zc), egui::Align2::CENTER_TOP, "starboard", egui::FontId::proportional(11.0), theme::TEXT_DIM);
        // interaction
        if let Some(pos) = resp.hover_pos() {
            let (x, z) = from(pos);
            if let Some(Cell::Room(id)) = grid.get(&(x, z)) {
                if let Some(r) = plan.rooms.iter().find(|r| r.id == *id) {
                    resp.clone().on_hover_text(format!("{} · {:.1} m²", r.kind.name(), r.area_m2()));
                }
            }
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let (x, z) = from(pos);
                self.light_sel = self.circuits.iter().flat_map(|c| c.lights.iter()).find(|l| l[0] == x && l[2] == z && l[1] == ceiling).copied();
                self.room_sel = match grid.get(&(x, z)) {
                    Some(Cell::Room(id)) => Some(*id),
                    _ => None,
                };
            }
        }
    }
}

fn tier_ui(ui: &mut egui::Ui, t: &mut Tier) {
    egui::Grid::new(ui.next_auto_id()).num_columns(2).show(ui, |ui| {
        let row = |ui: &mut egui::Ui, name: &str, v: &mut i32, lo: i32, hi: i32, tip: &str| {
            ui.label(name).on_hover_text(tip);
            ui.add(egui::DragValue::new(v).range(lo..=hi));
            ui.end_row();
        };
        row(ui, "Aft (z)", &mut t.aft, -2000, 2000, "aft end, voxels");
        row(ui, "Forward (z)", &mut t.fwd, -2000, 2000, "forward end, voxels");
        row(ui, "Half width", &mut t.half_width, 2, 400, "voxels each side of the centre line");
        row(ui, "Corner radius", &mut t.corner, 0, 40, "rounded front corners, voxels");
        row(ui, "Height", &mut t.height, 8, 20, "floor to floor, rows (headroom = height − 1; doors need 8)");
        row(ui, "Tumblehome", &mut t.tumblehome, 0, 16, "sides lean in 1 voxel every N rows (0 = vertical)");
        row(ui, "Rake", &mut t.rake, 0, 16, "front leans back 1 voxel every N rows (0 = vertical)");
        ui.label("Windows");
        ui.checkbox(&mut t.windows, "");
        ui.end_row();
    });
}
