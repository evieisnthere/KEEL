//! Decals: images placed onto panels of paintable signs and indicators. A sign face is a 9×9 pixel grid stored in
//! the part's `gc` attribute; pixel rows follow the part's local +Z and columns its local +X (inferred from seams
//! between neighbouring signs in the corpus, OPEN-ITEMS B5). A decal keeps its image, the signs it covers, their
//! original pixels and its transform in the design sidecar, so it can be moved, scaled, rotated, flipped or removed
//! at any time; each change re-composites over the original pixels as one undo step.
use crate::icons::{self, Icon};
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use keel_model::{CompId, Model};
use serde::{Deserialize, Serialize};

pub const SIGN_DEFS: [&str; 2] = ["sign_na", "sign"];
const N: usize = 9;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Decal {
    pub name: String,
    /// PNG bytes, base64 (downscaled to at most 512 px)
    pub png: String,
    pub body: usize,
    /// voxel positions of the signs, and their pixels before the decal
    pub signs: Vec<[i32; 3]>,
    pub base: Vec<Vec<String>>,
    /// shared rotation of the signs
    pub rot: [i32; 9],
    /// centre offset from the panel centre, in sign pixels
    pub offset: [f32; 2],
    /// image width as a fraction of the panel width
    pub scale: f32,
    pub rotation: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub opacity: f32,
}

impl Default for Decal {
    fn default() -> Self {
        Decal { name: String::new(), png: String::new(), body: 0, signs: vec![], base: vec![], rot: keel_io::vehicle::IDENTITY, offset: [0.0, 0.0], scale: 1.0, rotation: 0.0, flip_x: false, flip_y: false, opacity: 1.0 }
    }
}

// ---- base64 (no extra dependency) ---------------------------------------------------------------------------------
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn b64_decode(s: &str) -> Vec<u8> {
    let mut val = [255u8; 256];
    for (i, c) in B64.iter().enumerate() {
        val[*c as usize] = i as u8;
    }
    let mut out = vec![];
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = val[c as usize];
        if v == 255 {
            continue;
        }
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xFF) as u8);
        }
    }
    out
}

/// Load an image file for a decal: downscale to at most 512 px and encode as PNG.
pub fn load_image(path: &std::path::Path) -> Result<(String, image::RgbaImage), String> {
    let img = image::open(path).map_err(|e| e.to_string())?;
    let img = if img.width().max(img.height()) > 512 { img.resize(512, 512, image::imageops::FilterType::Lanczos3) } else { img };
    let rgba = img.to_rgba8();
    let mut buf = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut buf, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok((b64_encode(buf.get_ref()), rgba))
}

pub fn decode(d: &Decal) -> Option<image::RgbaImage> {
    image::load_from_memory(&b64_decode(&d.png)).ok().map(|i| i.to_rgba8())
}

/// Panel layout: for each sign its (x, z) cell in the panel, and the panel size in cells.
pub fn layout(signs: &[[i32; 3]], rot: &[i32; 9]) -> (Vec<[i32; 2]>, [i32; 2]) {
    // local = Rᵀ (v − v0); column-major R: M[i][j] = r[3j + i]
    let v0 = signs.first().copied().unwrap_or([0; 3]);
    let local: Vec<[i32; 2]> = signs
        .iter()
        .map(|v| {
            let d = [v[0] - v0[0], v[1] - v0[1], v[2] - v0[2]];
            let lx: i32 = (0..3).map(|i| rot[i] * d[i]).sum();
            let lz: i32 = (0..3).map(|i| rot[6 + i] * d[i]).sum();
            [lx, lz]
        })
        .collect();
    let minx = local.iter().map(|l| l[0]).min().unwrap_or(0);
    let minz = local.iter().map(|l| l[1]).min().unwrap_or(0);
    let maxx = local.iter().map(|l| l[0]).max().unwrap_or(0);
    let maxz = local.iter().map(|l| l[1]).max().unwrap_or(0);
    (local.iter().map(|l| [l[0] - minx, l[1] - minz]).collect(), [maxx - minx + 1, maxz - minz + 1])
}

fn parse_hex(s: &str) -> [u8; 3] {
    let v = u32::from_str_radix(s.trim().get(..6).unwrap_or("FFFFFF"), 16).unwrap_or(0xFFFFFF);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

fn sample(img: &image::RgbaImage, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (img.width() as f32, img.height() as f32);
    if u < 0.0 || v < 0.0 || u >= w || v >= h {
        return [0.0; 4];
    }
    let p = img.get_pixel(u as u32, v as u32).0;
    [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32 / 255.0]
}

/// Composite the decal over the signs' original pixels: per sign, 81 hex colours (row-major, rows along local +Z).
/// Each sign pixel averages a 4×4 grid of image samples.
pub fn rasterize(d: &Decal, img: &image::RgbaImage) -> Vec<Vec<String>> {
    let (cells, size) = layout(&d.signs, &d.rot);
    let (pw, ph) = ((size[0] as usize * N) as f32, (size[1] as usize * N) as f32);
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let width = (pw * d.scale).max(0.5);
    let height = width * ih / iw.max(1.0);
    let (ca, sa) = ((-d.rotation).to_radians().cos(), (-d.rotation).to_radians().sin());
    let centre = [pw / 2.0 + d.offset[0], ph / 2.0 + d.offset[1]];
    let mut out = vec![];
    for (k, cell) in cells.iter().enumerate() {
        let base = d.base.get(k).cloned().unwrap_or_else(|| vec!["FFFFFF".into(); N * N]);
        let mut px = Vec::with_capacity(N * N);
        for row in 0..N {
            for col in 0..N {
                // panel pixel; panel "up" (image top) is local +Z, i.e. the highest row
                let x = (cell[0] as usize * N + col) as f32;
                let y = (cell[1] as usize * N + row) as f32;
                let mut acc = [0.0f32; 4];
                let ss = 4;
                for sy in 0..ss {
                    for sx in 0..ss {
                        let p = [x + (sx as f32 + 0.5) / ss as f32 - centre[0], (ph - (y + (sy as f32 + 0.5) / ss as f32)) - (ph - centre[1])];
                        let mut q = [p[0] * ca - p[1] * sa, p[0] * sa + p[1] * ca];
                        if d.flip_x {
                            q[0] = -q[0];
                        }
                        if d.flip_y {
                            q[1] = -q[1];
                        }
                        let u = (q[0] / width + 0.5) * iw;
                        let v = (q[1] / height + 0.5) * ih;
                        let s = sample(img, u, v);
                        for i in 0..3 {
                            acc[i] += s[i] * s[3];
                        }
                        acc[3] += s[3];
                    }
                }
                let n = (ss * ss) as f32;
                let a = (acc[3] / n * d.opacity).clamp(0.0, 1.0);
                let b = parse_hex(base.get(row * N + col).map(|s| s.as_str()).unwrap_or("FFFFFF"));
                let c: [u8; 3] = [0, 1, 2].map(|i| {
                    let fg = if acc[3] > 0.0 { acc[i] / acc[3] } else { 0.0 };
                    (b[i] as f32 * (1.0 - a) + fg * a).round().clamp(0.0, 255.0) as u8
                });
                px.push(format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
            }
        }
        out.push(px);
    }
    out
}

/// The current pixels of a sign component (81 colours), or white.
pub fn pixels_of(model: &Model, id: CompId) -> Vec<String> {
    model
        .component_element(id)
        .and_then(|c| c.find("o"))
        .and_then(|o| o.get("gc"))
        .map(|g| g.split(',').map(|s| s.to_string()).collect::<Vec<_>>())
        .filter(|v| v.len() == N * N)
        .unwrap_or_else(|| vec!["FFFFFF".into(); N * N])
}

/// Sign components of a decal, in the decal's order (None if a sign was removed).
pub fn sign_ids(model: &Model, d: &Decal) -> Vec<Option<CompId>> {
    let comps = model.components();
    d.signs.iter().map(|v| comps.iter().find(|(id, c)| id.body == d.body && c.voxel == *v && SIGN_DEFS.contains(&c.def.as_str())).map(|(id, _)| *id)).collect()
}

/// `gc` edits that show the decal (or, with `restore`, the original pixels).
pub fn edits(model: &Model, d: &Decal, restore: bool) -> Vec<(CompId, &'static str, String)> {
    let ids = sign_ids(model, d);
    let pix = if restore { d.base.clone() } else { decode(d).map(|img| rasterize(d, &img)).unwrap_or_default() };
    ids.iter().zip(pix).filter_map(|(id, p)| id.map(|id| (id, "gc", p.join(",")))).collect()
}

/// Build a new decal on the selected signs. Errors explain what to select.
pub fn create(model: &Model, selected: &[CompId], name: &str, png: String) -> Result<Decal, String> {
    let comps = model.components();
    let signs: Vec<_> = comps.iter().filter(|(id, c)| selected.contains(id) && SIGN_DEFS.contains(&c.def.as_str())).collect();
    if signs.is_empty() {
        return Err("Select paintable signs or indicators first (tip: Fill a box with Paintable Signs to make a panel)".into());
    }
    let rot = signs[0].1.rotation;
    let body = signs[0].0.body;
    if signs.iter().any(|(id, c)| c.rotation != rot || id.body != body) {
        return Err("All signs of a decal must face the same way, on the same body".into());
    }
    Ok(Decal {
        name: name.into(),
        png,
        body,
        signs: signs.iter().map(|(_, c)| c.voxel).collect(),
        base: signs.iter().map(|(id, _)| pixels_of(model, *id)).collect(),
        rot,
        ..Default::default()
    })
}

/// What the panel asks the app to do.
pub enum Action {
    /// pick an image and make a decal on the selected signs
    New,
    /// re-composite decal `i`; `new_step` is false while a slider is being dragged
    Update(usize, bool),
    /// remove decal `i` (restoring the original pixels)
    Remove(usize),
    /// move decal `i` onto the selected signs
    Reattach(usize),
}

#[derive(Default)]
pub struct DecalUi {
    pub current: Option<usize>,
    dragging: bool,
    preview: Option<(usize, u64, egui::TextureHandle)>,
    /// image cache: decal index and image bytes length
    images: std::collections::HashMap<usize, (usize, image::RgbaImage)>,
}

impl DecalUi {
    fn image(&mut self, i: usize, d: &Decal) -> Option<&image::RgbaImage> {
        let fresh = self.images.get(&i).is_none_or(|c| c.0 != d.png.len());
        if fresh {
            let img = decode(d)?;
            self.images.insert(i, (d.png.len(), img));
        }
        self.images.get(&i).map(|c| &c.1)
    }

    pub fn panel_ui(&mut self, ui: &mut egui::Ui, decals: &mut [Decal], signs_selected: usize) -> Option<Action> {
        let mut act = None;
        theme::section(ui, "Decals");
        ui.label(RichText::new("Put images on paintable signs and indicators. Select the signs (click, Shift+click), then add an image. Decals stay editable.").small().color(theme::TEXT_DIM));
        ui.add_space(4.0);
        let add = ui.add_enabled(signs_selected > 0, egui::Button::new(RichText::new(format!("Add image to {signs_selected} selected sign(s)…")).strong()).fill(theme::SURFACE_HI));
        if add.on_disabled_hover_text("Select paintable signs first. A box Fill of Paintable Signs makes a panel.").clicked() {
            act = Some(Action::New);
        }
        ui.add_space(6.0);
        for (i, d) in decals.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.current == Some(i), format!("{}  ·  {} signs", if d.name.is_empty() { "Decal" } else { &d.name }, d.signs.len())).clicked() {
                    self.current = Some(i);
                }
                if icons::button(ui, false, Icon::Trash, "", "Remove this decal (restores the signs' pixels)").clicked() {
                    act = Some(Action::Remove(i));
                }
            });
        }
        let Some(i) = self.current.filter(|i| *i < decals.len()) else { return act };
        ui.separator();
        let d = &mut decals[i];
        let before = d.clone();
        let mut dragging = false;
        ui.add(egui::TextEdit::singleline(&mut d.name).hint_text("Name"));
        let (_, size) = layout(&d.signs, &d.rot);
        let (pw, ph) = (size[0] as f32 * N as f32, size[1] as f32 * N as f32);
        for r in [
            ui.add(egui::Slider::new(&mut d.offset[0], -pw..=pw).text("Move across")),
            ui.add(egui::Slider::new(&mut d.offset[1], -ph..=ph).text("Move up")),
            ui.add(egui::Slider::new(&mut d.scale, 0.05..=4.0).logarithmic(true).text("Size")),
            ui.add(egui::Slider::new(&mut d.rotation, -180.0..=180.0).text("Rotate °")),
            ui.add(egui::Slider::new(&mut d.opacity, 0.0..=1.0).text("Opacity")),
        ] {
            dragging |= r.dragged();
        }
        ui.horizontal(|ui| {
            ui.toggle_value(&mut d.flip_x, "Flip H");
            ui.toggle_value(&mut d.flip_y, "Flip V");
            for (deg, n) in [(-90.0, "−90°"), (90.0, "+90°")] {
                if ui.button(n).clicked() {
                    d.rotation = ((d.rotation + deg + 180.0).rem_euclid(360.0)) - 180.0;
                }
            }
            if ui.button("Fit").on_hover_text("Centre and fill the panel width").clicked() {
                d.offset = [0.0, 0.0];
                d.scale = 1.0;
                d.rotation = 0.0;
            }
        });
        if ui.add_enabled(signs_selected > 0, egui::Button::new("Move to selected signs")).on_hover_text("Re-attach this decal to the signs you have selected").clicked() {
            act = Some(Action::Reattach(i));
        }
        // preview of the panel pixels
        let d = decals[i].clone();
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            format!("{:?}{:?}{}{}{}{}{}", d.offset, d.signs, d.scale, d.rotation, d.flip_x, d.flip_y, d.opacity).hash(&mut h);
            d.png.len().hash(&mut h);
            h.finish()
        };
        if self.preview.as_ref().is_none_or(|p| p.0 != i || p.1 != key) {
            if let Some(img) = self.image(i, &d).cloned() {
                let pix = rasterize(&d, &img);
                let (cells, size) = layout(&d.signs, &d.rot);
                let (w, h) = (size[0] as usize * N, size[1] as usize * N);
                let mut ci = egui::ColorImage::new([w, h], vec![Color32::from_rgb(0x0e, 0x10, 0x24); w * h]);
                for (k, cell) in cells.iter().enumerate() {
                    for row in 0..N {
                        for col in 0..N {
                            let c = parse_hex(&pix[k][row * N + col]);
                            let (x, y) = (cell[0] as usize * N + col, h - 1 - (cell[1] as usize * N + row));
                            ci.pixels[y * w + x] = Color32::from_rgb(c[0], c[1], c[2]);
                        }
                    }
                }
                let tex = ui.ctx().load_texture("decal_preview", ci, egui::TextureOptions::NEAREST);
                self.preview = Some((i, key, tex));
            }
        }
        if let Some((_, _, tex)) = &self.preview {
            let s = tex.size_vec2();
            let k = (ui.available_width() / s.x).min(200.0 / s.y).max(1.0);
            ui.add_space(4.0);
            ui.label(RichText::new("Preview (as painted on the signs)").small().color(theme::TEXT_DIM));
            ui.image((tex.id(), s * k));
        }
        if decals[i] != before || (self.dragging && !dragging) {
            act = Some(Action::Update(i, !self.dragging));
        }
        self.dragging = dragging;
        act
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base64_round_trip_and_layout() {
        for s in [&b""[..], b"a", b"ab", b"abc", b"hello world!!"] {
            assert_eq!(b64_decode(&b64_encode(s)), s);
        }
        // three signs in a row along local +X (identity), one above along +Z
        let (cells, size) = layout(&[[0, 0, 0], [1, 0, 0], [2, 0, 0], [0, 0, 1]], &keel_io::vehicle::IDENTITY);
        assert_eq!(size, [3, 2]);
        assert_eq!(cells, vec![[0, 0], [1, 0], [2, 0], [0, 1]]);
        // a red image covering a white panel fully
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]));
        let d = Decal { signs: vec![[0, 0, 0]], base: vec![vec!["FFFFFF".into(); 81]], scale: 2.0, ..Default::default() };
        let px = rasterize(&d, &img);
        assert_eq!(px[0][40], "FF0000");
        let half = Decal { opacity: 0.5, ..d };
        assert_eq!(rasterize(&half, &img)[0][40], "FF8080");
    }
}
