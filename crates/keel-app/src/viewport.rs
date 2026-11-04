//! 3D block viewport: orbit camera, painter's-algorithm flat-shaded polygons (egui mesh), picking.
//! A GPU renderer (wgpu) replaces this when vehicles outgrow it; the interface stays the same.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke};
use keel_model::geometry::Face;
use keel_model::CompId;
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub struct Camera {
    pub target: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { target: [0.0; 3], yaw: 0.8, pitch: 0.45, dist: 20.0 }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt().max(1e-9);
    [a[0] / l, a[1] / l, a[2] / l]
}

pub struct View {
    pub eye: [f32; 3],
    r: [f32; 3],
    u: [f32; 3],
    d: [f32; 3],
    focal: f32,
    centre: Pos2,
}

impl Camera {
    pub fn view(&self, rect: Rect) -> View {
        let dir = [self.pitch.cos() * self.yaw.sin(), self.pitch.sin(), self.pitch.cos() * self.yaw.cos()];
        let eye = [self.target[0] + dir[0] * self.dist, self.target[1] + dir[1] * self.dist, self.target[2] + dir[2] * self.dist];
        let d = norm(sub(self.target, eye));
        let r = norm(cross(d, [0.0, 1.0, 0.0]));
        let u = cross(r, d);
        View { eye, r, u, d, focal: rect.height().min(rect.width()) * 1.2, centre: rect.center() }
    }
    pub fn frame(&mut self, faces: &[Face]) {
        if faces.is_empty() {
            return;
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for f in faces {
            for v in &f.verts {
                for i in 0..3 {
                    lo[i] = lo[i].min(v[i]);
                    hi[i] = hi[i].max(v[i]);
                }
            }
        }
        self.target = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let ext = dot(sub(hi, lo), sub(hi, lo)).sqrt();
        self.dist = (ext * 1.1).max(2.0);
    }
}

impl View {
    /// camera-space (x right, y up, z forward)
    pub fn cam(&self, p: [f32; 3]) -> [f32; 3] {
        let q = sub(p, self.eye);
        [dot(q, self.r), dot(q, self.u), dot(q, self.d)]
    }
    pub fn project(&self, p: [f32; 3]) -> Option<(Pos2, f32)> {
        let c = self.cam(p);
        (c[2] > 0.05).then(|| (Pos2::new(self.centre.x + c[0] / c[2] * self.focal, self.centre.y - c[1] / c[2] * self.focal), c[2]))
    }
}

pub struct Hit {
    pub comp: CompId,
    pub point: [f32; 3],
    /// face normal oriented toward the camera
    pub normal: [f32; 3],
}

fn point_in_poly(p: Pos2, poly: &[Pos2]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub fn pick(view: &View, faces: &[Face], p: Pos2) -> Option<Hit> {
    let mut best: Option<(f32, usize)> = None;
    for (k, f) in faces.iter().enumerate() {
        let pr: Option<Vec<(Pos2, f32)>> = f.verts.iter().map(|v| view.project(*v)).collect();
        let Some(pr) = pr else { continue };
        let pts: Vec<Pos2> = pr.iter().map(|x| x.0).collect();
        if point_in_poly(p, &pts) {
            let z = pr.iter().map(|x| x.1).sum::<f32>() / pr.len() as f32;
            if best.is_none_or(|(bz, _)| z < bz) {
                best = Some((z, k));
            }
        }
    }
    best.map(|(_, k)| {
        let f = &faces[k];
        let n = f.verts.len() as f32;
        let c = [0, 1, 2].map(|i| f.verts.iter().map(|v| v[i]).sum::<f32>() / n);
        let n = if dot(f.normal, sub(view.eye, c)) >= 0.0 { f.normal } else { [-f.normal[0], -f.normal[1], -f.normal[2]] };
        Hit { comp: f.comp, point: c, normal: n }
    })
}

pub struct Response {
    pub clicked: Option<(Pos2, egui::Modifiers)>,
}

/// Draw the faces and handle camera input. Returns click/hover positions for the caller's tools.
pub fn show(ui: &mut egui::Ui, cam: &mut Camera, faces: &[Face], selected: &HashSet<CompId>, ghost: Option<&[Face]>) -> Response {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 34, 44));

    // camera input: left-drag orbit, right/middle-drag pan, wheel zoom
    if resp.dragged_by(egui::PointerButton::Primary) {
        let d = resp.drag_delta();
        cam.yaw -= d.x * 0.008;
        cam.pitch = (cam.pitch + d.y * 0.008).clamp(-1.55, 1.55);
    }
    if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
        let d = resp.drag_delta();
        let v = cam.view(rect);
        let s = cam.dist / v.focal;
        for i in 0..3 {
            cam.target[i] += (-d.x * v.r[i] + d.y * v.u[i]) * s;
        }
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            cam.dist = (cam.dist * (-scroll * 0.002).exp()).clamp(0.5, 5000.0);
        }
    }
    let view = cam.view(rect);
    let light = norm([0.35, 0.85, 0.4]);

    let mut order: Vec<(f32, usize, bool)> = Vec::with_capacity(faces.len());
    let all = faces.iter().map(|f| (f, false)).chain(ghost.unwrap_or(&[]).iter().map(|f| (f, true)));
    let list: Vec<(&Face, bool)> = all.collect();
    for (k, (f, _)) in list.iter().enumerate() {
        let mut z = 0.0;
        let mut ok = true;
        for v in &f.verts {
            let c = view.cam(*v);
            if c[2] <= 0.05 {
                ok = false;
                break;
            }
            z += c[2];
        }
        if ok {
            order.push((z / f.verts.len() as f32, k, list[k].1));
        }
    }
    order.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut mesh = egui::Mesh::default();
    let mut outlines: Vec<Vec<Pos2>> = Vec::new();
    for (_, k, is_ghost) in order {
        let f = list[k].0;
        let pts: Vec<Pos2> = f.verts.iter().filter_map(|v| view.project(*v).map(|x| x.0)).collect();
        if pts.len() < 3 {
            continue;
        }
        let shade = 0.38 + 0.62 * dot(f.normal, light).abs();
        let sel = selected.contains(&f.comp);
        let base = if is_ghost { [120, 220, 140] } else if sel { [255, 170, 60] } else { f.colour };
        let a = if is_ghost { 150 } else { 255 };
        let col = Color32::from_rgba_unmultiplied(
            (base[0] as f32 * shade) as u8,
            (base[1] as f32 * shade) as u8,
            (base[2] as f32 * shade) as u8,
            a,
        );
        let i0 = mesh.vertices.len() as u32;
        for p in &pts {
            mesh.colored_vertex(*p, col);
        }
        for t in 1..pts.len() as u32 - 1 {
            mesh.add_triangle(i0, i0 + t, i0 + t + 1);
        }
        if sel {
            outlines.push(pts);
        }
        if mesh.vertices.len() > 60_000 {
            painter.add(egui::Shape::mesh(std::mem::take(&mut mesh)));
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    for o in outlines {
        painter.add(egui::Shape::closed_line(o, Stroke::new(1.0, Color32::from_rgb(255, 230, 160))));
    }
    // axis gizmo
    let g = rect.left_bottom() + egui::vec2(40.0, -40.0);
    for (axis, col, name) in [([1.0, 0.0, 0.0], Color32::from_rgb(230, 80, 80), "X"), ([0.0, 1.0, 0.0], Color32::from_rgb(90, 220, 90), "Y"), ([0.0, 0.0, 1.0], Color32::from_rgb(90, 140, 255), "Z")] {
        let e = egui::vec2(dot(axis, view.r), -dot(axis, view.u)) * 28.0;
        painter.line_segment([g, g + e], Stroke::new(2.0, col));
        painter.text(g + e * 1.2, egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(12.0), col);
    }
    let clicked = if resp.clicked() { resp.interact_pointer_pos().map(|p| (p, ui.input(|i| i.modifiers))) } else { None };
    Response { clicked }
}
