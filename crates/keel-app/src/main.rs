#![cfg_attr(windows, windows_subsystem = "windows")]
//! KEEL desktop application.
//!
//! Usage: `keel [vehicle.xml] [--defs <rom/data/definitions>]`. The definitions folder is remembered in the settings file.
mod decal;
mod design;
mod game;
mod icons;
mod interior;
mod mc;
mod paint;
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
    /// frames drawn (for KEEL_SHOT=path:frames, a screenshot after N frames)
    frames: u64,
    export_name: String,
    /// thumbnail preview for the export dialog (rendered when the dialog opens)
    export_thumb: Option<(image::RgbaImage, egui::TextureHandle)>,
    /// textures of saved-vehicle thumbnails in the library dialog
    library_thumbs: std::collections::HashMap<PathBuf, Option<egui::TextureHandle>>,
    pending_thumb: Option<image::RgbaImage>,
    /// parts already painted in the current brush stroke
    stroke: HashSet<CompId>,
    livery_on_start: bool,
    decal_ui: decal::DecalUi,
}

#[derive(PartialEq, Clone, Copy)]
enum Dialog {
    Open,
    SaveAs,
    Settings,
    Scale,
    Welcome,
    Export,
    Library,
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
            frames: 0,
            export_name: String::new(),
            export_thumb: None,
            library_thumbs: Default::default(),
            pending_thumb: None,
            stroke: HashSet::new(),
            livery_on_start: args.livery,
            decal_ui: Default::default(),
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
        // first run: find the game through Steam, otherwise ask with a folder picker (no paths to type)
        if app.settings.definitions_dir.as_ref().is_none_or(|d| !d.is_dir()) {
            match game::find_game() {
                Some(g) => {
                    app.settings.definitions_dir = Some(game::definitions_of(&g));
                    app.settings.save();
                }
                None => app.dialog = Some(Dialog::Welcome),
            }
        }
        if app.settings.vehicles_dir.as_ref().is_none_or(|d| !d.is_dir()) {
            app.settings.vehicles_dir = game::vehicles_dir();
            app.settings.save();
        }
        app.load_catalogue();
        if let Some(p) = args.vehicle {
            app.open(p);
        }
        if let Some(img) = &args.decal_demo {
            // screenshot check: an 8×4 panel of paintable signs with the image on it
            let parts: Vec<keel_model::edit::GenPart> = (0..8).flat_map(|x| (0..4).map(move |z| keel_model::edit::GenPart { def: "sign_na".into(), vp: [x - 4, 0, z], r: keel_io::vehicle::IDENTITY, bc: Some("FFFFFF".into()), ac: None })).collect();
            app.model.apply_build(0, &[], &parts, &[]);
            app.selected = app.model.components().into_iter().map(|(id, _)| id).collect();
            if let Ok((png, _)) = decal::load_image(img) {
                app.add_decal("demo", png);
            }
            app.tool = Tool::Decal;
        }
        match args.dialog.as_deref() {
            Some("export") => app.open_export(),
            Some("library") => app.dialog = Some(Dialog::Library),
            Some("welcome") => app.dialog = Some(Dialog::Welcome),
            _ => {}
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
        }
        // --bake-hull (any mode): bake the current hull design once at start-up
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
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::E)) {
            self.open_export();
        }
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

    /// One brush dab: the part under the cursor (or every part within the brush radius), mirrored if Mirror is on,
    /// each with its own seeded variation of the brush colour. Parts already painted in this stroke are skipped.
    fn dab(&mut self, comp: CompId, point: [f32; 3], new_step: bool) {
        let d = self.design.doc.paint.clone();
        let comps = self.model.components();
        let centre = [0, 1, 2].map(|i| (point[i] / 0.25).round() as i32);
        let r = d.radius;
        let hit_voxel = comps.iter().find(|(id, _)| *id == comp).map(|(_, c)| (c.voxel, c.def.clone()));
        let mut targets: Vec<(CompId, [i32; 3])> = vec![];
        for (id, c) in &comps {
            if id.body != comp.body {
                continue;
            }
            let near = |v: [i32; 3]| r > 0.0 && (0..3).map(|i| ((c.voxel[i] - v[i]) as f32).powi(2)).sum::<f32>() <= r * r;
            let twin = self.tools.mirror && hit_voxel.as_ref().is_some_and(|(v, def)| c.voxel == [-v[0], v[1], v[2]] && &c.def == def);
            let mirrored = self.tools.mirror && near([-centre[0], centre[1], centre[2]]);
            if *id == comp || near(centre) || twin || mirrored {
                targets.push((*id, c.voxel));
            }
        }
        let paints: Vec<(CompId, String)> = targets
            .into_iter()
            .filter(|(id, _)| self.stroke.insert(*id))
            .map(|(id, v)| (id, paint::hex(paint::vary(self.tools.colour, d.variation, d.jitter, d.seed, v))))
            .collect();
        self.model.paint_each(&paints, new_step);
    }

    /// Pick an image and put it on the selected signs (one undo step).
    fn new_decal(&mut self) {
        let Some(p) = rfd::FileDialog::new().set_title("Choose an image for the decal").add_filter("Images", &["png", "jpg", "jpeg"]).pick_file() else { return };
        match decal::load_image(&p) {
            Ok((png, _)) => {
                let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                self.add_decal(&name, png);
            }
            Err(e) => self.status = format!("Could not read the image: {e}"),
        }
    }

    fn add_decal(&mut self, name: &str, png: String) {
        let sel = self.sel();
        match decal::create(&self.model, &sel, name, png) {
            Ok(d) => {
                let e = decal::edits(&self.model, &d, false);
                self.model.set_attr_each(&e, true);
                self.design.doc.decals.push(d);
                self.decal_ui.current = Some(self.design.doc.decals.len() - 1);
                self.status = format!("Decal added on {} sign(s). Adjust it on the left at any time.", e.len());
            }
            Err(m) => self.status = m,
        }
    }

    /// Apply the livery to the selection (or the whole vehicle) as one undo step.
    fn apply_livery(&mut self, default_waterline: Option<f32>) {
        let d = self.design.doc.paint.clone();
        let comps = self.model.components();
        let targets: Vec<_> = comps
            .iter()
            .filter(|(id, _)| self.selected.is_empty() || self.selected.contains(id))
            .filter(|(_, c)| !d.blocks_only || self.catalogue.get(&c.def).is_some_and(|x| x.category == 0))
            .collect();
        if targets.is_empty() {
            self.status = "Nothing to paint".into();
            return;
        }
        let (bottom, top) = targets.iter().map(|(_, c)| c.voxel[1] as f32 * 0.25).fold((f32::MAX, f32::MIN), |a, y| (a.0.min(y), a.1.max(y)));
        let wl = d.waterline.or(default_waterline).unwrap_or(0.0);
        let paints: Vec<(CompId, String)> = targets.iter().map(|(id, c)| (*id, paint::hex(paint::livery(&d, c.voxel, wl, bottom, top)))).collect();
        self.model.paint_each(&paints, true);
        self.status = format!("Livery applied to {} parts (one undo step)", paints.len());
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
                if ui.button("Open from Stormworks…").on_hover_text("Your saved vehicles in the game's vehicles folder").clicked() {
                    self.dialog = Some(Dialog::Library);
                    ui.close();
                }
                if ui.button("Open file…  (Ctrl+O)").clicked() {
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
                if ui.button("Export to Stormworks…  (Ctrl+E)").clicked() {
                    self.open_export();
                    ui.close();
                }
                if ui.button("Launch Stormworks").clicked() {
                    self.status = match game::launch() {
                        Ok(()) => "Starting Stormworks through Steam…".into(),
                        Err(e) => format!("Could not start Steam: {e}"),
                    };
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
                        let b = egui::Button::new(RichText::new("Export to Stormworks").color(theme::BG).strong()).fill(theme::CYAN).corner_radius(egui::CornerRadius::same(16)).min_size(egui::vec2(0.0, 30.0));
                        if ui.add(b).on_hover_text("Save this vehicle straight into the game's vehicles folder, with a thumbnail (Ctrl+E)").clicked() {
                            self.open_export();
                        }
                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("KEEL {}", env!("CARGO_PKG_VERSION"))).small().color(theme::TEXT_DIM));
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
        use keel_model::edit::{GenLink, GenPart};
        let rem_prev = std::mem::take(&mut self.interior.doc.generated);
        let rem_links: Vec<GenLink> = std::mem::take(&mut self.interior.doc.generated_links).into_iter().map(|(kind, a, b)| GenLink { kind, a, b }).collect();
        let b = &g.layout.build;
        // hull parts in the way of a new shaft or pipe make way; ones displaced earlier come back if no longer needed
        let clear: HashSet<[i32; 3]> = b.clear.iter().copied().collect();
        let generated: HashSet<([i32; 3], String)> = rem_prev.iter().map(|(d, v, _)| (*v, d.clone())).collect();
        let prev_displaced = std::mem::take(&mut self.interior.doc.displaced);
        let mut displaced: Vec<(String, [i32; 3], [i32; 9], Option<String>)> = prev_displaced.iter().filter(|d| clear.contains(&d.1)).cloned().collect();
        let mut remove = rem_prev.clone();
        for (id, c) in self.model.components() {
            if id.body == 0 && clear.contains(&c.voxel) && !generated.contains(&(c.voxel, c.def.clone())) {
                remove.push((c.def.clone(), c.voxel, c.rotation));
                displaced.push((c.def.clone(), c.voxel, c.rotation, c.attrs.get("bc").cloned()));
            }
        }
        let mut parts = b.parts.clone();
        parts.extend(prev_displaced.iter().filter(|d| !clear.contains(&d.1)).map(|(def, vp, r, bc)| GenPart { def: def.clone(), vp: *vp, r: *r, bc: bc.clone(), ac: None }));
        let (r, a, l) = self.model.apply_build_replacing(0, &remove, &rem_links, &parts, &b.links);
        // part settings (fuel type, gear ratios) and microcontroller programs, in the same undo step
        let comps = self.model.components();
        let find = |def: &str, vp: [i32; 3]| comps.iter().find(|(id, c)| id.body == 0 && c.voxel == vp && c.def == def).map(|(id, _)| *id);
        let mut sets = vec![];
        for e in &b.extras {
            let Some(id) = find(&e.def, e.vp) else { continue };
            for (k, v) in &e.attrs {
                sets.push((id, k.as_str(), v.clone()));
            }
            if let Some(xml) = &e.mc {
                if let Ok(doc) = keel_io::xml::parse(xml) {
                    self.model.put_mc_definition(id, doc.root, false);
                }
            }
        }
        self.model.set_attr_each(&sets, false);
        self.interior.doc.generated = b.parts.iter().map(|p| (p.def.clone(), p.vp, p.r)).collect();
        self.interior.doc.generated_links = b.links.iter().map(|l| (l.kind, l.a, l.b)).collect();
        self.interior.doc.displaced = displaced;
        let prop = match &g.layout.propulsion {
            Some(rep) if rep.warnings.is_empty() => "; propulsion installed".to_string(),
            Some(rep) => format!("; propulsion installed with {} warning(s) (see Decks)", rep.warnings.len()),
            None => String::new(),
        };
        self.status = format!("Generated: {a} parts and {l} wires added, {r} parts removed (one undo step); {} lighting circuits{prop}", g.layout.circuits.len());
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
        if self.tool == Tool::Decal {
            let comps = self.model.components();
            let n = comps.iter().filter(|(id, c)| self.selected.contains(id) && decal::SIGN_DEFS.contains(&c.def.as_str())).count();
            let act = self.decal_ui.panel_ui(ui, &mut self.design.doc.decals, n);
            match act {
                Some(decal::Action::New) => self.new_decal(),
                Some(decal::Action::Update(i, new_step)) => {
                    let e = decal::edits(&self.model, &self.design.doc.decals[i], false);
                    self.model.set_attr_each(&e, new_step);
                }
                Some(decal::Action::Remove(i)) => {
                    let e = decal::edits(&self.model, &self.design.doc.decals[i], true);
                    self.model.set_attr_each(&e, true);
                    self.design.doc.decals.remove(i);
                    self.decal_ui.current = None;
                    self.status = "Decal removed; the signs' original pixels are back (one undo step)".into();
                }
                Some(decal::Action::Reattach(i)) => {
                    let sel = self.sel();
                    let old = decal::edits(&self.model, &self.design.doc.decals[i], true);
                    let d0 = self.design.doc.decals[i].clone();
                    match decal::create(&self.model, &sel, &d0.name, d0.png.clone()) {
                        Ok(mut d) => {
                            d.offset = d0.offset;
                            d.scale = d0.scale;
                            d.rotation = d0.rotation;
                            d.flip_x = d0.flip_x;
                            d.flip_y = d0.flip_y;
                            d.opacity = d0.opacity;
                            let mut e = old;
                            e.extend(decal::edits(&self.model, &d, false));
                            self.model.set_attr_each(&e, true);
                            self.design.doc.decals[i] = d;
                        }
                        Err(m) => self.status = m,
                    }
                }
                None => {}
            }
            return;
        }
        if self.tool == Tool::Paint {
            let wl = self.design.hull.as_ref().map(|_| (self.design.doc.hull.draft + self.design.doc.origin[1] as f64 * 0.25) as f32);
            let n = self.selected.len();
            if let Some(paint::Action::ApplyLivery) = paint::panel_ui(ui, &mut self.design.doc.paint, &mut self.tools.colour, wl, n) {
                self.apply_livery(wl);
            }
            return;
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
        if let Some(img) = self.pending_thumb.take() {
            let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            let tex = ctx.load_texture("export_thumb", ci, egui::TextureOptions::LINEAR);
            self.export_thumb = Some((img, tex));
        }
        let Some(d) = self.dialog else { return };
        let mut close = false;
        let title = match d {
            Dialog::Open => "Open vehicle",
            Dialog::SaveAs => "Save / export vehicle XML",
            Dialog::Settings => "Settings",
            Dialog::Scale => "Scale parts",
            Dialog::Welcome => "Welcome to KEEL",
            Dialog::Export => "Export to Stormworks",
            Dialog::Library => "Open from Stormworks",
        };
        egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| match d {
            Dialog::Welcome => {
                ui.set_max_width(460.0);
                ui.label("KEEL reads the parts of your own copy of Stormworks so it can show and check them. It never copies or shares them.");
                ui.add_space(6.0);
                ui.label(RichText::new("KEEL could not find Stormworks automatically. Choose the Stormworks folder (the one Steam installed, usually …/steamapps/common/Stormworks).").color(theme::TEXT_DIM));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Choose the Stormworks folder…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().set_title("Choose the Stormworks folder").pick_folder() {
                            match game::normalise_pick(&p) {
                                Some(defs) => {
                                    self.settings.definitions_dir = Some(defs);
                                    self.settings.save();
                                    self.load_catalogue();
                                    close = true;
                                }
                                None => self.status = format!("No Stormworks parts found in {}. Pick the folder that contains 'rom'.", p.display()),
                            }
                        }
                    }
                    if ui.button("Try again").on_hover_text("Search the Steam libraries again").clicked() {
                        if let Some(g) = game::find_game() {
                            self.settings.definitions_dir = Some(game::definitions_of(&g));
                            self.settings.save();
                            self.load_catalogue();
                            close = true;
                        }
                    }
                    if ui.button("Later").on_hover_text("Blocks only until the game folder is set (File > Settings)").clicked() {
                        close = true;
                    }
                });
            }
            Dialog::Export => {
                ui.set_max_width(560.0);
                ui.horizontal_top(|ui| {
                    if let Some((_, tex)) = &self.export_thumb {
                        ui.image((tex.id(), egui::vec2(220.0, 220.0)));
                    }
                    ui.vertical(|ui| {
                        ui.label("Vehicle name");
                        ui.add(egui::TextEdit::singleline(&mut self.export_name).desired_width(260.0));
                        ui.add_space(6.0);
                        match &self.settings.vehicles_dir {
                            Some(d) => {
                                ui.label(RichText::new(format!("Saves to {}", game::pretty(d))).small().color(theme::TEXT_DIM));
                                let n = game::safe_name(&self.export_name);
                                if d.join(format!("{n}.xml")).exists() {
                                    ui.label(RichText::new(format!("A vehicle called “{n}” exists and will be replaced.")).small().color(theme::ORANGE));
                                }
                            }
                            None => {
                                ui.label(RichText::new("The game's vehicles folder was not found. Start Stormworks once and save any vehicle, or choose the folder.").small().color(theme::ORANGE));
                            }
                        }
                        if ui.small_button("Choose vehicles folder…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().set_title("Choose Stormworks' vehicles folder (…/Stormworks/data/vehicles)").pick_folder() {
                                self.settings.vehicles_dir = Some(p);
                                self.settings.save();
                            }
                        }
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            let ok = self.settings.vehicles_dir.is_some();
                            let go = |launch: bool, this: &mut Self| {
                                if let (Some(dir), Some((thumb, _))) = (this.settings.vehicles_dir.clone(), &this.export_thumb) {
                                    match game::export(&dir, &this.export_name, &this.model.to_xml(), thumb) {
                                        Ok(p) => {
                                            this.status = format!("Exported to Stormworks as “{}”. In the game, open the vehicle editor and load it.", game::safe_name(&this.export_name));
                                            this.settings.push_recent(&p);
                                            if launch {
                                                let _ = game::launch();
                                            }
                                        }
                                        Err(e) => this.status = format!("Export failed: {e}"),
                                    }
                                }
                            };
                            if ui.add_enabled(ok, egui::Button::new(RichText::new("Export").strong())).clicked() {
                                go(false, self);
                                close = true;
                            }
                            if ui.add_enabled(ok, egui::Button::new("Export and launch Stormworks")).clicked() {
                                go(true, self);
                                close = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
                });
            }
            Dialog::Library => {
                ui.set_max_width(720.0);
                let Some(dir) = self.settings.vehicles_dir.clone() else {
                    ui.label("The game's vehicles folder was not found.");
                    if ui.button("Choose vehicles folder…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.settings.vehicles_dir = Some(p);
                            self.settings.save();
                        }
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                    return;
                };
                let list = game::list_vehicles(&dir);
                ui.label(RichText::new(format!("{} saved vehicles in {}", list.len(), game::pretty(&dir))).small().color(theme::TEXT_DIM));
                let mut pick = None;
                egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (name, xml, png) in &list {
                            let tex = self.library_thumbs.entry(xml.clone()).or_insert_with(|| {
                                let img = png.as_ref().and_then(|p| image::open(p).ok())?.thumbnail(128, 128).to_rgba8();
                                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                                Some(ctx.load_texture(format!("lib_{}", xml.display()), ci, egui::TextureOptions::LINEAR))
                            });
                            let (r, resp) = ui.allocate_exact_size(egui::vec2(132.0, 158.0), egui::Sense::click());
                            ui.painter().rect_filled(r, 10.0, if resp.hovered() { theme::SURFACE_HI } else { theme::SURFACE });
                            let ir = egui::Rect::from_min_size(r.min + egui::vec2(6.0, 6.0), egui::vec2(120.0, 120.0));
                            match tex {
                                Some(t) => {
                                    ui.painter().image(t.id(), ir, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                                }
                                None => {
                                    icons::paint(ui.painter(), egui::Rect::from_center_size(ir.center(), egui::vec2(36.0, 36.0)), icons::Icon::Hull, theme::TEXT_DIM);
                                }
                            }
                            let short: String = if name.chars().count() > 17 { name.chars().take(16).collect::<String>() + "…" } else { name.clone() };
                            ui.painter().text(egui::pos2(r.center().x, r.bottom() - 8.0), egui::Align2::CENTER_BOTTOM, short, egui::FontId::proportional(12.0), theme::TEXT);
                            if resp.on_hover_text(name).clicked() {
                                pick = Some(xml.clone());
                            }
                        }
                    });
                });
                if let Some(p) = pick {
                    self.open(p);
                    close = true;
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            }
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
                if ui.button("Choose the Stormworks folder…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        if let Some(d) = game::normalise_pick(&p) {
                            self.defs_input = d.display().to_string();
                        }
                    }
                }
                if ui.button("Find Stormworks automatically").clicked() {
                    if let Some(g) = game::find_game() {
                        self.defs_input = game::definitions_of(&g).display().to_string();
                    }
                }
                ui.label(RichText::new("Stormworks definitions folder (rom/data/definitions). KEEL reads it; it is never copied or bundled.").small().color(theme::TEXT_DIM));
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

impl KeelApp {
    /// Open the export dialog: suggest a name and render the thumbnail.
    fn open_export(&mut self) {
        if self.export_name.is_empty() {
            self.export_name = self
                .path
                .as_ref()
                .map(|p| if p.file_name().is_some_and(|f| f == "vehicle.xml") { p.parent().and_then(|d| d.file_name()).map(|d| d.to_string_lossy().to_string()).unwrap_or_default() } else { p.file_stem().unwrap_or_default().to_string_lossy().to_string() })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "My KEEL boat".into());
        }
        self.refresh();
        let img = game::thumbnail(&self.faces);
        self.export_thumb = None;
        self.pending_thumb = Some(img);
        if self.settings.vehicles_dir.is_none() {
            self.settings.vehicles_dir = game::vehicles_dir();
        }
        self.dialog = Some(Dialog::Export);
    }

    /// `KEEL_SHOT=out.png:60` saves a screenshot after 60 frames and quits (layout regression checks).
    fn debug_shot(&mut self, ctx: &egui::Context) {
        let Ok(spec) = std::env::var("KEEL_SHOT") else { return };
        let (path, n) = spec.rsplit_once(':').map(|(p, n)| (p.to_string(), n.parse().unwrap_or(60))).unwrap_or((spec.clone(), 60));
        self.frames += 1;
        ctx.request_repaint();
        if self.frames == n {
            let c = ctx.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                let [w, h] = img.size;
                let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                    let _ = buf.save(&path);
                }
                c.send_viewport_cmd(egui::ViewportCommand::Close);
                c.request_repaint();
            })));
        }
    }
}

impl eframe::App for KeelApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.debug_shot(&ctx);
        if self.livery_on_start && !self.model.components().is_empty() && !self.bake_on_start {
            self.livery_on_start = false;
            let wl = self.design.hull.as_ref().map(|_| (self.design.doc.hull.draft + self.design.doc.origin[1] as f64 * 0.25) as f32);
            self.apply_livery(wl);
        }
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
            egui::Panel::left("left").frame(theme::side_frame()).show_separator_line(false).default_size(300.0).size_range(240.0..=560.0).show(ui, |ui| theme::card(ui, |ui| match self.mode {
                Mode::Build => self.left_panel(ui),
                Mode::Hull => {
                    self.design.refresh();
                    bake = self.design.params_ui(ui);
                }
                Mode::Power => {
                    self.design.refresh();
                    if std::mem::take(&mut self.power.install_requested) {
                        self.interior.refresh(self.design.hull.as_ref(), &self.design.doc.hull, self.design.doc.origin);
                        self.interior.doc.install_propulsion = true;
                        let (o, sum) = self.power.propulsion();
                        self.interior.propulsion = Some(o);
                        self.interior.propulsion_summary = sum;
                        match self.interior.generate() {
                            Some(g) => generated = Some(g),
                            None => self.status = "Design and bake a hull first (Hull mode): the drive goes into its engine room".into(),
                        }
                    }
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
                    let (o, sum) = self.power.propulsion();
                    self.interior.propulsion = Some(o);
                    self.interior.propulsion_summary = sum;
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
            egui::Panel::right("right").frame(theme::side_frame()).show_separator_line(false).default_size(310.0).size_range(240.0..=560.0).show(ui, |ui| theme::card(ui, |ui| {
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
            let r = viewport::show_with(ui, &mut self.cam, &self.renderer, ghost.as_deref(), self.tool == Tool::Paint);
            if self.tool == Tool::Paint {
                if let (Some((_, started)), Some(h)) = (r.tool_drag, &hit) {
                    if started {
                        self.stroke.clear();
                    }
                    let alt = ui.input(|i| i.modifiers.alt);
                    if !alt {
                        let first = self.stroke.is_empty();
                        self.dab(h.comp, h.point, first);
                    }
                }
                if r.tool_drag_stopped && !self.stroke.is_empty() {
                    self.status = format!("Painted {} parts (one undo step)", self.stroke.len());
                    self.stroke.clear();
                }
            }
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
                    Tool::Select | Tool::Decal => match &hit {
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
                                self.stroke.clear();
                                self.dab(h.comp, h.point, true);
                                self.status = format!("Painted {} part(s)", self.stroke.len());
                                self.stroke.clear();
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
    dialog: Option<String>,
    livery: bool,
    decal_demo: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut a = Args { vehicle: None, defs: None, rom: None, view: None, clean: false, zoom: 1.0, look: None, mode: None, bake: false, plan_docked: false, tool: None, dialog: None, livery: false, decal_demo: None };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        if x == "--defs" {
            a.defs = it.next().map(PathBuf::from);
        } else if x == "--plan-docked" {
            a.plan_docked = true;
        } else if x == "--bake-hull" {
            a.bake = true;
        } else if x == "--decal-demo" {
            a.decal_demo = it.next().map(PathBuf::from);
        } else if x == "--livery" {
            a.livery = true;
        } else if x == "--dialog" {
            a.dialog = it.next();
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
