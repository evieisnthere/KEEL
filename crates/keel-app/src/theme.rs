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

/// Frame for side panels: panel fill, generous margins (document-editor feel).
pub fn side_frame() -> egui::Frame {
    egui::Frame::NONE.fill(PANEL).inner_margin(Margin::symmetric(12, 10)).stroke(Stroke::new(1.0, LINE))
}

/// Frame for the title/menu/toolbar rows.
pub fn bar_frame() -> egui::Frame {
    egui::Frame::NONE.fill(BG).inner_margin(Margin::symmetric(12, 6))
}

/// Rounded "pill" container for the toolbar.
pub fn pill() -> egui::Frame {
    egui::Frame::NONE.fill(SURFACE).corner_radius(CornerRadius::same(18)).inner_margin(Margin::symmetric(10, 4))
}

/// Section heading in side panels.
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text.to_uppercase()).small().color(TEXT_DIM).strong());
    ui.add_space(2.0);
}

/// Toolbar toggle button styled as a pill segment; returns true when clicked.
pub fn tool_button(ui: &mut egui::Ui, active: bool, text: &str, tip: &str) -> bool {
    let rt = egui::RichText::new(text).color(if active { BG } else { TEXT });
    let b = egui::Button::new(rt).fill(if active { CYAN } else { Color32::TRANSPARENT }).corner_radius(CornerRadius::same(14));
    ui.add(b).on_hover_text(tip).clicked()
}
