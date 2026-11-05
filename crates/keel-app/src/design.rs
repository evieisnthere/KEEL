//! Hull design mode: parametric realistic hull, live hydrostatics, NURBS preview and baking to Stormworks blocks.
use crate::theme;
use eframe::egui::{self, RichText};
use keel_geom::hull::{hydrostatics, Hull, HullParams, Hydrostatics};
use keel_geom::voxel::Placement;
use keel_model::geometry::Face;
use keel_model::CompId;
use serde::{Deserialize, Serialize};

/// Everything about the design that is not in the vehicle file. Saved as `<vehicle>.keel.json` next to the vehicle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DesignDoc {
    pub hull: HullParams,
    /// voxel position of the hull's midship baseline in the vehicle
    pub origin: [i32; 3],
    pub shell: u32,
    pub deck: bool,
    pub topside: String,
    pub bottom: String,
    /// placements written by the last bake (removed again on re-bake)
    pub baked: Vec<Placement>,
    /// interior design (Decks mode)
    pub interior: crate::interior::InteriorDoc,
    /// powertrain design (Power mode)
    pub power: crate::power::PowerDoc,
    /// paint brush and livery settings
    pub paint: crate::paint::PaintDoc,
}

impl Default for DesignDoc {
    fn default() -> Self {
        DesignDoc { hull: HullParams::default(), origin: [0, 0, 0], shell: 1, deck: true, topside: "E8E8E4".into(), bottom: "7A2A1E".into(), baked: vec![], interior: Default::default(), power: Default::default(), paint: Default::default() }
    }
}

pub struct Design {
    pub doc: DesignDoc,
    pub hull: Option<Hull>,
    pub hydro: Option<Hydrostatics>,
    pub preview: Vec<Face>,
    pub show_preview: bool,
    pub generation: u64,
    built_for: Option<HullParams>,
    pub status: String,
}

impl Default for Design {
    fn default() -> Self {
        Design { doc: DesignDoc::default(), hull: None, hydro: None, preview: vec![], show_preview: true, generation: 1, built_for: None, status: String::new() }
    }
}

/// Result of a bake, applied to the model by the caller in one undo step.
pub struct Bake {
    pub remove: Vec<Placement>,
    pub add: Vec<(Placement, String)>,
}

fn slider(ui: &mut egui::Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, unit: &str, tip: &str) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized([118.0, 20.0], egui::Label::new(RichText::new(label).color(theme::TEXT_DIM))).on_hover_text(tip);
        ui.add(egui::Slider::new(v, range).suffix(unit).max_decimals(2)).changed()
    })
    .inner
}

impl Design {
    pub fn load_for(path: &std::path::Path) -> DesignDoc {
        std::fs::read_to_string(sidecar(path)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save_for(&self, path: &std::path::Path) {
        if let Ok(s) = serde_json::to_string_pretty(&self.doc) {
            let _ = std::fs::write(sidecar(path), s);
        }
    }

    /// Rebuild the NURBS hull, hydrostatics and preview when parameters changed.
    pub fn refresh(&mut self) {
        if self.built_for.as_ref() == Some(&self.doc.hull) {
            return;
        }
        self.hull = self.doc.hull.build();
        self.hydro = self.hull.as_ref().map(|h| hydrostatics(h, self.doc.hull.draft));
        self.preview = self.hull.as_ref().map(|h| preview_faces(h, self.doc.origin)).unwrap_or_default();
        self.built_for = Some(self.doc.hull.clone());
        self.generation += 1;
    }

    /// Left panel: hull parameters.
    pub fn params_ui(&mut self, ui: &mut egui::Ui) -> Option<Bake> {
        let mut bake = None;
        theme::section(ui, "Hull");
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.doc.hull.name);
        });
        egui::ScrollArea::vertical().id_salt("hullparams").show(ui, |ui| {
            let p = &mut self.doc.hull;
            theme::section(ui, "Main dimensions");
            slider(ui, "Length", &mut p.length, 3.0..=120.0, " m", "Overall length");
            slider(ui, "Beam", &mut p.beam, 1.0..=30.0, " m", "Maximum beam at deck");
            slider(ui, "Depth", &mut p.depth, 0.5..=15.0, " m", "Baseline to sheer at midship");
            let dmax = p.depth;
            slider(ui, "Draft", &mut p.draft, 0.1..=dmax, " m", "Design waterline above baseline");
            theme::section(ui, "Sections");
            slider(ui, "Deadrise", &mut p.deadrise_deg, 0.0..=35.0, "°", "Bottom V angle at midship (0 = flat)");
            slider(ui, "Bow deadrise", &mut p.bow_deadrise_deg, 0.0..=60.0, "°", "Deadrise at the stem: finer entry");
            slider(ui, "Bilge radius", &mut p.bilge, 0.0..=0.95, "", "Fraction of half-beam; 0 = hard chine");
            slider(ui, "Flare", &mut p.flare_deg, -10.0..=35.0, "°", "Topside flare from vertical");
            theme::section(ui, "Ends");
            slider(ui, "Entrance", &mut p.entrance, 0.1..=0.8, "", "Fraction of length over which the bow tapers");
            slider(ui, "Bow fullness", &mut p.entrance_fullness, 0.5..=6.0, "", "1 = fine, 3+ = full/bluff");
            slider(ui, "Forefoot", &mut p.forefoot, 0.0..=0.95, "", "Keel rise at the bow (fraction of depth)");
            slider(ui, "Run", &mut p.run, 0.0..=0.6, "", "Fraction of length over which the stern tapers");
            slider(ui, "Transom width", &mut p.transom_width, 0.0..=1.0, "", "Transom half-beam / max");
            slider(ui, "Transom lift", &mut p.transom_lift, 0.0..=1.2, "", "Transom bottom height / draft");
            theme::section(ui, "Sheer");
            slider(ui, "Sheer at bow", &mut p.sheer_bow, -1.0..=5.0, " m", "Deck rise at the bow");
            slider(ui, "Sheer at stern", &mut p.sheer_stern, -1.0..=3.0, " m", "Deck rise at the stern");
            ui.horizontal(|ui| {
                ui.label(RichText::new("Stations").color(theme::TEXT_DIM));
                ui.add(egui::DragValue::new(&mut p.stations).range(5..=81));
            });
            ui.horizontal_wrapped(|ui| {
                for (name, preset) in presets() {
                    if ui.button(name).clicked() {
                        let keep = p.name.clone();
                        *p = preset;
                        p.name = keep;
                    }
                }
            });
            let _ = p;

            theme::section(ui, "Blocks");
            ui.horizontal(|ui| {
                ui.label("Origin (voxels)");
                for v in self.doc.origin.iter_mut() {
                    ui.add(egui::DragValue::new(v).speed(0.2));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Shell");
                ui.add(egui::DragValue::new(&mut self.doc.shell).range(0..=8).suffix(" vox"));
                ui.label(RichText::new("0 = solid").small().color(theme::TEXT_DIM));
            });
            ui.checkbox(&mut self.doc.deck, "Deck plate at the sheer");
            ui.horizontal(|ui| {
                ui.label("Topside");
                ui.add(egui::TextEdit::singleline(&mut self.doc.topside).desired_width(70.0));
                ui.label("Bottom");
                ui.add(egui::TextEdit::singleline(&mut self.doc.bottom).desired_width(70.0));
            });
            ui.checkbox(&mut self.show_preview, "Show NURBS surface");
            ui.add_space(6.0);
            let label = if self.doc.baked.is_empty() { "Bake hull to blocks" } else { "Re-bake hull (replaces previous)" };
            if ui.add(egui::Button::new(RichText::new(label).strong()).fill(theme::CYAN.gamma_multiply(0.55))).clicked() {
                bake = self.bake();
            }
            ui.label(
                RichText::new("Lattice builder: side and bottom are additive height fields quantised to block gradients; every tile is one wedge or pyramid/inverse-pyramid pair, so faces are planar and continuous. One-voxel shell. Standard blocks only, no XML scale edits.")
                    .small()
                    .color(theme::TEXT_DIM),
            );
            if !self.status.is_empty() {
                ui.label(RichText::new(&self.status).small().color(theme::CYAN));
            }
        });
        bake
    }

    pub fn bake_now(&mut self) -> Option<Bake> {
        self.bake()
    }

    fn bake(&mut self) -> Option<Bake> {
        let hull = self.hull.as_ref()?;
        let t = std::time::Instant::now();
        let o = self.doc.origin;
        let built = keel_geom::hullblocks::build(hull, &keel_geom::hullblocks::HullBlockOptions { origin: o, deck: self.doc.deck });
        let placed = built.placements;
        let wl = (self.doc.hull.draft / keel_geom::voxel::VOXEL).round() as i32 + o[1];
        let add = placed.iter().map(|p| (p.clone(), if p.vp[1] < wl { self.doc.bottom.clone() } else { self.doc.topside.clone() })).collect::<Vec<_>>();
        let remove = std::mem::replace(&mut self.doc.baked, placed);
        self.status = format!("Baked {} blocks in {:.1} s", add.len(), t.elapsed().as_secs_f32());
        Some(Bake { remove, add })
    }

    /// Right panel: hydrostatics readout.
    pub fn hydro_ui(&self, ui: &mut egui::Ui) {
        let p = &self.doc.hull;
        // the game refuses vehicles larger than its editor volumes (RE-0003 load-time validation): about 58 m long
        if p.length > 58.25 || p.beam > 23.5 {
            ui.label(RichText::new(format!("Too large for the game: {:.1} × {:.1} m. Vehicles must fit about 58 m long and 23.5 m wide (RE-0003); split the ship or scale it down.", p.length, p.beam)).color(theme::RED));
        }
        theme::section(ui, "Hydrostatics (real world, sea water)");
        let Some(h) = &self.hydro else {
            ui.label(RichText::new("Hull could not be built from these parameters").color(theme::ORANGE));
            return;
        };
        egui::Grid::new("hydro").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
            let mut row = |k: &str, v: String| {
                ui.label(RichText::new(k).color(theme::TEXT_DIM));
                ui.label(v);
                ui.end_row();
            };
            row("Draft", format!("{:.2} m", h.draft));
            row("Displacement", format!("{:.1} t", h.displacement_t));
            row("Volume", format!("{:.1} m³", h.volume_m3));
            row("LWL × BWL", format!("{:.2} × {:.2} m", h.waterline_length, h.waterline_beam));
            row("Block coeff. Cb", format!("{:.3}", h.block_coefficient));
            row("Waterplane", format!("{:.1} m²", h.waterplane_m2));
            row("LCB", format!("{:+.2} m", h.lcb));
            row("KB", format!("{:.2} m", h.kb));
            row("BM", format!("{:.2} m", h.bm));
            row("KM", format!("{:.2} m", h.km));
        });
        ui.label(
            RichText::new("GM = KM − KG. Stormworks uses different buoyancy scaling; the in-game waterline is an estimate (OPEN-ITEMS A1).")
                .small()
                .color(theme::TEXT_DIM),
        );
    }
}

fn sidecar(path: &std::path::Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".keel.json");
    s.into()
}

/// Presets spanning common real hull families (realistic by default).
pub fn presets() -> Vec<(&'static str, HullParams)> {
    keel_geom::hull::presets()
}

/// Translucent preview of the NURBS surface (both halves), in vehicle metres at the design origin.
fn preview_faces(hull: &Hull, origin: [i32; 3]) -> Vec<Face> {
    let (pts, tris) = hull.surface.tessellate(28, 72);
    let o = origin.map(|v| v as f64 * keel_geom::voxel::VOXEL);
    // the voxel grid is centred on voxel centres: hull baseline y = 0 lies on the bottom face of voxel row 0
    let off = [o[0], o[1] - 0.125, o[2]];
    let mut out = Vec::with_capacity(tris.len() * 2);
    let id = CompId { body: usize::MAX, comp: usize::MAX };
    for side in [1.0, -1.0] {
        for t in &tris {
            let v: Vec<[f32; 3]> = t.iter().map(|&i| { let p = pts[i as usize]; [(p[0] * side + off[0]) as f32, (p[1] + off[1]) as f32, (p[2] + off[2]) as f32] }).collect();
            let a = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
            let b = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
            let n = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if l < 1e-9 {
                continue;
            }
            out.push(Face { comp: id, verts: v, normal: [n[0] / l, n[1] / l, n[2] / l], colour: [63, 214, 230, 90], two_sided: true });
        }
    }
    out
}
