//! KEEL visual identity (PROJECT_PLAN §20): deep navy/purple-blue surfaces, blue-grey panels, cyan selection and
//! active tools, white linework, orange warnings. Layout follows a document-editor pattern: title row, menu row,
//! rounded pill toolbar, side panels and a dominant canvas.
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Visuals};

pub const BG: Color32 = Color32::from_rgb(0x12, 0x13, 0x2b); // app background (deep navy-purple)
pub const CANVAS: Color32 = Color32::from_rgb(0x0e, 0x10, 0x24); // viewport background
pub const PANEL: Color32 = Color32::from_rgb(0x1b, 0x1e, 0x3d); // side panels
pub const SURFACE: Color32 = Color32::from_rgb(0x25, 0x29, 0x52); // inputs, buttons
pub const SURFACE_HI: Color32 = Color32::from_rgb(0x31, 0x36, 0x6b); // hover
pub const LINE: Color32 = Color32::from_rgb(0x3a, 0x40, 0x78); // separators
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xea, 0xf6);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xc8);
pub const CYAN: Color32 = Color32::from_rgb(0x3f, 0xd6, 0xe6); // selection / active tool / measurement
pub const ORANGE: Color32 = Color32::from_rgb(0xff, 0x9a, 0x3c); // warnings
pub const RED: Color32 = Color32::from_rgb(0xff, 0x5c, 0x6c); // errors
pub const GRID: Color32 = Color32::from_rgba_premultiplied(0x30, 0x3a, 0x70, 0x60);

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = SURFACE;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = CYAN;
    v.warn_fg_color = ORANGE;
    v.error_fg_color = RED;
    v.selection.bg_fill = Color32::from_rgb(0x1f, 0x6f, 0x86);
    v.selection.stroke = Stroke::new(1.0, CYAN);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_shadow.color = Color32::from_black_alpha(120);
    let r = CornerRadius::same(8);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.weak_bg_fill = PANEL;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    v.widgets.noninteractive.corner_radius = r;
    for (w, bg) in [(&mut v.widgets.inactive, SURFACE), (&mut v.widgets.hovered, SURFACE_HI), (&mut v.widgets.active, Color32::from_rgb(0x1f, 0x6f, 0x86)), (&mut v.widgets.open, SURFACE_HI)] {
        w.bg_fill = bg;
        w.weak_bg_fill = bg;
        w.corner_radius = r;
        w.fg_stroke = Stroke::new(1.0, TEXT);
        w.bg_stroke = Stroke::NONE;
    }
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.active.bg_stroke = Stroke::new(1.0, CYAN);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.window_margin = Margin::same(14);
        s.spacing.menu_margin = Margin::same(8);
        s.spacing.interact_size.y = 26.0;
        use egui::{FontId, TextStyle};
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(17.0));
        s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
    });
}

/// Frame for side panels: the app background around a floating card (see [`card`]).
pub fn side_frame() -> egui::Frame {
    egui::Frame::NONE.fill(BG).inner_margin(Margin { left: 10, right: 10, top: 0, bottom: 10 })
}

/// A rounded card filling the panel, document-editor style.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::NONE
        .fill(PANEL)
        .corner_radius(CornerRadius::same(16))
        .stroke(Stroke::new(1.0, LINE.gamma_multiply(0.6)))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            // height only: a minimum width equal to the available width would grow the panel by the card's margins
            // every frame
            ui.set_min_height(ui.available_height());
            add(ui)
        })
        .inner
}

/// The central area: background margin so the canvas floats like a page.
pub fn centre_frame() -> egui::Frame {
    egui::Frame::NONE.fill(BG).inner_margin(Margin { left: 0, right: 0, top: 0, bottom: 10 })
}

/// Mask the corners of a canvas (GL viewport or painter canvas) so it reads as a rounded card.
pub fn round_corners(p: &egui::Painter, r: egui::Rect) {
    let rad = 16.0;
    let n = 8;
    for (c, a0) in [(r.left_top() + egui::vec2(rad, rad), 180.0f32), (r.right_top() + egui::vec2(-rad, rad), 270.0), (r.right_bottom() + egui::vec2(-rad, -rad), 0.0), (r.left_bottom() + egui::vec2(rad, -rad), 90.0)] {
        let corner = egui::pos2(c.x + rad * (a0 + 45.0).to_radians().cos() * std::f32::consts::SQRT_2, c.y + rad * (a0 + 45.0).to_radians().sin() * std::f32::consts::SQRT_2);
        for i in 0..n {
            let t0 = (a0 + 90.0 * i as f32 / n as f32).to_radians();
            let t1 = (a0 + 90.0 * (i + 1) as f32 / n as f32).to_radians();
            let pts = vec![corner, egui::pos2(c.x + rad * t0.cos(), c.y + rad * t0.sin()), egui::pos2(c.x + rad * t1.cos(), c.y + rad * t1.sin())];
            p.add(egui::Shape::convex_polygon(pts, BG, Stroke::NONE));
        }
    }
}

/// The KEEL mark: a rounded tile with a hull profile and keel line.
pub fn logo(p: &egui::Painter, r: egui::Rect) {
    p.rect_filled(r.shrink(2.0), 10.0, Color32::from_rgb(0x2b, 0x2f, 0x6e));
    let s = r.width() / 40.0;
    let at = |x: f32, y: f32| r.min + egui::vec2(x * s, y * s);
    p.add(egui::Shape::convex_polygon(vec![at(8.0, 17.0), at(33.0, 17.0), at(28.0, 26.0), at(12.0, 26.0)], CYAN, Stroke::NONE));
    p.line_segment([at(20.0, 26.0), at(20.0, 32.0)], Stroke::new(2.4 * s, CYAN));
    p.line_segment([at(14.0, 12.5), at(26.0, 12.5)], Stroke::new(2.0 * s, TEXT));
}

/// Frame for the title/menu/toolbar rows.
pub fn bar_frame() -> egui::Frame {
    egui::Frame::NONE.fill(BG).inner_margin(Margin::symmetric(12, 6))
}

/// Rounded "pill" container for the toolbar.
pub fn pill() -> egui::Frame {
    egui::Frame::NONE.fill(SURFACE).corner_radius(CornerRadius::same(18)).inner_margin(Margin::symmetric(10, 4))
}

/// Section heading in side panels (sentence case, document style).
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text).size(13.5).color(TEXT).strong());
    ui.add_space(2.0);
}
