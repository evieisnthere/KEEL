//! KEEL desktop application (Phase 1: native vehicle foundation).
//!
//! Usage: `keel [vehicle.xml] [--defs <rom/data/definitions>]`. The definitions folder is remembered in the settings file.
mod design;
mod settings;
mod theme;
mod viewport;

use eframe::egui::{self, Color32, RichText};
use keel_model::diagnostics::{self, Diagnostic, Severity};
use keel_model::geometry::{self, Face};
use keel_model::{Catalogue, CompId, Model};
use settings::Settings;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    Build,
    Hull,
    Decks,
}

#[derive(PartialEq, Clone, Copy)]
enum Tool {
    Select,
    Place,
}

struct KeelApp {
    settings: Settings,
    catalogue: Catalogue,
    model: Model,
    path: Option<PathBuf>,
    faces: Vec<Face>,
    faces_gen: u64,
    diags: Vec<Diagnostic>,
    selected: HashSet<CompId>,
    cam: viewport::Camera,
    tool: Tool,
    place_def: String,
    place_body: usize,
    search: String,
    path_input: String,
    defs_input: String,
    dialog: Option<Dialog>,
    status: String,
    scale_input: [i32; 3],
    frame_next: bool,
    renderer: std::sync::Arc<std::sync::Mutex<viewport::Renderer>>,
    meshes: geometry::MeshLibrary,
    rom_override: Option<PathBuf>,
    clean: bool,
    zoom: f32,
    look: Option<[f32; 3]>,
    mode: Mode,
    design: design::Design,
    frame_next_hull: bool,
    bake_on_start: bool,
}

#[derive(PartialEq, Clone, Copy)]
enum Dialog {
    Open,
    SaveAs,
    Settings,
    Scale,
}

impl KeelApp {
    fn new(args: Args) -> Self {
        let mut settings = Settings::load();
        if let Some(d) = args.defs {
            settings.definitions_dir = Some(d);
        }
        let mut app = KeelApp {
            catalogue: Catalogue::fallback(),
            model: Model::new_empty(),
            path: None,
            faces: vec![],
            faces_gen: u64::MAX,
            diags: vec![],
            selected: HashSet::new(),
            cam: viewport::Camera::default(),
            tool: Tool::Select,
            place_def: "01_block".into(),
            place_body: 0,
            search: String::new(),
            path_input: String::new(),
            defs_input: settings.definitions_dir.clone().map(|p| p.display().to_string()).unwrap_or_default(),
            dialog: None,
            status: "Ready".into(),
            scale_input: [1, 1, 1],
            frame_next: true,
            renderer: Default::default(),
            meshes: geometry::MeshLibrary::default(),
            rom_override: args.rom.clone(),
            clean: args.clean,
            zoom: args.zoom,
            look: args.look,
            mode: match args.mode.as_deref() { Some("hull") => Mode::Hull, Some("decks") => Mode::Decks, _ => Mode::Build },
            design: design::Design::default(),
            frame_next_hull: true,
            bake_on_start: args.bake,
            settings,
        };
        if let Some([y, p]) = args.view {
            app.cam.yaw = y;
            app.cam.pitch = p;
        }
        app.load_catalogue();
        if let Some(p) = args.vehicle {
            app.open(p);
        }
        app
    }

    fn load_catalogue(&mut self) {
        match &self.settings.definitions_dir {
            Some(d) => match Catalogue::load_dir(d) {
                Ok(c) => {
                    self.status = format!("Catalogue: {} definitions from {}", c.defs.len(), d.display());
                    self.catalogue = c;
                }
                Err(e) => {
                    self.status = format!("Could not read definitions folder {}: {e}", d.display());
                    self.catalogue = Catalogue::fallback();
                }
            },
            None => {
                self.status = "No definitions folder set (Settings): showing blocks only".into();
                self.catalogue = Catalogue::fallback();
            }
        }
        let rom = self.rom_override.clone().or_else(|| self.settings.definitions_dir.as_deref().and_then(geometry::MeshLibrary::rom_from_definitions));
        self.meshes = geometry::MeshLibrary::new(rom);
        self.faces_gen = u64::MAX;
    }

    fn open(&mut self, p: PathBuf) {
        match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| Model::parse(&String::from_utf8_lossy(&b))) {
            Ok(m) => {
                self.model = m;
                self.selected.clear();
                self.status = format!("Opened {}", p.display());
                self.settings.push_recent(&p);
                self.design.doc = design::Design::load_for(&p);
                self.path = Some(p);
                self.faces_gen = u64::MAX;
                self.frame_next = true;
            }
            Err(e) => self.status = format!("Open failed: {e}"),
        }
    }

    fn save(&mut self, p: PathBuf) {
        match std::fs::write(&p, self.model.to_xml()) {
            Ok(()) => {
                self.design.save_for(&p);
                self.model.dirty = false;
                self.status = format!("Saved {}", p.display());
                self.settings.push_recent(&p);
                self.path = Some(p);
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    fn refresh(&mut self) {
        if self.faces_gen != self.model.generation {
            self.faces = geometry::build_faces_with(&self.model, &self.catalogue, &mut self.meshes);
            self.diags = diagnostics::check(&self.model, &self.catalogue);
            self.faces_gen = self.model.generation;
            let n = self.model.components().len();
            self.selected.retain(|id| id.comp < n + 1_000_000 && self.model.component_element(*id).is_some());
            if self.frame_next {
                self.cam.frame(&self.faces);
                self.cam.dist /= self.zoom;
                if let Some(l) = self.look {
                    self.cam.target = l;
                }
                self.frame_next = false;
            }
        }
        let sel_key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            self.sel().hash(&mut h);
            h.finish()
        };
        let show_hull = self.mode == Mode::Hull && self.design.show_preview;
        if self.mode == Mode::Hull {
            self.design.refresh();
            if self.faces.is_empty() && self.frame_next_hull {
                self.cam.frame(&self.design.preview);
                self.cam.dist /= self.zoom;
                self.frame_next_hull = false;
            }
            if self.bake_on_start {
                self.bake_on_start = false;
                self.design.refresh();
                if let Some(b) = self.design.bake_now() {
                    self.apply_bake(b);
                    self.frame_next = true;
                    self.faces_gen = u64::MAX;
                }
            }
        }
        let key = (self.faces_gen, sel_key ^ if show_hull { self.design.generation.wrapping_mul(0x9e37_79b9) } else { 0 });
        let mut r = self.renderer.lock().unwrap();
        if r.uploaded_key != key {
            if show_hull {
                let mut all = self.faces.clone();
                all.extend(self.design.preview.iter().cloned());
                r.set_data(viewport::vertex_data(&all, &self.selected), key);
            } else {
                r.set_data(viewport::vertex_data(&self.faces, &self.selected), key);
            }
        }
    }

    fn sel(&self) -> Vec<CompId> {
        let mut v: Vec<CompId> = self.selected.iter().copied().collect();
        v.sort();
        v
    }

    fn pivot(&self) -> [i32; 3] {
        let comps = self.model.components();
        let sel: Vec<_> = comps.iter().filter(|(id, _)| self.selected.contains(id)).collect();
        if sel.is_empty() {
            return [0; 3];
        }
        let n = sel.len() as f32;
        [0, 1, 2].map(|i| (sel.iter().map(|(_, c)| c.voxel[i] as f32).sum::<f32>() / n).round() as i32)
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let ids = self.sel();
        let (undo, redo, del, dup, rot, esc, mv, frame, save, scale) = ctx.input(|i| {
            let ctrl = i.modifiers.command;
            let mut mv = [0; 3];
            let step = if i.modifiers.shift { 4 } else { 1 };
            if i.key_pressed(egui::Key::ArrowLeft) {
                mv[0] -= step;
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                mv[0] += step;
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                mv[2] += step;
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                mv[2] -= step;
            }
            if i.key_pressed(egui::Key::PageUp) {
                mv[1] += step;
            }
            if i.key_pressed(egui::Key::PageDown) {
                mv[1] -= step;
            }
            (
                ctrl && i.key_pressed(egui::Key::Z) && !i.modifiers.shift,
                ctrl && (i.key_pressed(egui::Key::Y) || (i.key_pressed(egui::Key::Z) && i.modifiers.shift)),
                i.key_pressed(egui::Key::Delete),
                ctrl && i.key_pressed(egui::Key::D),
                if i.key_pressed(egui::Key::R) && !ctrl { Some(if i.modifiers.shift { 0 } else if i.modifiers.alt { 2 } else { 1 }) } else { None },
                i.key_pressed(egui::Key::Escape),
                mv,
                i.key_pressed(egui::Key::F),
                ctrl && i.key_pressed(egui::Key::S),
                i.key_pressed(egui::Key::S) && !ctrl,
            )
        });
        if undo && self.model.undo() {
            self.status = "Undo".into();
        }
        if redo && self.model.redo() {
            self.status = "Redo".into();
        }
        if del && !ids.is_empty() {
            let n = self.model.delete(&ids);
            self.selected.clear();
            self.status = format!("Deleted {n} component(s)");
        }
        if dup && !ids.is_empty() {
            let new = self.model.duplicate(&ids, [0, 1, 0]);
            self.selected = new.into_iter().collect();
            self.status = "Duplicated (offset +1 Y)".into();
        }
        if let Some(axis) = rot {
            if !ids.is_empty() {
                let p = self.pivot();
                self.model.rotate_quarter(&ids, axis, p);
                self.status = format!("Rotated 90° about {}", ["X", "Y", "Z"][axis]);
            }
        }
        if mv != [0, 0, 0] && !ids.is_empty() {
            self.model.translate(&ids, mv);
        }
        if esc {
            self.selected.clear();
            self.tool = Tool::Select;
        }
        if frame {
            self.cam.frame(&self.faces);
        }
        if save {
            match self.path.clone() {
                Some(p) => self.save(p),
                None => self.dialog = Some(Dialog::SaveAs),
            }
        }
        if scale && !ids.is_empty() {
            self.open_scale_dialog();
        }
    }

    fn open_scale_dialog(&mut self) {
        if let Some((_, c)) = self.model.components().into_iter().find(|(id, _)| self.selected.contains(id)) {
            self.scale_input = Model::scale_of(&c.rotation);
        }
        self.dialog = Some(Dialog::Scale);
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.model = Model::new_empty();
                    self.path = None;
                    self.selected.clear();
                    self.faces_gen = u64::MAX;
                    ui.close();
                }
                if ui.button("Open…  (Ctrl+O)").clicked() {
                    self.open_dialog();
                    ui.close();
                }
                if ui.button("Open by path…").clicked() {
                    self.path_input = self.path.clone().map(|p| p.display().to_string()).unwrap_or_default();
                    self.dialog = Some(Dialog::Open);
                    ui.close();
                }
                let recent = self.settings.recent.clone();
                if !recent.is_empty() {
                    ui.menu_button("Open recent", |ui| {
                        for p in recent {
                            if ui.button(p.display().to_string()).clicked() {
                                self.open(p);
                                ui.close();
                            }
                        }
                    });
                }
                if ui.add_enabled(self.path.is_some(), egui::Button::new("Save  (Ctrl+S)")).clicked() {
                    if let Some(p) = self.path.clone() {
                        self.save(p);
                    }
                    ui.close();
                }
                if ui.button("Save as / Export vehicle XML…").clicked() {
                    self.save_as_dialog();
                    ui.close();
                }
                ui.separator();
                if ui.button("Settings…").clicked() {
                    self.dialog = Some(Dialog::Settings);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(self.model.can_undo(), egui::Button::new("Undo  (Ctrl+Z)")).clicked() {
                    self.model.undo();
                    ui.close();
                }
                if ui.add_enabled(self.model.can_redo(), egui::Button::new("Redo  (Ctrl+Y)")).clicked() {
                    self.model.redo();
                    ui.close();
                }
                ui.separator();
                let has = !self.selected.is_empty();
                if ui.add_enabled(has, egui::Button::new("Scale…  (S)")).clicked() {
                    self.open_scale_dialog();
                    ui.close();
                }
                if ui.add_enabled(has, egui::Button::new("Delete  (Del)")).clicked() {
                    let ids = self.sel();
                    self.model.delete(&ids);
                    self.selected.clear();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Frame all  (F)").clicked() {
                    self.cam.frame(&self.faces);
                    ui.close();
                }
            });
            ui.menu_button("Design", |ui| {
                if ui.button("Hull designer").clicked() {
                    self.mode = Mode::Hull;
                    ui.close();
                }
                if ui.button("Deck & floor plan").clicked() {
                    self.mode = Mode::Decks;
                    ui.close();
                }
            });
            ui.menu_button("Help", |ui| {
                ui.label("LMB orbit / select · RMB pan · wheel zoom");
                ui.label("R rotate · S scale · arrows / PgUp / PgDn move");
                ui.label("Del delete · Ctrl+D duplicate · Ctrl+Z / Y undo / redo · F frame");
            });
        });
    }

    fn open_dialog(&mut self) {
        let mut d = rfd::FileDialog::new().add_filter("Stormworks vehicle", &["xml"]);
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            d = d.set_directory(dir);
        }
        match d.pick_file() {
            Some(p) => self.open(p),
            None => self.status = "Open cancelled (use File → Open by path if no dialog appeared)".into(),
        }
    }

    fn save_as_dialog(&mut self) {
        let mut d = rfd::FileDialog::new().add_filter("Stormworks vehicle", &["xml"]).set_file_name("vehicle.xml");
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            d = d.set_directory(dir);
        }
        match d.save_file() {
            Some(p) => self.save(p),
            None => {
                self.path_input = self.path.clone().map(|p| p.display().to_string()).unwrap_or_default();
                self.dialog = Some(Dialog::SaveAs);
            }
        }
    }

    /// Title row (document-editor style): product mark, document name, save state.
    fn title_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("KEEL").strong().size(20.0).color(theme::CYAN));
            ui.add_space(8.0);
            let name = self.path.as_ref().map(|p| {
                let dir = p.parent().and_then(|d| d.file_name()).map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
                if p.file_name().is_some_and(|f| f == "vehicle.xml") && !dir.is_empty() { dir } else { p.file_name().unwrap_or_default().to_string_lossy().to_string() }
            });
            ui.label(RichText::new(name.unwrap_or_else(|| "Untitled vessel".into())).size(17.0));
            ui.add_space(10.0);
            let (txt, col) = if self.model.dirty { ("Unsaved changes", theme::ORANGE) } else if self.path.is_some() { ("All changes saved", theme::TEXT_DIM) } else { ("Not saved yet", theme::TEXT_DIM) };
            ui.label(RichText::new(txt).small().color(col));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("Stormworks Engineering Suite").small().color(theme::TEXT_DIM));
            });
        });
    }

    /// Rounded pill toolbar: history, tools, view, modes.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            theme::pill().show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.model.can_undo(), egui::Button::new("Undo").frame(false)).on_hover_text("Ctrl+Z").clicked() {
                        self.model.undo();
                    }
                    if ui.add_enabled(self.model.can_redo(), egui::Button::new("Redo").frame(false)).on_hover_text("Ctrl+Y").clicked() {
                        self.model.redo();
                    }
                    ui.separator();
                    if theme::tool_button(ui, self.tool == Tool::Select, "Select", "Click to select, Shift/Ctrl to add") {
                        self.tool = Tool::Select;
                    }
                    if theme::tool_button(ui, self.tool == Tool::Place, "Place", "Place the catalogue part under the cursor") {
                        self.tool = Tool::Place;
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Frame").frame(false)).on_hover_text("F").clicked() {
                        self.cam.frame(&self.faces);
                    }
                });
            });
            ui.add_space(12.0);
            theme::pill().show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (m, name, tip) in [(Mode::Build, "Build", "Block editor and catalogue"), (Mode::Hull, "Hull", "Parametric NURBS hull, hydrostatics, bake to blocks"), (Mode::Decks, "Decks", "Deck and floor planning, generators")] {
                        if theme::tool_button(ui, self.mode == m, name, tip) {
                            self.mode = m;
                        }
                    }
                });
            });
        });
    }

    fn apply_bake(&mut self, b: design::Bake) {
        let rem: Vec<_> = b.remove.iter().map(|p| (p.def.clone(), p.vp, p.r)).collect();
        let add: Vec<_> = b.add.iter().map(|(p, c)| (p.def.clone(), p.vp, p.r, c.clone())).collect();
        let (r, a) = self.model.replace_generated(0, &rem, &add);
        self.status = format!("Hull baked: {a} blocks added, {r} previous hull blocks removed (one undo step)");
    }

    fn decks_panel(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Decks & floor plan");
        ui.label("Deck planning arrives in the next step: decks at heights, rooms, and generated floors, walls, doors and railings inside the hull.");
    }

    fn left_panel(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Catalogue");
        ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("search parts"));
        ui.label(RichText::new(format!("Placing: {}", self.place_def)).small());
        egui::ScrollArea::vertical().show(ui, |ui| {
            let q = self.search.to_lowercase();
            let mut shown = 0;
            for (id, d) in &self.catalogue.defs {
                if !q.is_empty() && !id.to_lowercase().contains(&q) && !d.name.to_lowercase().contains(&q) {
                    continue;
                }
                shown += 1;
                if shown > 400 {
                    ui.label("… refine the search");
                    break;
                }
                if ui.selectable_label(self.place_def == *id, format!("{}  ·  {}", d.name, id)).clicked() {
                    self.place_def = id.clone();
                    self.tool = Tool::Place;
                }
            }
        });
    }

    fn right_panel(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Inspector");
        let comps = self.model.components();
        let sel: Vec<_> = comps.iter().filter(|(id, _)| self.selected.contains(id)).collect();
        match sel.len() {
            0 => {
                ui.label(format!("{} components in {} bodies", comps.len(), self.model.body_count()));
                let mass: f64 = comps.iter().map(|(_, c)| self.catalogue.get(&c.def).map(|d| d.mass).unwrap_or(0.0)).sum();
                ui.label(format!("Mass (definition units): {:.0}", mass.max(0.0)));
            }
            1 => {
                let (id, c) = sel[0];
                let name = self.catalogue.get(&c.def).map(|d| d.name.clone()).unwrap_or_else(|| "(unknown definition)".into());
                ui.label(RichText::new(name).strong());
                egui::Grid::new("insp").num_columns(2).show(ui, |ui| {
                    ui.label("definition");
                    ui.label(&c.def);
                    ui.end_row();
                    ui.label("body / index");
                    ui.label(format!("{} / {}", id.body, id.comp));
                    ui.end_row();
                    ui.label("voxel");
                    ui.label(format!("{:?}", c.voxel));
                    ui.end_row();
                    ui.label("r");
                    ui.label(format!("{:?}", c.rotation));
                    ui.end_row();
                    ui.label("scale");
                    ui.label(format!("{:?}", Model::scale_of(&c.rotation)));
                    ui.end_row();
                });
                if let Some(e) = self.model.component_element(*id) {
                    let mut s = String::new();
                    e.write(&mut s);
                    if s.len() > 4000 {
                        s.truncate(4000);
                        s.push('…');
                    }
                    ui.collapsing("Raw XML (preserved on save)", |ui| {
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| ui.monospace(s));
                    });
                }
            }
            n => {
                ui.label(format!("{n} components selected"));
            }
        }
        if !sel.is_empty() {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Rotate Y (R)").clicked() {
                    let (ids, p) = (self.sel(), self.pivot());
                    self.model.rotate_quarter(&ids, 1, p);
                }
                if ui.button("Scale… (S)").clicked() {
                    self.open_scale_dialog();
                }
                if ui.button("Duplicate (Ctrl+D)").clicked() {
                    let ids = self.sel();
                    self.selected = self.model.duplicate(&ids, [0, 1, 0]).into_iter().collect();
                }
                if ui.button("Delete (Del)").clicked() {
                    let ids = self.sel();
                    self.model.delete(&ids);
                    self.selected.clear();
                }
            });
        }
        ui.separator();
        theme::section(ui, "Diagnostics");
        egui::ScrollArea::vertical().id_salt("diag").show(ui, |ui| {
            for d in self.diags.clone() {
                let col = match d.severity {
                    Severity::Error => Color32::from_rgb(240, 90, 90),
                    Severity::Warning => Color32::from_rgb(240, 190, 80),
                    Severity::Info => Color32::from_rgb(140, 180, 230),
                };
                let r = ui.add(egui::Label::new(RichText::new(format!("{} {}", match d.severity { Severity::Error => "[error]", Severity::Warning => "[warn]", Severity::Info => "[info]" }, d.message)).color(col)).sense(egui::Sense::click()));
                if !d.components.is_empty() && r.on_hover_text("click to select").clicked() {
                    self.selected = d.components.iter().copied().collect();
                }
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(d) = self.dialog else { return };
        let mut close = false;
        let title = match d {
            Dialog::Open => "Open vehicle",
            Dialog::SaveAs => "Save / export vehicle XML",
            Dialog::Settings => "Settings",
            Dialog::Scale => "Scale parts",
        };
        egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| match d {
            Dialog::Open | Dialog::SaveAs => {
                ui.label("Path to a vehicle .xml file:");
                ui.add(egui::TextEdit::singleline(&mut self.path_input).desired_width(480.0));
                ui.horizontal(|ui| {
                    let ok = ui.button(if d == Dialog::Open { "Open" } else { "Save" }).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ok && !self.path_input.trim().is_empty() {
                        let p = PathBuf::from(self.path_input.trim());
                        if d == Dialog::Open {
                            self.open(p)
                        } else {
                            self.save(p)
                        }
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            }
            Dialog::Settings => {
                ui.label("Stormworks definitions folder (rom/data/definitions). KEEL reads it; it is never copied or bundled.");
                ui.add(egui::TextEdit::singleline(&mut self.defs_input).desired_width(480.0));
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        let s = self.defs_input.trim();
                        self.settings.definitions_dir = (!s.is_empty()).then(|| PathBuf::from(s));
                        self.settings.save();
                        self.load_catalogue();
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            }
            Dialog::Scale => {
                ui.label("Integer scale factor per local axis (0 = zero-size). Written into the part transform exactly like a workshop XML edit.");
                ui.horizontal(|ui| {
                    for (i, a) in ["X", "Y", "Z"].iter().enumerate() {
                        ui.label(*a);
                        ui.add(egui::DragValue::new(&mut self.scale_input[i]).range(0..=64));
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "WARNING: Scaling does not guarantee sealing anywhere except the block's origin voxel. Buoyancy and \
                         watertightness come from the unscaled definition at the origin, so a scaled hull may leak or float \
                         differently from how it looks.",
                    )
                    .color(Color32::from_rgb(240, 190, 80)),
                );
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        let ids = self.sel();
                        self.model.set_scale(&ids, self.scale_input);
                        self.status = format!(
                            "Scaled {} part(s) to {:?}. Sealing is only guaranteed at each block's origin voxel",
                            ids.len(),
                            self.scale_input
                        );
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            }
        });
        if close {
            self.dialog = None;
        }
    }
}

impl eframe::App for KeelApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.refresh();
        self.shortcuts(&ctx);
        self.refresh();
        let title = format!(
            "KEEL: {}{}",
            self.path.as_ref().map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into()),
            if self.model.dirty { " •" } else { "" }
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        if !self.clean {
            egui::Panel::top("title").frame(theme::bar_frame()).show(ui, |ui| self.title_row(ui));
            egui::Panel::top("menu").frame(theme::bar_frame()).show(ui, |ui| self.menu(ui));
            egui::Panel::top("toolbar").frame(theme::bar_frame().inner_margin(egui::Margin::symmetric(12, 8))).show(ui, |ui| self.toolbar(ui));
            egui::Panel::bottom("status").frame(theme::bar_frame()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.status).small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let n = self.model.components().len();
                        ui.label(RichText::new(format!("{n} parts · {} bodies", self.model.body_count())).small().color(theme::TEXT_DIM));
                    });
                });
            });
            let mut bake = None;
            egui::Panel::left("left").frame(theme::side_frame()).default_size(300.0).show(ui, |ui| match self.mode {
                Mode::Build => self.left_panel(ui),
                Mode::Hull => {
                    self.design.refresh();
                    bake = self.design.params_ui(ui);
                }
                Mode::Decks => self.decks_panel(ui),
            });
            if let Some(b) = bake {
                self.apply_bake(b);
            }
            egui::Panel::right("right").frame(theme::side_frame()).default_size(310.0).show(ui, |ui| {
                if self.mode == Mode::Hull {
                    self.design.hydro_ui(ui);
                    ui.separator();
                }
                self.right_panel(ui)
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            let rect = ui.available_rect_before_wrap();
            // placement ghost under the cursor
            let mut ghost = None;
            let mut place_at = None;
            if self.tool == Tool::Place {
                if let Some(h) = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p)) {
                    let view = self.cam.view(rect);
                    let target = match viewport::pick(&view, &self.faces, h) {
                        Some(hit) => {
                            self.place_body = hit.comp.body;
                            let p = [0, 1, 2].map(|i| ((hit.point[i] + hit.normal[i] * 0.125) / 0.25).round() as i32);
                            Some(p)
                        }
                        None if self.faces.is_empty() => Some([0, 0, 0]),
                        None => None,
                    };
                    if let Some(p) = target {
                        let mut tmp = Model::new_empty();
                        tmp.add(0, &self.place_def, p, keel_io::vehicle::IDENTITY);
                        ghost = Some(geometry::build_faces(&tmp, &self.catalogue));
                        place_at = Some(p);
                    }
                }
            }
            let r = viewport::show(ui, &mut self.cam, &self.renderer, ghost.as_deref());
            if let Some((pos, mods)) = r.clicked {
                match self.tool {
                    Tool::Select => {
                        let view = self.cam.view(rect);
                        match viewport::pick(&view, &self.faces, pos) {
                            Some(hit) => {
                                if mods.shift || mods.command {
                                    if !self.selected.remove(&hit.comp) {
                                        self.selected.insert(hit.comp);
                                    }
                                } else {
                                    self.selected = [hit.comp].into_iter().collect();
                                }
                            }
                            None if !mods.shift => self.selected.clear(),
                            None => {}
                        }
                    }
                    Tool::Place => {
                        if let Some(p) = place_at {
                            let body = self.place_body.min(self.model.body_count().saturating_sub(1));
                            if let Some(id) = self.model.add(body, &self.place_def, p, keel_io::vehicle::IDENTITY) {
                                self.selected = [id].into_iter().collect();
                                self.status = format!("Placed {} at {:?}", self.place_def, p);
                            }
                        }
                    }
                }
            }
        });
        self.dialogs(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.save();
    }
}

struct Args {
    vehicle: Option<PathBuf>,
    defs: Option<PathBuf>,
    rom: Option<PathBuf>,
    view: Option<[f32; 2]>,
    clean: bool,
    zoom: f32,
    look: Option<[f32; 3]>,
    mode: Option<String>,
    bake: bool,
}

fn parse_args() -> Args {
    let mut a = Args { vehicle: None, defs: None, rom: None, view: None, clean: false, zoom: 1.0, look: None, mode: None, bake: false };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        if x == "--defs" {
            a.defs = it.next().map(PathBuf::from);
        } else if x == "--bake-hull" {
            a.bake = true;
        } else if x == "--mode" {
            a.mode = it.next();
        } else if x == "--zoom" {
            a.zoom = it.next().and_then(|v| v.parse().ok()).unwrap_or(1.0);
        } else if x == "--look" {
            a.look = it.next().and_then(|v| {
                let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                (p.len() == 3).then(|| [p[0], p[1], p[2]])
            });
        } else if x == "--clean" {
            a.clean = true;
        } else if x == "--rom" {
            a.rom = it.next().map(PathBuf::from);
        } else if x == "--view" {
            // yaw,pitch in degrees (camera direction from the target)
            a.view = it.next().and_then(|v| {
                let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                (p.len() == 2).then(|| [p[0].to_radians(), p[1].to_radians()])
            });
        } else if x == "--help" || x == "-h" {
            println!("keel [vehicle.xml] [--defs <rom/data/definitions>] [--rom <rom folder with meshes/>] [--view yaw,pitch]");
            std::process::exit(0);
        } else {
            a.vehicle = Some(PathBuf::from(x));
        }
    }
    if a.defs.is_none() {
        a.defs = std::env::var_os("KEEL_DEFINITIONS").map(PathBuf::from);
    }
    a
}

fn main() -> eframe::Result<()> {
    let args = parse_args();
    let options = eframe::NativeOptions {
        depth_buffer: 24,
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 860.0]).with_title("KEEL"),
        ..Default::default()
    };
    eframe::run_native(
        "KEEL",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(KeelApp::new(args)))
        }),
    )
}
