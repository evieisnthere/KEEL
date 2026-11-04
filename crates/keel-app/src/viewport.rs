//! 3D viewport: orbit camera, OpenGL depth-buffered rendering (via an egui paint callback), CPU picking.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke};
use eframe::egui_glow;
use eframe::glow::{self, HasContext};
use keel_model::geometry::Face;
use keel_model::CompId;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

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

#[derive(Clone, Copy)]
pub struct View {
    pub eye: [f32; 3],
    r: [f32; 3],
    u: [f32; 3],
    d: [f32; 3],
    focal: f32,
    centre: Pos2,
    size: [f32; 2],
    near: f32,
    far: f32,
}

impl Camera {
    pub fn view(&self, rect: Rect) -> View {
        let dir = [self.pitch.cos() * self.yaw.sin(), self.pitch.sin(), self.pitch.cos() * self.yaw.cos()];
        let eye = [self.target[0] + dir[0] * self.dist, self.target[1] + dir[1] * self.dist, self.target[2] + dir[2] * self.dist];
        let d = norm(sub(self.target, eye));
        let r0 = norm(cross(d, [0.0, 1.0, 0.0]));
        let u = cross(r0, d);
        // Stormworks' world is left-handed (X to starboard when Z is forward and Y up): mirror the screen's x axis
        let r = [-r0[0], -r0[1], -r0[2]];
        View {
            eye,
            r,
            u,
            d,
            focal: rect.height().min(rect.width()) * 1.2,
            centre: rect.center(),
            size: [rect.width(), rect.height()],
            near: (self.dist * 0.002).max(0.02),
            far: self.dist * 20.0 + 500.0,
        }
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
        self.dist = (ext * 1.05).max(2.0);
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
        (c[2] > self.near).then(|| (Pos2::new(self.centre.x + c[0] / c[2] * self.focal, self.centre.y - c[1] / c[2] * self.focal), c[2]))
    }
    /// Column-major clip matrix matching `project` exactly.
    fn clip_matrix(&self) -> [f32; 16] {
        let (sx, sy) = (self.focal / (self.size[0] / 2.0), self.focal / (self.size[1] / 2.0));
        let (n, f) = (self.near, self.far);
        let (a, b) = ((f + n) / (f - n), -2.0 * f * n / (f - n));
        // rows of view (world → camera): r, u, d with translation -eye
        let t = [-dot(self.r, self.eye), -dot(self.u, self.eye), -dot(self.d, self.eye)];
        let rows = [
            [sx * self.r[0], sx * self.r[1], sx * self.r[2], sx * t[0]],
            [sy * self.u[0], sy * self.u[1], sy * self.u[2], sy * t[1]],
            [a * self.d[0], a * self.d[1], a * self.d[2], a * t[2] + b],
            [self.d[0], self.d[1], self.d[2], t[2]],
        ];
        let mut m = [0.0; 16];
        for (i, row) in rows.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                m[j * 4 + i] = *v;
            }
        }
        m
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

// ---------------------------------------------------------------------------------------------------------------------
// GPU renderer

const VS: &str = r#"
in vec3 a_pos; in vec3 a_nrm; in vec4 a_col;
uniform mat4 u_clip;
out vec3 v_nrm; out vec4 v_col; out vec3 v_pos;
void main() { v_nrm = a_nrm; v_col = a_col; v_pos = a_pos; gl_Position = u_clip * vec4(a_pos, 1.0); }
"#;
const FS: &str = r#"
precision mediump float;
in vec3 v_nrm; in vec4 v_col; in vec3 v_pos;
uniform vec3 u_eye;
out vec4 o_col;
void main() {
    vec3 n = normalize(v_nrm);
    vec3 view = normalize(u_eye - v_pos);
    bool one_sided = length(v_nrm) > 0.75;           // block faces carry unit outward normals; meshes half-length
    if (dot(n, view) < 0.0) { if (one_sided) discard; n = -n; }
    vec3 l1 = normalize(vec3(0.35, 0.85, 0.40));
    vec3 l2 = normalize(vec3(-0.5, 0.3, -0.6));
    float k = 0.42 + 0.48 * max(dot(n, l1), 0.0) + 0.16 * max(dot(n, l2), 0.0);
    vec3 c = pow(v_col.rgb, vec3(2.2)) * k;          // light in linear space
    o_col = vec4(pow(c, vec3(1.0 / 2.2)), v_col.a);
}
"#;

struct Gpu {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    opaque: i32,
    glass: i32,
}

/// Shared between the UI thread and the paint callback.
#[derive(Default)]
pub struct Renderer {
    gpu: Option<Gpu>,
    pending: Option<(Vec<u8>, i32, i32)>,
    pub uploaded_key: (u64, u64),
}

const STRIDE: i32 = 28;

pub fn vertex_data(faces: &[Face], selected: &HashSet<CompId>) -> (Vec<u8>, i32, i32) {
    let mut opaque = Vec::with_capacity(faces.len() * 3 * STRIDE as usize);
    let mut glass = Vec::new();
    let (mut no, mut ng) = (0, 0);
    for f in faces {
        let sel = selected.contains(&f.comp);
        let mut col = f.colour;
        if sel {
            col = [((col[0] as u32 + 255 * 2) / 3) as u8, ((col[1] as u32 + 150 * 2) / 3) as u8, ((col[2] as u32 + 40 * 2) / 3) as u8, col[3].max(200)];
        }
        let (buf, n) = if col[3] < 255 { (&mut glass, &mut ng) } else { (&mut opaque, &mut no) };
        for t in 1..f.verts.len().saturating_sub(1) {
            for v in [f.verts[0], f.verts[t], f.verts[t + 1]] {
                let k = if f.two_sided { 0.5 } else { 1.0 };
                let nrm = [f.normal[0] * k, f.normal[1] * k, f.normal[2] * k];
                for x in v.iter().chain(nrm.iter()) {
                    buf.extend_from_slice(&x.to_le_bytes());
                }
                buf.extend_from_slice(&col);
                *n += 1;
            }
        }
    }
    opaque.extend_from_slice(&glass);
    (opaque, no, ng)
}

impl Renderer {
    pub fn set_data(&mut self, data: (Vec<u8>, i32, i32), key: (u64, u64)) {
        self.pending = Some(data);
        self.uploaded_key = key;
    }

    unsafe fn init(gl: &glow::Context) -> Gpu {
        let ver = if cfg!(target_os = "macos") { "#version 330 core\n" } else { "#version 330\n" };
        let program = gl.create_program().unwrap();
        let mut shaders = vec![];
        for (kind, src) in [(glow::VERTEX_SHADER, VS), (glow::FRAGMENT_SHADER, FS)] {
            let s = gl.create_shader(kind).unwrap();
            gl.shader_source(s, &format!("{ver}{src}"));
            gl.compile_shader(s);
            assert!(gl.get_shader_compile_status(s), "shader: {}", gl.get_shader_info_log(s));
            gl.attach_shader(program, s);
            shaders.push(s);
        }
        gl.bind_attrib_location(program, 0, "a_pos");
        gl.bind_attrib_location(program, 1, "a_nrm");
        gl.bind_attrib_location(program, 2, "a_col");
        gl.link_program(program);
        assert!(gl.get_program_link_status(program), "link: {}", gl.get_program_info_log(program));
        for s in shaders {
            gl.detach_shader(program, s);
            gl.delete_shader(s);
        }
        let vao = gl.create_vertex_array().unwrap();
        let vbo = gl.create_buffer().unwrap();
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, STRIDE, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, STRIDE, 12);
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 4, glow::UNSIGNED_BYTE, true, STRIDE, 24);
        gl.bind_vertex_array(None);
        Gpu { program, vao, vbo, opaque: 0, glass: 0 }
    }

    unsafe fn paint(&mut self, gl: &glow::Context, view: &View, vp: eframe::epaint::ViewportInPixels) {
        if self.gpu.is_none() {
            self.gpu = Some(Self::init(gl));
        }
        let gpu = self.gpu.as_mut().unwrap();
        if let Some((data, no, ng)) = self.pending.take() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(gpu.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &data, glow::STATIC_DRAW);
            gpu.opaque = no;
            gpu.glass = ng;
        }
        gl.viewport(vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px);
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px);
        gl.clear_color(0.11, 0.135, 0.17, 1.0);
        gl.depth_mask(true);
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        gl.enable(glow::DEPTH_TEST);
        gl.depth_func(glow::LEQUAL);
        gl.disable(glow::CULL_FACE);
        gl.use_program(Some(gpu.program));
        gl.uniform_matrix_4_f32_slice(gl.get_uniform_location(gpu.program, "u_clip").as_ref(), false, &view.clip_matrix());
        gl.uniform_3_f32_slice(gl.get_uniform_location(gpu.program, "u_eye").as_ref(), &view.eye);
        gl.bind_vertex_array(Some(gpu.vao));
        gl.disable(glow::BLEND);
        gl.draw_arrays(glow::TRIANGLES, 0, gpu.opaque);
        if gpu.glass > 0 {
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.depth_mask(false);
            gl.draw_arrays(glow::TRIANGLES, gpu.opaque, gpu.glass);
            gl.depth_mask(true);
        }
        gl.bind_vertex_array(None);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::SCISSOR_TEST);
        gl.enable(glow::BLEND);
    }
}

pub struct Response {
    pub clicked: Option<(Pos2, egui::Modifiers)>,
}

/// Draw the scene and handle camera input. `ghost` faces (placement preview) are outlined on top.
pub fn show(ui: &mut egui::Ui, cam: &mut Camera, renderer: &Arc<Mutex<Renderer>>, ghost: Option<&[Face]>) -> Response {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
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
    let r = renderer.clone();
    let cb = egui_glow::CallbackFn::new(move |info, painter| {
        let vp = info.viewport_in_pixels();
        unsafe { r.lock().unwrap().paint(painter.gl(), &view, vp) };
    });
    let painter = ui.painter_at(rect);
    painter.add(egui::PaintCallback { rect, callback: Arc::new(cb) });

    if let Some(g) = ghost {
        for f in g {
            let pts: Vec<Pos2> = f.verts.iter().filter_map(|v| view.project(*v).map(|x| x.0)).collect();
            if pts.len() == f.verts.len() && pts.len() >= 3 {
                painter.add(egui::Shape::convex_polygon(pts, Color32::from_rgba_unmultiplied(120, 230, 150, 70), Stroke::new(1.0, Color32::from_rgb(120, 230, 150))));
            }
        }
    }
    let g = rect.left_bottom() + egui::vec2(40.0, -40.0);
    for (axis, col, name) in [([1.0, 0.0, 0.0], Color32::from_rgb(230, 80, 80), "X"), ([0.0, 1.0, 0.0], Color32::from_rgb(90, 220, 90), "Y"), ([0.0, 0.0, 1.0], Color32::from_rgb(90, 140, 255), "Z")] {
        let e = egui::vec2(dot(axis, view.r), -dot(axis, view.u)) * 28.0;
        painter.line_segment([g, g + e], Stroke::new(2.0, col));
        painter.text(g + e * 1.2, egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(12.0), col);
    }
    let clicked = if resp.clicked() { resp.interact_pointer_pos().map(|p| (p, ui.input(|i| i.modifiers))) } else { None };
    Response { clicked }
}
