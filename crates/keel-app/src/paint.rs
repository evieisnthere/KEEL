//! Paint tools: the variation brush (each block gets a stable, seeded jitter of the brush colour, so repainting is
//! repeatable and a whole stroke undoes at once) and the livery generator (hull colour schemes laid out by height and
//! length, applied to the selection or the whole vehicle as one undo step and re-applicable after edits).
use crate::theme;
use eframe::egui::{self, RichText};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Jitter {
    /// lighter / darker only
    Value,
    /// value and saturation
    Weathered,
    /// value, saturation and a little hue
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pattern {
    Solid,
    /// one colour above the waterline, another below
    TwoTone,
    /// topsides, a boot-top band at the waterline, antifouling below
    BootTop,
    /// two-tone plus a stripe a set distance below the sheer
    SheerStripe,
    /// blend from the bottom colour to the top colour with height
    Gradient,
    /// blotches of up to four colours
    Camo,
}

impl Pattern {
    pub const ALL: [(Pattern, &'static str); 6] = [
        (Pattern::Solid, "Solid"),
        (Pattern::TwoTone, "Two-tone"),
        (Pattern::BootTop, "Boot-top"),
        (Pattern::SheerStripe, "Sheer stripe"),
        (Pattern::Gradient, "Gradient"),
        (Pattern::Camo, "Camouflage"),
    ];
}

/// Brush and livery settings, saved in the `.keel.json` sidecar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaintDoc {
    /// 0 = exact colour; 0.25 = ±25 % variation
    pub variation: f32,
    pub jitter: Jitter,
    pub seed: u32,
    /// brush radius in blocks (0 = the part under the cursor only)
    pub radius: f32,
    pub pattern: Pattern,
    /// top, bottom, stripe, fourth colour
    pub colours: [[u8; 3]; 4],
    /// waterline height in metres (vehicle coordinates); None = from the hull design
    pub waterline: Option<f32>,
    /// boot-top / stripe thickness in metres
    pub band: f32,
    /// sheer stripe: distance below the highest hull block, metres
    pub stripe_drop: f32,
    /// camouflage blotch size in metres
    pub camo_scale: f32,
    pub camo_colours: usize,
    pub livery_variation: f32,
    /// only paint hull blocks (blocks, wedges, pyramids), not fittings
    pub blocks_only: bool,
}

impl Default for PaintDoc {
    fn default() -> Self {
        PaintDoc {
            variation: 0.12,
            jitter: Jitter::Weathered,
            seed: 1,
            radius: 0.0,
            pattern: Pattern::BootTop,
            colours: [[0xE8, 0xE8, 0xE4], [0x7A, 0x2A, 0x1E], [0x22, 0x26, 0x3A], [0x5C, 0x6B, 0x4E]],
            waterline: None,
            band: 0.25,
            stripe_drop: 0.75,
            camo_scale: 2.0,
            camo_colours: 3,
            livery_variation: 0.05,
            blocks_only: true,
        }
    }
}

pub fn hex(c: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn hash(seed: u32, v: [i32; 3], k: u32) -> u32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    for x in v {
        h ^= (x as u32).wrapping_mul(0xC2B2_AE35);
        h = h.rotate_left(13).wrapping_mul(5).wrapping_add(0xE654_6B64);
    }
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^ (h >> 15)
}

/// Uniform in [-1, 1], stable for a seed, voxel and channel.
fn rnd(seed: u32, v: [i32; 3], k: u32) -> f32 {
    hash(seed, v, k) as f32 / u32::MAX as f32 * 2.0 - 1.0
}

pub fn rgb_to_hsv(c: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = c.map(|x| x as f32 / 255.0);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let h = if d == 0.0 {
        0.0
    } else if mx == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if mx == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    [h, if mx == 0.0 { 0.0 } else { d / mx }, mx]
}

pub fn hsv_to_rgb([h, s, v]: [f32; 3]) -> [u8; 3] {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r, g, b].map(|t| ((t + m).clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The brush colour varied for one block: the same voxel and seed always give the same result.
pub fn vary(base: [u8; 3], amount: f32, jitter: Jitter, seed: u32, voxel: [i32; 3]) -> [u8; 3] {
    if amount <= 0.0 {
        return base;
    }
    let [h, s, v] = rgb_to_hsv(base);
    let dv = rnd(seed, voxel, 1) * amount;
    let (ds, dh) = match jitter {
        Jitter::Value => (0.0, 0.0),
        Jitter::Weathered => (rnd(seed, voxel, 2) * amount * 0.6, 0.0),
        Jitter::Full => (rnd(seed, voxel, 2) * amount * 0.6, rnd(seed, voxel, 3) * amount * 40.0),
    };
    hsv_to_rgb([h + dh, (s + ds).clamp(0.0, 1.0), (v * (1.0 + dv)).clamp(0.0, 1.0)])
}

/// Smooth 3D value noise in [0, 1].
fn value_noise(seed: u32, p: [f32; 3]) -> f32 {
    let i = p.map(|x| x.floor() as i32);
    let f = p.map(|x| x - x.floor());
    let s = f.map(|t| t * t * (3.0 - 2.0 * t));
    let mut acc = 0.0;
    for dz in 0..2 {
        for dy in 0..2 {
            for dx in 0..2 {
                let w = (if dx == 1 { s[0] } else { 1.0 - s[0] }) * (if dy == 1 { s[1] } else { 1.0 - s[1] }) * (if dz == 1 { s[2] } else { 1.0 - s[2] });
                acc += w * (rnd(seed, [i[0] + dx, i[1] + dy, i[2] + dz], 7) * 0.5 + 0.5);
            }
        }
    }
    acc
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| (a[i] as f32 * (1.0 - t) + b[i] as f32 * t).round() as u8)
}

/// Livery colour of a block at voxel `v` (centre at v × 0.25 m). `top` is the highest hull block's height (m).
pub fn livery(doc: &PaintDoc, v: [i32; 3], waterline: f32, bottom: f32, top: f32) -> [u8; 3] {
    let y = v[1] as f32 * 0.25;
    let [c0, c1, c2, c3] = doc.colours;
    let base = match doc.pattern {
        Pattern::Solid => c0,
        Pattern::TwoTone => {
            if y >= waterline {
                c0
            } else {
                c1
            }
        }
        Pattern::BootTop => {
            if (y - waterline).abs() <= doc.band * 0.5 + 0.01 {
                c2
            } else if y > waterline {
                c0
            } else {
                c1
            }
        }
        Pattern::SheerStripe => {
            let s = top - doc.stripe_drop;
            if (y - s).abs() <= doc.band * 0.5 + 0.01 {
                c2
            } else if y >= waterline {
                c0
            } else {
                c1
            }
        }
        Pattern::Gradient => mix(c1, c0, (y - bottom) / (top - bottom).max(0.25)),
        Pattern::Camo => {
            let p = [v[0] as f32 * 0.25, y, v[2] as f32 * 0.25].map(|x| x / doc.camo_scale.max(0.25));
            let n = value_noise(doc.seed, p) * 0.7 + value_noise(doc.seed ^ 0x51, p.map(|x| x * 2.3)) * 0.3;
            let k = doc.camo_colours.clamp(2, 4);
            [c0, c1, c2, c3][((n * k as f32) as usize).min(k - 1)]
        }
    };
    vary(base, doc.livery_variation, doc.jitter, doc.seed.wrapping_add(17), v)
}

/// What the panel asks the app to do.
pub enum Action {
    ApplyLivery,
}

/// Paint tool options (brush and livery). `default_waterline` comes from the hull design.
pub fn panel_ui(ui: &mut egui::Ui, doc: &mut PaintDoc, brush: &mut [u8; 3], default_waterline: Option<f32>, selected: usize) -> Option<Action> {
    let mut act = None;
    theme::section(ui, "Brush");
    ui.horizontal(|ui| {
        ui.label("Colour");
        ui.color_edit_button_srgb(brush);
        ui.label(RichText::new(hex(*brush)).monospace().small().color(theme::TEXT_DIM));
    });
    ui.add(egui::Slider::new(&mut doc.variation, 0.0..=0.5).text("Variation").custom_formatter(|v, _| format!("{:.0} %", v * 100.0)))
        .on_hover_text("Each block gets its own slightly different shade while you paint. 0 % paints the exact colour.");
    ui.horizontal(|ui| {
        for (j, n, tip) in [(Jitter::Value, "Shade", "Lighter and darker only"), (Jitter::Weathered, "Weathered", "Shade and saturation"), (Jitter::Full, "Full", "Shade, saturation and a little hue")] {
            if ui.selectable_label(doc.jitter == j, n).on_hover_text(tip).clicked() {
                doc.jitter = j;
            }
        }
    });
    ui.add(egui::Slider::new(&mut doc.radius, 0.0..=6.0).step_by(0.5).text("Brush size (blocks)"));
    ui.horizontal(|ui| {
        ui.label("Seed");
        ui.add(egui::DragValue::new(&mut doc.seed));
        if ui.small_button("New").on_hover_text("A different random variation").clicked() {
            doc.seed = doc.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345) % 100_000;
        }
    });
    ui.label(RichText::new("Click or drag over the vehicle to paint. Each stroke is one undo step. Alt+click picks a colour; middle-drag orbits.").small().color(theme::TEXT_DIM));
    ui.separator();
    theme::section(ui, "Livery");
    egui::ComboBox::from_id_salt("livery_pattern").selected_text(Pattern::ALL.iter().find(|p| p.0 == doc.pattern).map(|p| p.1).unwrap_or("")).show_ui(ui, |ui| {
        for (p, n) in Pattern::ALL {
            ui.selectable_value(&mut doc.pattern, p, n);
        }
    });
    let names: &[&str] = match doc.pattern {
        Pattern::Solid => &["Colour"],
        Pattern::TwoTone => &["Topsides", "Bottom"],
        Pattern::BootTop => &["Topsides", "Bottom", "Boot-top"],
        Pattern::SheerStripe => &["Topsides", "Bottom", "Stripe"],
        Pattern::Gradient => &["Top", "Bottom"],
        Pattern::Camo => &["Colour 1", "Colour 2", "Colour 3", "Colour 4"],
    };
    let shown = if doc.pattern == Pattern::Camo { doc.camo_colours.clamp(2, 4) } else { names.len() };
    ui.horizontal_wrapped(|ui| {
        for (i, n) in names.iter().enumerate().take(shown) {
            ui.vertical(|ui| {
                ui.label(RichText::new(*n).small());
                ui.color_edit_button_srgb(&mut doc.colours[i]);
            });
        }
    });
    if matches!(doc.pattern, Pattern::TwoTone | Pattern::BootTop | Pattern::SheerStripe) {
        let mut auto = doc.waterline.is_none();
        ui.horizontal(|ui| {
            if ui.checkbox(&mut auto, "Waterline from hull design").changed() {
                doc.waterline = if auto { None } else { Some(default_waterline.unwrap_or(0.5)) };
            }
        });
        if let Some(w) = &mut doc.waterline {
            ui.add(egui::Slider::new(w, -5.0..=10.0).step_by(0.25).text("Waterline (m)"));
        } else {
            ui.label(RichText::new(match default_waterline { Some(w) => format!("Waterline {w:.2} m"), None => "No hull design: waterline at 0 m".into() }).small().color(theme::TEXT_DIM));
        }
    }
    if matches!(doc.pattern, Pattern::BootTop | Pattern::SheerStripe) {
        ui.add(egui::Slider::new(&mut doc.band, 0.25..=1.5).step_by(0.25).text("Band (m)"));
    }
    if doc.pattern == Pattern::SheerStripe {
        ui.add(egui::Slider::new(&mut doc.stripe_drop, 0.25..=3.0).step_by(0.25).text("Below sheer (m)"));
    }
    if doc.pattern == Pattern::Camo {
        ui.add(egui::Slider::new(&mut doc.camo_scale, 0.5..=8.0).text("Blotch size (m)"));
        ui.add(egui::Slider::new(&mut doc.camo_colours, 2..=4).text("Colours"));
    }
    ui.add(egui::Slider::new(&mut doc.livery_variation, 0.0..=0.3).text("Variation").custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
    ui.checkbox(&mut doc.blocks_only, "Hull blocks only (leave fittings)");
    let target = if selected > 0 { format!("Apply to {selected} selected parts") } else { "Apply to the whole vehicle".into() };
    if ui.add(egui::Button::new(RichText::new(target).strong()).fill(theme::SURFACE_HI)).on_hover_text("One undo step. Change the settings and apply again at any time.").clicked() {
        act = Some(Action::ApplyLivery);
    }
    act
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variation_is_stable_and_bounded() {
        let c = [200, 60, 40];
        let a = vary(c, 0.2, Jitter::Full, 7, [1, 2, 3]);
        assert_eq!(a, vary(c, 0.2, Jitter::Full, 7, [1, 2, 3]));
        assert_eq!(vary(c, 0.0, Jitter::Full, 7, [1, 2, 3]), c);
        let distinct: std::collections::HashSet<_> = (0..50).map(|i| vary(c, 0.2, Jitter::Value, 7, [i, 0, 0])).collect();
        assert!(distinct.len() > 10);
        for x in [[0, 0, 0], [255, 255, 255], [12, 200, 99]] {
            let back = hsv_to_rgb(rgb_to_hsv(x));
            assert!((0..3).all(|i| (back[i] as i32 - x[i] as i32).abs() <= 1), "{x:?} {back:?}");
        }
        let d = PaintDoc { livery_variation: 0.0, ..Default::default() };
        assert_eq!(livery(&d, [0, 20, 0], 1.0, 0.0, 6.0), d.colours[0]);
        assert_eq!(livery(&d, [0, 4, 0], 1.0, 0.0, 6.0), d.colours[2]);
        assert_eq!(livery(&d, [0, 0, 0], 1.0, 0.0, 6.0), d.colours[1]);
    }
}
