//! Part palette: the game's categories, a thumbnail grid (parts rendered from their real geometry), search,
//! favourites and recently used parts.
use crate::theme;
use eframe::egui::{self, Color32, ColorImage, RichText, TextureHandle, Vec2};
use keel_model::catalogue::Catalogue;
use keel_model::geometry::{build_faces_with, MeshLibrary};
use keel_model::Model;
use std::collections::{BTreeSet, HashMap};

/// The game editor's part categories (definition `category`).
pub const CATEGORIES: [&str; 16] = [
    "Blocks",
    "Vehicle control",
    "Mechanics",
    "Propulsion",
    "Specialist",
    "Logic",
    "Displays",
    "Sensors",
    "Decorative",
    "Fluid",
    "Electric",
    "Jet engines",
    "Weapons",
    "Modular engines",
    "Industry",
    "Windows",
];

const THUMB: usize = 72;

pub struct Palette {
    pub category: Option<usize>,
    pub search: String,
    pub favourites: BTreeSet<String>,
    pub recent: Vec<String>,
    thumbs: HashMap<String, Option<TextureHandle>>,
    pub tile: f32,
}

impl Default for Palette {
    fn default() -> Self {
        Palette { category: Some(0), search: String::new(), favourites: BTreeSet::new(), recent: vec![], thumbs: HashMap::new(), tile: 76.0 }
    }
}

/// Render a part to a small image: isometric view, depth-buffered, flat shaded.
pub fn render_thumb(def: &str, cat: &Catalogue, meshes: &mut MeshLibrary) -> Option<ColorImage> {
    let mut m = Model::new_empty();
    m.add(0, def, [0, 0, 0], keel_io::vehicle::IDENTITY)?;
    let faces = build_faces_with(&m, cat, meshes);
    if faces.is_empty() {
        return None;
    }
    // isometric camera: yaw 35°, pitch 30°
    let (cy, sy) = (35f32.to_radians().cos(), 35f32.to_radians().sin());
    let (cp, sp) = (30f32.to_radians().cos(), 30f32.to_radians().sin());
    let proj = |p: [f32; 3]| -> [f32; 3] {
        let x = p[0] * cy - p[2] * sy;
        let z0 = p[0] * sy + p[2] * cy;
        let y = p[1] * cp - z0 * sp;
        let z = p[1] * sp + z0 * cp;
        [x, y, z]
    };
    let pts: Vec<[f32; 3]> = faces.iter().flat_map(|f| f.verts.iter().map(|v| proj(*v))).collect();
    let (x0, x1) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
    let (y0, y1) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
    let s = (THUMB as f32 - 8.0) / (x1 - x0).max(y1 - y0).max(1e-3);
    let (ox, oy) = (THUMB as f32 / 2.0 - (x0 + x1) / 2.0 * s, THUMB as f32 / 2.0 + (y0 + y1) / 2.0 * s);
    let mut img = ColorImage::new([THUMB, THUMB], vec![Color32::TRANSPARENT; THUMB * THUMB]);
    let mut zb = vec![f32::MIN; THUMB * THUMB];
    let light = [0.45f32, 0.8, 0.4];
    for f in &faces {
        let n = proj(f.normal);
        // view direction is +z in projected space (towards the viewer)
        if !f.two_sided && n[2] < 0.0 {
            continue;
        }
        let sh = 0.45 + 0.55 * (f.normal[0] * light[0] + f.normal[1] * light[1] + f.normal[2] * light[2]).abs().min(1.0);
        let c = f.colour;
        let col = Color32::from_rgba_unmultiplied((c[0] as f32 * sh) as u8, (c[1] as f32 * sh) as u8, (c[2] as f32 * sh) as u8, c[3].max(140));
        let v: Vec<[f32; 3]> = f.verts.iter().map(|p| {
            let q = proj(*p);
            [q[0] * s + ox, oy - q[1] * s, q[2]]
        }).collect();
        for k in 1..v.len().saturating_sub(1) {
            let (a, b, cc) = (v[0], v[k], v[k + 1]);
            let bx0 = a[0].min(b[0]).min(cc[0]).floor().max(0.0) as usize;
            let bx1 = (a[0].max(b[0]).max(cc[0]).ceil() as usize).min(THUMB - 1);
            let by0 = a[1].min(b[1]).min(cc[1]).floor().max(0.0) as usize;
            let by1 = (a[1].max(b[1]).max(cc[1]).ceil() as usize).min(THUMB - 1);
            let d = (b[1] - cc[1]) * (a[0] - cc[0]) + (cc[0] - b[0]) * (a[1] - cc[1]);
            if d.abs() < 1e-6 {
                continue;
            }
            for y in by0..=by1 {
                for x in bx0..=bx1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let l1 = ((b[1] - cc[1]) * (px - cc[0]) + (cc[0] - b[0]) * (py - cc[1])) / d;
                    let l2 = ((cc[1] - a[1]) * (px - cc[0]) + (a[0] - cc[0]) * (py - cc[1])) / d;
                    let l3 = 1.0 - l1 - l2;
                    if l1 < -1e-4 || l2 < -1e-4 || l3 < -1e-4 {
                        continue;
                    }
                    let z = l1 * a[2] + l2 * b[2] + l3 * cc[2];
                    let i = y * THUMB + x;
                    if z > zb[i] {
                        zb[i] = z;
                        img.pixels[i] = col;
                    }
                }
            }
        }
    }
    Some(img)
}

impl Palette {
    pub fn used(&mut self, def: &str) {
        self.recent.retain(|d| d != def);
        self.recent.insert(0, def.to_string());
        self.recent.truncate(12);
    }

    /// The palette panel. Returns the part chosen for placement.
    pub fn ui(&mut self, ui: &mut egui::Ui, cat: &Catalogue, meshes: &mut MeshLibrary, current: &str) -> Option<String> {
        let mut chosen = None;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
            crate::icons::paint(ui.painter(), r, crate::icons::Icon::Search, theme::TEXT_DIM);
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search parts").desired_width(f32::INFINITY));
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let label = match self.category {
                Some(i) => CATEGORIES[i].to_string(),
                None => "Favourites".to_string(),
            };
            egui::ComboBox::from_id_salt("palette_cat").selected_text(label).width((ui.available_width() - 56.0).max(80.0)).height(420.0).show_ui(ui, |ui| {
                for (i, c) in CATEGORIES.iter().enumerate() {
                    if ui.selectable_label(self.category == Some(i), *c).clicked() {
                        self.category = Some(i);
                        self.search.clear();
                    }
                }
            });
            if crate::icons::button(ui, self.category.is_none(), crate::icons::Icon::Star, "", "Favourites (right-click a part to add it)").clicked() {
                self.category = None;
                self.search.clear();
            }
        });
        ui.separator();
        if !self.recent.is_empty() && self.search.is_empty() {
            ui.label(RichText::new("Recent").small().color(theme::TEXT_DIM));
            let rec = self.recent.clone();
            if let Some(d) = self.tiles(ui, cat, meshes, &rec, current, "recent") {
                chosen = Some(d);
            }
            ui.separator();
        }
        let q = self.search.to_lowercase();
        let ids: Vec<String> = cat
            .defs
            .iter()
            .filter(|(id, d)| {
                if !q.is_empty() {
                    return id.to_lowercase().contains(&q) || d.name.to_lowercase().contains(&q) || d.tags.to_lowercase().contains(&q);
                }
                match self.category {
                    Some(c) => d.category == c as i32,
                    None => self.favourites.contains(*id),
                }
            })
            .map(|(id, _)| id.clone())
            .collect();
        if ids.is_empty() {
            ui.label(RichText::new(if self.category.is_none() { "Right-click a part to add it to your favourites." } else { "No parts match." }).color(theme::TEXT_DIM));
        }
        egui::ScrollArea::vertical().id_salt("palette_grid").show(ui, |ui| {
            if let Some(d) = self.tiles(ui, cat, meshes, &ids, current, "grid") {
                chosen = Some(d);
            }
        });
        if let Some(d) = &chosen {
            self.used(d);
        }
        chosen
    }

    fn tiles(&mut self, ui: &mut egui::Ui, cat: &Catalogue, meshes: &mut MeshLibrary, ids: &[String], current: &str, salt: &str) -> Option<String> {
        let mut chosen = None;
        // leave room for the scroll bar so the grid never asks for more width than the panel has
        let per_row = ((ui.available_width() - 14.0) / (self.tile + 4.0)).floor().max(1.0) as usize;
        // a few new thumbnails per frame keep the UI responsive
        let mut budget = 6;
        egui::Grid::new(format!("tiles_{salt}")).spacing(Vec2::new(4.0, 4.0)).show(ui, |ui| {
            for (i, id) in ids.iter().enumerate() {
                if !self.thumbs.contains_key(id) && budget > 0 {
                    budget -= 1;
                    let tex = render_thumb(id, cat, meshes).map(|img| ui.ctx().load_texture(format!("thumb_{id}"), img, egui::TextureOptions::LINEAR));
                    self.thumbs.insert(id.clone(), tex);
                }
                let name = cat.get(id).map(|d| d.name.clone()).unwrap_or_else(|| id.clone());
                let sel = current == id;
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(self.tile, self.tile + 16.0), egui::Sense::click());
                let bg = if sel { theme::SURFACE_HI } else if resp.hovered() { theme::SURFACE } else { theme::PANEL };
                ui.painter().rect_filled(rect, 8.0, bg);
                if sel {
                    ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.5, theme::CYAN), egui::StrokeKind::Inside);
                }
                let img_rect = egui::Rect::from_min_size(rect.min + Vec2::new(4.0, 2.0), Vec2::splat(self.tile - 8.0));
                match self.thumbs.get(id) {
                    Some(Some(t)) => {
                        ui.painter().image(t.id(), img_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    _ => {
                        crate::icons::paint(ui.painter(), egui::Rect::from_center_size(img_rect.center(), Vec2::splat(24.0)), crate::icons::Icon::Place, theme::TEXT_DIM);
                    }
                }
                if self.favourites.contains(id) {
                    crate::icons::paint(ui.painter(), egui::Rect::from_min_size(rect.right_top() + Vec2::new(-17.0, 3.0), Vec2::splat(13.0)), crate::icons::Icon::Star, theme::ORANGE);
                }
                let short: String = if name.chars().count() > 12 { name.chars().take(11).collect::<String>() + "…" } else { name.clone() };
                ui.painter().text(egui::pos2(rect.center().x, rect.bottom() - 3.0), egui::Align2::CENTER_BOTTOM, short, egui::FontId::proportional(10.5), theme::TEXT);
                let resp = resp.on_hover_text(format!("{name}\n{id}"));
                if resp.clicked() {
                    chosen = Some(id.clone());
                }
                if resp.secondary_clicked() && !self.favourites.remove(id) {
                    self.favourites.insert(id.clone());
                }
                if (i + 1) % per_row == 0 {
                    ui.end_row();
                }
            }
        });
        chosen
    }
}
