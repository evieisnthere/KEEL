#![cfg_attr(windows, windows_subsystem = "windows")]
//! KEEL desktop application.
//!
//! Usage: `keel [vehicle.xml] [--defs <rom/data/definitions>]`. The definitions folder is remembered in the settings file.
mod design;
mod icons;
mod interior;
mod mc;
mod palette;
mod tools;
mod wiring;
mod power;
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
    Power,
    Logic,
}

use tools::Tool;

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
    palette: palette::Palette,
    wiring: wiring::Wiring,
    tools: tools::Tools,
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
    interior: interior::Interior,
    power: power::Power,
    mc: mc::McEditor,
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
            palette: palette::Palette::default(),
            wiring: wiring::Wiring::default(),
            tools: tools::Tools::default(),
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
            mode: match args.mode.as_deref() { Some("hull") => Mode::Hull, Some("decks") => Mode::Decks, Some("power") => Mode::Power, Some("logic") => Mode::Logic, _ => Mode::Build },
            design: design::Design::default(),
            interior: interior::Interior::with_docked(args.plan_docked),
            power: power::Power::default(),
            mc: mc::McEditor::default(),
            frame_next_hull: true,
            bake_on_start: args.bake,
            settings,
        };
        if let Some(t) = &args.tool {
            if let Some((tool, ..)) = Tool::ALL.iter().find(|(_, _, n, _)| n.eq_ignore_ascii_case(t)) {
                app.tool = *tool;
                app.wiring.all = true;
            }
        }
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
                self.interior.doc = self.design.doc.interior.clone();
                self.power.doc = self.design.doc.power.clone();
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
                self.design.doc.interior = self.interior.doc.clone();
                self.design.doc.power = self.power.doc.clone();
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
                self.design.show_preview = false;
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
        if self.mode == Mode::Logic {
            // the canvas handles its own keys; only history and save are global here
            let (undo, redo, save) = ctx.input(|i| {
                let c = i.modifiers.command;
                (c && i.key_pressed(egui::Key::Z) && !i.modifiers.shift, c && (i.key_pressed(egui::Key::Y) || (i.key_pressed(egui::Key::Z) && i.modifiers.shift)), c && i.key_pressed(egui::Key::S))
            });
            if undo && self.model.undo() {
                self.status = "Undo".into();
            }
            if redo && self.model.redo() {
                self.status = "Redo".into();
            }
            if save {
                match self.path.clone() {
                    Some(p) => self.save(p),
                    None => self.dialog = Some(Dialog::SaveAs),
                }
            }
            return;
        }
        let ids = self.sel();
        let (copy, paste, mirror) = ctx.input(|i| (i.modifiers.command && i.key_pressed(egui::Key::C), i.modifiers.command && i.key_pressed(egui::Key::V), !i.modifiers.command && i.key_pressed(egui::Key::M)));
        if copy && !ids.is_empty() {
            self.copy_selection();
        }
        if paste && !self.tools.clipboard.is_empty() {
            self.tool = Tool::Place;
            self.tools.stamping = true;
            self.status = format!("Stamp: {} parts; click to paste, Esc to stop", self.tools.clipboard.len());
        }
        if mirror {
            self.tools.mirror = !self.tools.mirror;
            self.status = format!("Mirror {}", if self.tools.mirror { "on: placing, filling and painting are mirrored across the centre line" } else { "off" });
        }
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
        if del && self.tool == Tool::Wire && self.wiring.selected.is_some() {
            let l = self.wiring.selected.take().unwrap();
            self.model.remove_links(&[l]);
            self.status = "Wire removed".into();
        } else if del && !ids.is_empty() {
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
            if matches!(self.tool, Tool::Place | Tool::Fill) {
                if self.tools.stamping {
                    self.tools.clipboard = stamp_turn(&self.tools.clipboard, axis);
                } else {
                    self.tools.place_rot = tools::turn(self.tools.place_rot, axis);
                }
                self.status = format!("Placement rotated 90° about {}", ["X", "Y", "Z"][axis]);
            } else if !ids.is_empty() {
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
            self.tools.stamping = false;
            self.tools.fill_start = None;
            self.tools.measure.clear();
            self.wiring.pending = None;
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

    /// Add a 2×2 microcontroller above the vehicle and open it in the editor.
    fn create_mc(&mut self) {
        let top = self.model.components().iter().map(|(_, c)| c.voxel[1]).max().map(|y| y + 2).unwrap_or(0);
        if self.model.body_count() == 0 {
            self.status = "The vehicle has no body to add a microcontroller to".into();
            return;
        }
        if let Some(id) = self.model.add(0, "microprocessor", [0, top, 0], keel_io::vehicle::IDENTITY) {
            let g = keel_mc::graph::Graph::new_empty("New microcontroller", 2, 2);
            self.model.set_mc_definition(id, g.to_element());
            self.mc.open(id, &self.model);
            self.status = format!("Added a microcontroller at {:?}", [0, top, 0]);
        }
    }

    fn copy_selection(&mut self) {
        let parts: Vec<_> = self.model.components().into_iter().filter(|(id, _)| self.selected.contains(id)).map(|(_, c)| c).collect();
        let lo = [0, 1, 2].map(|i| parts.iter().map(|c| c.voxel[i]).min().unwrap_or(0));
        self.tools.clipboard = parts
            .iter()
            .map(|c| keel_model::edit::GenPart { def: c.def.clone(), vp: [0, 1, 2].map(|i| c.voxel[i] - lo[i]), r: c.rotation, bc: c.attrs.get("bc").cloned(), ac: c.attrs.get("ac").cloned() })
            .collect();
        self.status = format!("Copied {} parts (Ctrl+V to stamp)", self.tools.clipboard.len());
    }

    /// Parts the Place / Fill tools would add with the cursor at voxel `p`.
    fn placement(&self, p: [i32; 3]) -> Vec<keel_model::edit::GenPart> {
        use keel_model::edit::GenPart;
        let colour = tools::hex(self.tools.colour);
        let parts = match self.tool {
            Tool::Place if self.tools.stamping => self.tools.clipboard.iter().map(|c| GenPart { vp: [0, 1, 2].map(|i| c.vp[i] + p[i]), ..c.clone() }).collect(),
            Tool::Place => vec![GenPart { def: self.place_def.clone(), vp: p, r: self.tools.place_rot, bc: Some(colour), ac: None }],
            Tool::Fill => match self.tools.fill_start {
                Some(a) if (0..3).map(|i| (a[i] - p[i]).abs() + 1).product::<i32>() <= 8192 => tools::fill_box(&self.place_def, a, p, self.tools.place_rot, &colour),
                Some(_) => vec![],
                None => vec![GenPart { def: self.place_def.clone(), vp: p, r: self.tools.place_rot, bc: Some(colour), ac: None }],
            },
            _ => vec![],
        };
        tools::with_mirror(parts, self.tools.mirror)
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
                ui.label(RichText::new(format!("KEEL {}", env!("CARGO_PKG_VERSION"))).strong());
                ui.label(RichText::new("Stormworks Engineering Suite. Not affiliated with Geometa.").small().color(theme::TEXT_DIM));
                ui.separator();
                ui.label("Left-drag orbit · right-drag pan · wheel zoom · F frame");
                ui.label("Click select (Shift adds) · arrows / PgUp / PgDn move · R rotate");
                ui.label("S scale · Del delete · Ctrl+D duplicate · Ctrl+C / Ctrl+V stamp");
                ui.label("M mirror · Alt+click with Paint picks a colour · Esc cancels");
                ui.label("Ctrl+Z / Ctrl+Y undo / redo · Ctrl+S save");
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
            None => self.status = "Open cancelled (use File > Open by path if no dialog appeared)".into(),
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
    /// Document-style header: logo spanning two rows, vessel name and save state above the menu bar.
    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::hover());
            theme::logo(ui.painter(), r);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    let name = self.path.as_ref().map(|p| {
                        let dir = p.parent().and_then(|d| d.file_name()).map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
                        if p.file_name().is_some_and(|f| f == "vehicle.xml") && !dir.is_empty() { dir } else { p.file_name().unwrap_or_default().to_string_lossy().to_string() }
                    });
                    ui.label(RichText::new(name.unwrap_or_else(|| "Untitled vessel".into())).size(18.0));
                    ui.add_space(10.0);
                    let (txt, col) = if self.model.dirty { ("Unsaved changes", theme::ORANGE) } else if self.path.is_some() { ("All changes saved", theme::TEXT_DIM) } else { ("Not saved yet", theme::TEXT_DIM) };
                    let (d, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter().circle_filled(d.center(), 3.5, col);
                    ui.label(RichText::new(txt).small().color(col));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("KEEL {}  ·  Stormworks Engineering Suite", env!("CARGO_PKG_VERSION"))).small().color(theme::TEXT_DIM));
                    });
                });
                self.menu(ui);
            });
        });
    }

    /// Rounded pill toolbar: history, tools, view, modes.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            theme::pill().show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.add_enabled_ui(self.model.can_undo(), |ui| icons::button(ui, false, icons::Icon::Undo, "", "Undo (Ctrl+Z)")).inner.clicked() {
                        self.model.undo();
                    }
                    if ui.add_enabled_ui(self.model.can_redo(), |ui| icons::button(ui, false, icons::Icon::Redo, "", "Redo (Ctrl+Y)")).inner.clicked() {
                        self.model.redo();
                    }
                    if self.mode != Mode::Logic {
                        if icons::button(ui, false, icons::Icon::Frame, "", "Frame the vehicle (F)").clicked() {
                            self.cam.frame(&self.faces);
                        }
                    }
                });
            });
            if self.mode == Mode::Build {
                ui.add_space(8.0);
                theme::pill().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (t, icon, name, tip) in Tool::ALL {
                            if icons::button(ui, self.tool == t, icon, name, tip).clicked() {
                                self.tool = t;
                                self.tools.stamping = false;
                                self.tools.fill_start = None;
                            }
                        }
                        ui.separator();
                        if icons::button(ui, self.tools.mirror, icons::Icon::Mirror, "Mirror", "Mirror placing, filling and painting across the centre line (M)").clicked() {
                            self.tools.mirror = !self.tools.mirror;
                        }
                        ui.color_edit_button_srgb(&mut self.tools.colour).on_hover_text("Paint colour (used by Place, Fill and Paint; Alt+click with Paint picks a colour)");
                    });
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                theme::pill().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (m, icon, name, tip) in [
                            (Mode::Build, icons::Icon::Place, "Build", "Block editor, part palette and wiring"),
                            (Mode::Hull, icons::Icon::Hull, "Hull", "Parametric hull, hydrostatics, bake to blocks"),
                            (Mode::Decks, icons::Icon::Decks, "Decks", "Decks, floor plans, rooms, furniture and lighting"),
                            (Mode::Power, icons::Icon::Power, "Power", "Powertrain designer and calculators"),
                            (Mode::Logic, icons::Icon::Chip, "Logic", "Microcontroller editor and simulator"),
                        ]
                        .into_iter()
                        .rev()
                        {
                            if icons::button(ui, self.mode == m, icon, name, tip).clicked() {
                                self.mode = m;
                            }
                        }
                    });
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

    fn apply_interior(&mut self, g: interior::Generated) {
        use keel_model::edit::GenLink;
        let rem = std::mem::take(&mut self.interior.doc.generated);
        let rem_links: Vec<GenLink> = std::mem::take(&mut self.interior.doc.generated_links).into_iter().map(|(kind, a, b)| GenLink { kind, a, b }).collect();
        let b = &g.layout.build;
        let (r, a, l) = self.model.apply_build_replacing(0, &rem, &rem_links, &b.parts, &b.links);
        self.interior.doc.generated = b.parts.iter().map(|p| (p.def.clone(), p.vp, p.r)).collect();
        self.interior.doc.generated_links = b.links.iter().map(|l| (l.kind, l.a, l.b)).collect();
        self.status = format!("Interior generated: {a} parts and {l} wires added, {r} previous parts removed (one undo step); {} lighting circuits", g.layout.circuits.len());
    }

    fn left_panel(&mut self, ui: &mut egui::Ui) {
        if self.tool == Tool::Wire {
            theme::section(ui, "Wiring");
            ui.checkbox(&mut self.wiring.all, "Show every part's ports");
            ui.label(RichText::new("Select parts to see their ports, or show them all. Click an output (filled) then an input (ringed); electric and composite ports join either way. Click a wire and press Delete to remove it.").small().color(theme::TEXT_DIM));
            ui.add_space(4.0);
            for k in [0, 1, 4, 5, 6, 7] {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 5.0, wiring::kind_colour(k));
                    ui.label(wiring::kind_name(k));
                });
            }
            ui.label(RichText::new(format!("{} wires in this vehicle", self.model.links().len())).small().color(theme::TEXT_DIM));
            ui.separator();
        }
        if self.tool == Tool::Measure {
            theme::section(ui, "Measure");
            ui.label(RichText::new("Click two points on the vehicle. One block is 0.25 m.").small().color(theme::TEXT_DIM));
            ui.separator();
        }
        theme::section(ui, "Parts");
        if let Some(d) = self.palette.ui(ui, &self.catalogue, &mut self.meshes, &self.place_def) {
            self.place_def = d;
            self.tools.stamping = false;
            if !matches!(self.tool, Tool::Place | Tool::Fill) {
                self.tool = Tool::Place;
            }
        }
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
                if c.def == "microprocessor" && ui.button("Edit microcontroller").clicked() {
                    self.mode = Mode::Logic;
                    self.mc.open(*id, &self.model);
                }
                if let Some(bc) = c.attrs.get("bc") {
                    ui.horizontal(|ui| {
                        ui.label("paint");
                        let rgb = parse_hex(bc);
                        let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 4.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                        ui.label(RichText::new(bc).monospace());
                    });
                }
                let links = self.wiring.links_of(&self.model, &self.catalogue, *id);
                if !links.is_empty() {
                    theme::section(ui, "Wires");
                    let mut gone = None;
                    for (l, desc) in &links {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, wiring::kind_colour(l.kind));
                            ui.label(RichText::new(desc).small());
                            if icons::button(ui, false, icons::Icon::Trash, "", "Remove this wire").clicked() {
                                gone = Some(l.clone());
                            }
                        });
                    }
                    if let Some(l) = gone {
                        self.model.remove_links(&[l]);
                    }
                }
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
                    Severity::Error => theme::RED,
                    Severity::Warning => theme::ORANGE,
                    Severity::Info => Color32::from_rgb(0x7a, 0x9c, 0xe8),
                };
                let fr = egui::Frame::NONE.inner_margin(egui::Margin { left: 16, right: 0, top: 1, bottom: 1 }).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(&d.message).color(if d.severity == Severity::Info { theme::TEXT_DIM } else { col })).wrap().sense(egui::Sense::click()))
                });
                ui.painter().circle_filled(fr.response.rect.left_top() + egui::vec2(5.0, 10.0), 4.0, col);
                let r = fr.inner;
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

impl KeelApp {
    /// Measure tool: the two picked points, the distance and its components, plus a live rubber band.
    fn measure_overlay(&mut self, ui: &egui::Ui, view: &viewport::View, cursor: Option<[f32; 3]>) {
        let painter = ui.painter();
        let mut pts = self.tools.measure.clone();
        if pts.len() == 1 {
            if let Some(c) = cursor {
                pts.push(c);
            }
        }
        for p in &pts {
            if let Some((s, _)) = view.project(*p) {
                painter.circle_filled(s, 4.0, theme::CYAN);
            }
        }
        if let [a, b] = pts[..] {
            if let (Some((sa, _)), Some((sb, _))) = (view.project(a), view.project(b)) {
                painter.line_segment([sa, sb], egui::Stroke::new(2.0, theme::CYAN));
                let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                let txt = format!("{len:.2} m  ({:.0} blocks)\nΔx {:.2}  Δy {:.2}  Δz {:.2}", len / 0.25, d[0].abs(), d[1].abs(), d[2].abs());
                let at = sa + (sb - sa) * 0.5 + egui::vec2(10.0, -10.0);
                let galley = painter.layout_no_wrap(txt, egui::FontId::proportional(13.0), theme::TEXT);
                let bg = egui::Rect::from_min_size(at - egui::vec2(6.0, galley.size().y + 4.0), galley.size() + egui::vec2(12.0, 8.0));
                painter.rect_filled(bg, 6.0, theme::PANEL.gamma_multiply(0.95));
                painter.rect_stroke(bg, 6.0, egui::Stroke::new(1.0, theme::CYAN), egui::StrokeKind::Inside);
                painter.galley(bg.min + egui::vec2(6.0, 4.0), galley, theme::TEXT);
            }
        }
    }
}

fn parse_hex(s: &str) -> [u8; 3] {
    let s = s.trim_start_matches('#');
    let v = u32::from_str_radix(s.get(..6).unwrap_or(s), 16).unwrap_or(0xFFFFFF);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// Rotate a copied stamp a quarter turn about `axis` (positions and orientations), keeping its corner at the origin.
fn stamp_turn(parts: &[keel_model::edit::GenPart], axis: usize) -> Vec<keel_model::edit::GenPart> {
    let q = keel_model::edit::quarter_turn(axis);
    let mut out: Vec<_> = parts
        .iter()
        .map(|p| {
            let vp = [0, 1, 2].map(|i| (0..3).map(|k| q[i][k] * p.vp[k]).sum::<i32>());
            keel_model::edit::GenPart { vp, r: tools::turn(p.r, axis), ..p.clone() }
        })
        .collect();
    let lo = [0, 1, 2].map(|i| out.iter().map(|p| p.vp[i]).min().unwrap_or(0));
    for p in &mut out {
        for i in 0..3 {
            p.vp[i] -= lo[i];
        }
    }
    out
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
            egui::Panel::top("header").show_separator_line(false).frame(theme::bar_frame().inner_margin(egui::Margin { left: 14, right: 14, top: 8, bottom: 2 })).show(ui, |ui| self.header(ui));
            egui::Panel::top("toolbar").show_separator_line(false).frame(theme::bar_frame().inner_margin(egui::Margin { left: 12, right: 12, top: 6, bottom: 8 })).show(ui, |ui| self.toolbar(ui));
            egui::Panel::bottom("status").show_separator_line(false).frame(theme::bar_frame()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.status).small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let n = self.model.components().len();
                        ui.label(RichText::new(format!("{n} parts · {} bodies", self.model.body_count())).small().color(theme::TEXT_DIM));
                    });
                });
            });
            let mut bake = None;
            let mut generated = None;
            egui::Panel::left("left").frame(theme::side_frame()).show_separator_line(false).default_size(300.0).show(ui, |ui| theme::card(ui, |ui| match self.mode {
                Mode::Build => self.left_panel(ui),
                Mode::Hull => {
                    self.design.refresh();
                    bake = self.design.params_ui(ui);
                }
                Mode::Power => {
                    self.design.refresh();
                    let mass: f64 = self.model.components().iter().map(|(_, c)| self.catalogue.get(&c.def).map(|d| d.mass).unwrap_or(0.0)).sum();
                    self.power.refresh(self.design.hull.as_ref(), &self.design.doc.hull, mass);
                    self.power.panel_ui(ui);
                }
                Mode::Logic => {
                    self.mc.sync(&self.model);
                    if self.mc.panel_ui(ui, &self.model) {
                        self.create_mc();
                    }
                }
                Mode::Decks => {
                    self.design.refresh();
                    self.interior.refresh(self.design.hull.as_ref(), &self.design.doc.hull, self.design.doc.origin);
                    if let Some(g) = self.interior.panel_ui(ui) {
                        generated = Some(g);
                    }
                }
            }));
            if let Some(b) = bake {
                self.apply_bake(b);
            }
            if let Some(g) = generated {
                self.apply_interior(g);
            }
            egui::Panel::right("right").frame(theme::side_frame()).show_separator_line(false).default_size(310.0).show(ui, |ui| theme::card(ui, |ui| {
                if self.mode == Mode::Hull {
                    self.design.hydro_ui(ui);
                    ui.separator();
                }
                if self.mode == Mode::Power {
                    self.power.results_ui(ui);
                    ui.separator();
                }
                if self.mode == Mode::Logic {
                    self.mc.inspector_ui(ui, &mut self.model);
                    return;
                }
                self.right_panel(ui)
            }));
        }
        if self.mode == Mode::Decks {
            self.interior.refresh(self.design.hull.as_ref(), &self.design.doc.hull, self.design.doc.origin);
            self.interior.plan_window(&ctx);
            if self.interior.docked {
                egui::CentralPanel::default().frame(theme::centre_frame()).show(ui, |ui| self.interior.plan_ui(ui));
                return;
            }
        }
        if self.mode == Mode::Logic {
            self.mc.sync(&self.model);
            egui::CentralPanel::default().frame(theme::centre_frame()).show(ui, |ui| {
                let r = ui.available_rect_before_wrap();
                self.mc.canvas(ui, &mut self.model);
                theme::round_corners(ui.painter(), r);
            });
            if !self.mc.status.is_empty() {
                self.status = std::mem::take(&mut self.mc.status);
            }
            self.dialogs(&ctx);
            return;
        }
        egui::CentralPanel::default().frame(if self.clean { egui::Frame::NONE } else { theme::centre_frame() }).show(ui, |ui| {
            let rect = ui.available_rect_before_wrap();
            let view = self.cam.view(rect);
            let hover = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p));
            let hit = hover.and_then(|h| viewport::pick(&view, &self.faces, h));
            // the voxel a new part would occupy: against the face under the cursor, or the origin in an empty scene
            let place_at = match &hit {
                Some(h) => Some([0, 1, 2].map(|i| ((h.point[i] + h.normal[i] * 0.125) / 0.25).round() as i32)),
                None if self.faces.is_empty() && hover.is_some() => Some([0, 0, 0]),
                None => None,
            };
            if let Some(h) = &hit {
                self.place_body = h.comp.body;
            }
            let mut ghost = None;
            if matches!(self.tool, Tool::Place | Tool::Fill) {
                if let Some(p) = place_at {
                    let parts = self.placement(p);
                    let mut tmp = Model::new_empty();
                    for g in &parts {
                        tmp.add(0, &g.def, g.vp, g.r);
                    }
                    ghost = Some(geometry::build_faces(&tmp, &self.catalogue));
                }
            }
            let r = viewport::show(ui, &mut self.cam, &self.renderer, ghost.as_deref());
            if !self.clean {
                theme::round_corners(ui.painter(), rect);
            }
            let click = r.clicked;
            if self.tool == Tool::Wire {
                if let Some(l) = self.wiring.overlay(ui, &view, &self.model, &self.catalogue, &self.selected, click.map(|c| c.0)) {
                    self.model.add_link(l);
                }
                if !self.wiring.status.is_empty() {
                    self.status = std::mem::take(&mut self.wiring.status);
                }
            }
            if self.tool == Tool::Measure {
                self.measure_overlay(ui, &view, hit.as_ref().map(|h| h.point));
            }
            if let Some((_, mods)) = click {
                match self.tool {
                    Tool::Select => match &hit {
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
                    },
                    Tool::Place => {
                        if let Some(p) = place_at {
                            let parts = self.placement(p);
                            let body = self.place_body.min(self.model.body_count().saturating_sub(1));
                            let (_, a, _) = self.model.apply_build(body, &[], &parts, &[]);
                            if !self.tools.stamping {
                                self.palette.used(&self.place_def);
                            }
                            self.status = format!("Placed {a} part(s) at {p:?}");
                        }
                    }
                    Tool::Fill => {
                        if let Some(p) = place_at {
                            match self.tools.fill_start.take() {
                                None => {
                                    self.tools.fill_start = Some(p);
                                    self.status = format!("Fill from {p:?}: click the opposite corner (Esc cancels)");
                                }
                                Some(a) => {
                                    self.tools.fill_start = Some(a);
                                    let parts = self.placement(p);
                                    self.tools.fill_start = None;
                                    if parts.is_empty() {
                                        self.status = "Fill box too large (limit 8192 parts)".into();
                                    } else {
                                        let body = self.place_body.min(self.model.body_count().saturating_sub(1));
                                        let (_, n, _) = self.model.apply_build(body, &[], &parts, &[]);
                                        self.status = format!("Filled {n} parts (one undo step)");
                                    }
                                }
                            }
                        }
                    }
                    Tool::Paint => {
                        if let Some(h) = &hit {
                            let comps = self.model.components();
                            if mods.alt {
                                if let Some(bc) = comps.iter().find(|(id, _)| *id == h.comp).and_then(|(_, c)| c.attrs.get("bc")) {
                                    self.tools.colour = parse_hex(bc);
                                    self.status = format!("Picked colour {bc}");
                                }
                            } else {
                                let mut ids = vec![h.comp];
                                if self.tools.mirror {
                                    if let Some((_, c)) = comps.iter().find(|(id, _)| *id == h.comp) {
                                        let m = [-c.voxel[0], c.voxel[1], c.voxel[2]];
                                        ids.extend(comps.iter().filter(|(id, o)| id.body == h.comp.body && o.voxel == m && o.def == c.def && *id != h.comp).map(|(id, _)| *id));
                                    }
                                }
                                let hx = tools::hex(self.tools.colour);
                                self.model.set_paint(&ids, Some(&hx), None);
                                self.status = format!("Painted {} part(s) {hx}", ids.len());
                            }
                        }
                    }
                    Tool::Measure => {
                        if let Some(h) = &hit {
                            if self.tools.measure.len() >= 2 {
                                self.tools.measure.clear();
                            }
                            self.tools.measure.push(h.point);
                        }
                    }
                    Tool::Wire => {}
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
    plan_docked: bool,
    tool: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args { vehicle: None, defs: None, rom: None, view: None, clean: false, zoom: 1.0, look: None, mode: None, bake: false, plan_docked: false, tool: None };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        if x == "--defs" {
            a.defs = it.next().map(PathBuf::from);
        } else if x == "--plan-docked" {
            a.plan_docked = true;
        } else if x == "--bake-hull" {
            a.bake = true;
        } else if x == "--tool" {
            a.tool = it.next();
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
            println!("keel {} [vehicle.xml] [--defs <rom/data/definitions>] [--rom <rom folder with meshes/>] [--mode build|hull|decks|power|logic] [--tool select|place|paint|fill|measure|wire] [--view yaw,pitch]", env!("CARGO_PKG_VERSION"));
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
