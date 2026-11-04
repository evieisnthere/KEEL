//! Vector icons drawn with the painter (no icon font needed): consistent 1.6 px line icons in the toolbar style.
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Icon {
    Select,
    Place,
    Paint,
    Fill,
    Measure,
    Wire,
    Mirror,
    Frame,
    Undo,
    Redo,
    Search,
    Star,
    Chip,
    Trash,
}

/// Draw `icon` into `rect` (square) with colour `c`.
pub fn paint(p: &egui::Painter, rect: Rect, icon: Icon, c: Color32) {
    let s = rect.width().min(rect.height());
    let o = rect.center() - Vec2::splat(s / 2.0);
    // coordinates on a 16 x 16 grid
    let at = |x: f32, y: f32| Pos2::new(o.x + x / 16.0 * s, o.y + y / 16.0 * s);
    let k = Stroke::new(1.6 * s / 16.0, c);
    let line = |pts: &[(f32, f32)]| p.add(egui::Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), k));
    let closed = |pts: &[(f32, f32)]| p.add(egui::Shape::closed_line(pts.iter().map(|&(x, y)| at(x, y)).collect(), k));
    match icon {
        Icon::Select => {
            p.add(egui::Shape::convex_polygon(vec![at(3.5, 2.0), at(12.5, 8.5), at(8.2, 9.3), at(5.8, 13.5)], c, Stroke::NONE));
            line(&[(8.4, 9.4), (11.0, 14.0)]);
        }
        Icon::Place => {
            // isometric cube with a plus
            closed(&[(8.0, 3.0), (13.0, 5.5), (13.0, 11.0), (8.0, 13.5), (3.0, 11.0), (3.0, 5.5)]);
            line(&[(3.0, 5.5), (8.0, 8.0), (13.0, 5.5)]);
            line(&[(8.0, 8.0), (8.0, 13.5)]);
        }
        Icon::Paint => {
            // brush: handle and bristles
            line(&[(13.5, 2.5), (7.5, 8.5)]);
            p.add(egui::Shape::convex_polygon(vec![at(6.0, 8.0), at(8.0, 10.0), at(5.5, 13.5), at(2.5, 13.5), at(2.5, 11.5)], c, Stroke::NONE));
        }
        Icon::Fill => {
            closed(&[(2.5, 2.5), (13.5, 2.5), (13.5, 13.5), (2.5, 13.5)]);
            line(&[(6.2, 2.5), (6.2, 13.5)]);
            line(&[(9.8, 2.5), (9.8, 13.5)]);
            line(&[(2.5, 6.2), (13.5, 6.2)]);
            line(&[(2.5, 9.8), (13.5, 9.8)]);
        }
        Icon::Measure => {
            closed(&[(1.5, 10.5), (10.5, 1.5), (14.5, 5.5), (5.5, 14.5)]);
            for (x, y) in [(4.5, 7.5), (7.0, 5.0), (9.5, 2.5)] {
                line(&[(x, y), (x + 1.8, y + 1.8)]);
            }
        }
        Icon::Wire => {
            p.circle_filled(at(3.5, 12.5), 1.9 * s / 16.0, c);
            p.circle_stroke(at(12.5, 3.5), 1.9 * s / 16.0, k);
            p.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
                [at(3.5, 12.5), at(9.0, 12.5), at(7.0, 3.5), at(12.5, 3.5)],
                false,
                Color32::TRANSPARENT,
                k,
            )));
        }
        Icon::Mirror => {
            p.add(egui::Shape::dashed_line(&[at(8.0, 1.5), at(8.0, 14.5)], k, 1.6 * s / 16.0, 1.4 * s / 16.0));
            closed(&[(6.0, 4.0), (6.0, 12.0), (1.5, 12.0)]);
            p.add(egui::Shape::convex_polygon(vec![at(10.0, 4.0), at(14.5, 12.0), at(10.0, 12.0)], c, Stroke::NONE));
        }
        Icon::Frame => {
            for (x, y, dx, dy) in [(2.5, 2.5, 1.0, 1.0), (13.5, 2.5, -1.0, 1.0), (2.5, 13.5, 1.0, -1.0), (13.5, 13.5, -1.0, -1.0)] {
                line(&[(x + 3.5 * dx, y), (x, y), (x, y + 3.5 * dy)]);
            }
            p.circle_filled(at(8.0, 8.0), 1.6 * s / 16.0, c);
        }
        Icon::Undo | Icon::Redo => {
            let f = if icon == Icon::Undo { 1.0 } else { -1.0 };
            let x = |v: f32| 8.0 + (v - 8.0) * f;
            line(&[(x(3.0), 7.0), (x(10.0), 7.0), (x(12.0), 8.0), (x(13.0), 10.0), (x(12.0), 12.0), (x(10.0), 13.0), (x(6.0), 13.0)]);
            line(&[(x(6.0), 4.0), (x(3.0), 7.0), (x(6.0), 10.0)]);
        }
        Icon::Search => {
            p.circle_stroke(at(7.0, 7.0), 4.3 * s / 16.0, k);
            line(&[(10.2, 10.2), (14.0, 14.0)]);
        }
        Icon::Star => {
            let pts: Vec<Pos2> = (0..10)
                .map(|i| {
                    let a = std::f32::consts::PI * (i as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
                    let r = if i % 2 == 0 { 6.5 } else { 2.8 };
                    at(8.0 + r * a.cos(), 8.5 + r * a.sin())
                })
                .collect();
            p.add(egui::Shape::closed_line(pts, k));
        }
        Icon::Chip => {
            closed(&[(4.0, 4.0), (12.0, 4.0), (12.0, 12.0), (4.0, 12.0)]);
            for v in [6.0, 8.0, 10.0] {
                line(&[(v, 1.5), (v, 4.0)]);
                line(&[(v, 12.0), (v, 14.5)]);
                line(&[(1.5, v), (4.0, v)]);
                line(&[(12.0, v), (14.5, v)]);
            }
        }
        Icon::Trash => {
            line(&[(2.5, 4.0), (13.5, 4.0)]);
            line(&[(6.0, 4.0), (6.5, 2.0), (9.5, 2.0), (10.0, 4.0)]);
            closed(&[(4.0, 4.0), (12.0, 4.0), (11.0, 14.0), (5.0, 14.0)]);
        }
    }
}

/// An icon-and-label pill segment button. Returns the response (clicked etc.).
pub fn button(ui: &mut egui::Ui, active: bool, icon: Icon, text: &str, tip: &str) -> egui::Response {
    let font = egui::FontId::proportional(14.0);
    let fg = if active { crate::theme::BG } else { crate::theme::TEXT };
    let galley = (!text.is_empty()).then(|| ui.painter().layout_no_wrap(text.to_string(), font, fg));
    let w = 10.0 + 16.0 + galley.as_ref().map(|g| 6.0 + g.size().x).unwrap_or(0.0) + 10.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), egui::Sense::click());
    let bg = if active {
        crate::theme::CYAN
    } else if resp.hovered() {
        crate::theme::SURFACE_HI
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 14.0, bg);
    let ir = Rect::from_center_size(Pos2::new(rect.left() + 18.0, rect.center().y), Vec2::splat(16.0));
    paint(ui.painter(), ir, icon, fg);
    if let Some(g) = galley {
        ui.painter().galley(Pos2::new(ir.right() + 6.0, rect.center().y - g.size().y / 2.0), g, fg);
    }
    resp.on_hover_text(tip)
}
