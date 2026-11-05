//! Stormworks integration with no terminal needed: find the game through Steam, read its definitions, list and write
//! vehicles in the game's own vehicles folder (with a "Made with KEEL" thumbnail), and launch the game.
use keel_model::geometry::Face;
use std::path::{Path, PathBuf};

/// Steam app id of Stormworks: Build and Rescue.
pub const APP_ID: &str = "573090";

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// A path for display, with the home folder shortened to `~`.
pub fn pretty(p: &Path) -> String {
    match home() {
        Some(h) if p.starts_with(&h) => format!("~{}{}", std::path::MAIN_SEPARATOR, p.strip_prefix(&h).unwrap().display()),
        _ => p.display().to_string(),
    }
}

/// Steam installation roots that exist on this machine.
pub fn steam_roots() -> Vec<PathBuf> {
    let mut c: Vec<PathBuf> = vec![];
    if cfg!(windows) {
        for v in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(p) = std::env::var_os(v) {
                c.push(PathBuf::from(p).join("Steam"));
            }
        }
        c.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    } else if let Some(h) = home() {
        c.push(h.join(".steam/steam"));
        c.push(h.join(".local/share/Steam"));
        c.push(h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
        c.push(h.join("snap/steam/common/.local/share/Steam"));
        c.push(h.join("Library/Application Support/Steam"));
    }
    let mut out: Vec<PathBuf> = vec![];
    for p in c {
        let p = std::fs::canonicalize(&p).unwrap_or(p);
        if p.join("steamapps").is_dir() && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Library folders listed in `steamapps/libraryfolders.vdf` (plus the root itself).
pub fn libraries() -> Vec<PathBuf> {
    let mut out = vec![];
    for root in steam_roots() {
        out.push(root.clone());
        if let Ok(s) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            for line in s.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("\"path\"") {
                    let v = rest.trim().trim_matches('"').replace("\\\\", "\\");
                    let p = PathBuf::from(v);
                    if !out.contains(&p) {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
}

/// A Stormworks installation folder (the one holding `rom/`).
pub fn find_game() -> Option<PathBuf> {
    libraries().into_iter().map(|l| l.join("steamapps/common/Stormworks")).find(|p| p.join("rom/data/definitions").is_dir())
}

pub fn definitions_of(game: &Path) -> PathBuf {
    game.join("rom/data/definitions")
}

/// Accept the game folder, its `rom`, or the definitions folder itself; returns the definitions folder.
pub fn normalise_pick(p: &Path) -> Option<PathBuf> {
    for c in [p.join("rom/data/definitions"), p.join("data/definitions"), p.to_path_buf()] {
        if c.is_dir() && c.join("01_block.xml").exists() {
            return Some(c);
        }
    }
    None
}

/// The game's saved-vehicles folder: `%APPDATA%\Stormworks\data\vehicles` on Windows; inside the Proton prefix on
/// Linux.
pub fn vehicles_dir() -> Option<PathBuf> {
    let mut c = vec![];
    if let Some(a) = std::env::var_os("APPDATA") {
        c.push(PathBuf::from(a).join("Stormworks/data/vehicles"));
    }
    for l in libraries() {
        c.push(l.join(format!("steamapps/compatdata/{APP_ID}/pfx/drive_c/users/steamuser/AppData/Roaming/Stormworks/data/vehicles")));
    }
    c.into_iter().find(|p| p.is_dir())
}

/// Saved vehicles in a folder: (name, xml path, thumbnail if any), newest first.
pub fn list_vehicles(dir: &Path) -> Vec<(String, PathBuf, Option<PathBuf>)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut v: Vec<(std::time::SystemTime, String, PathBuf, Option<PathBuf>)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xml")))
        .map(|p| {
            let name = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let png = p.with_extension("png");
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            (t, name, p, png.exists().then_some(png))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, n, p, t)| (n, p, t)).collect()
}

/// A file name the game and every file system accept.
pub fn safe_name(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c.is_alphanumeric() || " -_()".contains(c) { c } else { '_' }).collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        "keel_vehicle".into()
    } else {
        s
    }
}

/// Write `<name>.xml` and the `<name>.png` thumbnail into the vehicles folder. Returns the XML path.
pub fn export(dir: &Path, name: &str, xml: &str, thumb: &image::RgbaImage) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let n = safe_name(name);
    let p = dir.join(format!("{n}.xml"));
    std::fs::write(&p, xml).map_err(|e| format!("could not write {}: {e}", p.display()))?;
    thumb.save(dir.join(format!("{n}.png"))).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Open Stormworks through Steam.
pub fn launch() -> Result<(), String> {
    let url = format!("steam://rungameid/{APP_ID}");
    open_url(&url)
}

pub fn open_url(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        return std::process::Command::new("cmd").args(["/C", "start", "", url]).creation_flags(CREATE_NO_WINDOW).spawn().map(|_| ()).map_err(|e| e.to_string());
    }
    #[cfg(target_os = "macos")]
    {
        return std::process::Command::new("open").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string());
    }
    #[allow(unreachable_code)]
    std::process::Command::new("xdg-open").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ---- thumbnail ---------------------------------------------------------------------------------------------------

/// Flat-shaded, depth-buffered render of faces from a 3/4 view, fitted to `size` with a margin.
pub fn raster(faces: &[Face], size: usize, yaw_deg: f32, pitch_deg: f32, margin: f32) -> Vec<[u8; 4]> {
    let mut img = vec![[0u8; 4]; size * size];
    if faces.is_empty() {
        return img;
    }
    let (cy, sy) = (yaw_deg.to_radians().cos(), yaw_deg.to_radians().sin());
    let (cp, sp) = (pitch_deg.to_radians().cos(), pitch_deg.to_radians().sin());
    let proj = |p: [f32; 3]| -> [f32; 3] {
        let x = p[0] * cy - p[2] * sy;
        let z0 = p[0] * sy + p[2] * cy;
        [x, p[1] * cp - z0 * sp, p[1] * sp + z0 * cp]
    };
    let pts: Vec<[f32; 3]> = faces.iter().flat_map(|f| f.verts.iter().map(|v| proj(*v))).collect();
    let (x0, x1) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
    let (y0, y1) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
    let n = size as f32;
    let s = (n * (1.0 - 2.0 * margin)) / (x1 - x0).max(y1 - y0).max(1e-3);
    let (ox, oy) = (n / 2.0 - (x0 + x1) / 2.0 * s, n / 2.0 + (y0 + y1) / 2.0 * s);
    let mut zb = vec![f32::MIN; size * size];
    let light = [0.45f32, 0.8, 0.4];
    for f in faces {
        let nn = proj(f.normal);
        if !f.two_sided && nn[2] < 0.0 {
            continue;
        }
        let sh = 0.5 + 0.5 * (f.normal[0] * light[0] + f.normal[1] * light[1] + f.normal[2] * light[2]).abs().min(1.0);
        let c = f.colour;
        let col = [(c[0] as f32 * sh) as u8, (c[1] as f32 * sh) as u8, (c[2] as f32 * sh) as u8, 255];
        let v: Vec<[f32; 3]> = f.verts.iter().map(|p| {
            let q = proj(*p);
            [q[0] * s + ox, oy - q[1] * s, q[2]]
        }).collect();
        for k in 1..v.len().saturating_sub(1) {
            let (a, b, cc) = (v[0], v[k], v[k + 1]);
            let bx0 = a[0].min(b[0]).min(cc[0]).floor().max(0.0) as usize;
            let bx1 = (a[0].max(b[0]).max(cc[0]).ceil().max(0.0) as usize).min(size - 1);
            let by0 = a[1].min(b[1]).min(cc[1]).floor().max(0.0) as usize;
            let by1 = (a[1].max(b[1]).max(cc[1]).ceil().max(0.0) as usize).min(size - 1);
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
                    let i = y * size + x;
                    if z > zb[i] {
                        zb[i] = z;
                        img[i] = col;
                    }
                }
            }
        }
    }
    img
}

/// 5×7 bitmap glyphs for the badge text.
fn glyph(c: char) -> [u8; 7] {
    match c {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        _ => [0; 7],
    }
}

fn blend(px: &mut image::Rgba<u8>, c: [u8; 3], a: f32) {
    for i in 0..3 {
        px.0[i] = (px.0[i] as f32 * (1.0 - a) + c[i] as f32 * a) as u8;
    }
    px.0[3] = 255;
}

fn text(img: &mut image::RgbaImage, s: &str, x0: i32, y0: i32, scale: i32, c: [u8; 3]) {
    let mut x = x0;
    for ch in s.chars() {
        let g = glyph(ch);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits >> (4 - col) & 1 == 1 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let (px, py) = (x + col * scale + dx, y0 + row as i32 * scale + dy);
                            if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                                blend(img.get_pixel_mut(px as u32, py as u32), c, 1.0);
                            }
                        }
                    }
                }
            }
        }
        x += 6 * scale;
    }
}

pub fn text_width(s: &str, scale: i32) -> i32 {
    s.chars().count() as i32 * 6 * scale - scale
}

/// The KEEL mark (rounded tile, hull, keel line) at (x, y) with side `w`.
pub fn mark(img: &mut image::RgbaImage, x: i32, y: i32, w: i32) {
    let tile = [0x2b, 0x2f, 0x6e];
    let cyan = [0x3f, 0xd6, 0xe6];
    let white = [0xe8, 0xea, 0xf6];
    let r = w as f32 * 0.25;
    let s = w as f32 / 40.0;
    for py in 0..w {
        for px in 0..w {
            let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
            let cx = fx.clamp(r, w as f32 - r);
            let cy = fy.clamp(r, w as f32 - r);
            let inside = (fx - cx).powi(2) + (fy - cy).powi(2) <= r * r;
            if !inside {
                continue;
            }
            let (u, v) = (fx / s, fy / s);
            // hull trapezoid 8..33 at y 17, 12..28 at y 26
            let t = ((v - 17.0) / 9.0).clamp(0.0, 1.0);
            let hull = (17.0..=26.0).contains(&v) && u >= 8.0 + 4.0 * t && u <= 33.0 - 5.0 * t;
            let keel = (19.0..=21.0).contains(&u) && (26.0..=32.0).contains(&v);
            let deck = (14.0..=26.0).contains(&u) && (11.5..=13.5).contains(&v);
            let c = if hull || keel { cyan } else if deck { white } else { tile };
            let (ix, iy) = (x + px, y + py);
            if ix >= 0 && iy >= 0 && (ix as u32) < img.width() && (iy as u32) < img.height() {
                blend(img.get_pixel_mut(ix as u32, iy as u32), c, 1.0);
            }
        }
    }
}

/// The 512×512 vehicle thumbnail: the vehicle on a navy gradient with a "MADE WITH KEEL" badge.
pub fn thumbnail(faces: &[Face]) -> image::RgbaImage {
    let n = 512u32;
    let mut img = image::RgbaImage::new(n, n);
    for y in 0..n {
        let t = y as f32 / n as f32;
        let c = [(0x24 as f32 * (1.0 - t) + 0x0e as f32 * t) as u8, (0x28 as f32 * (1.0 - t) + 0x10 as f32 * t) as u8, (0x5a as f32 * (1.0 - t) + 0x24 as f32 * t) as u8];
        for x in 0..n {
            img.put_pixel(x, y, image::Rgba([c[0], c[1], c[2], 255]));
        }
    }
    let r = raster(faces, n as usize, 35.0, 22.0, 0.1);
    for (i, px) in r.iter().enumerate() {
        if px[3] > 0 {
            img.put_pixel((i % n as usize) as u32, (i / n as usize) as u32, image::Rgba(*px));
        }
    }
    // badge in the bottom right
    let label = "MADE WITH KEEL";
    let sc = 2;
    let (bw, bh) = (text_width(label, sc) + 26 + 22, 30);
    let (bx, by) = (n as i32 - bw - 14, n as i32 - bh - 14);
    for y in by..by + bh {
        for x in bx..bx + bw {
            blend(img.get_pixel_mut(x as u32, y as u32), [0x12, 0x13, 0x2b], 0.85);
        }
    }
    mark(&mut img, bx + 6, by + 4, 22);
    text(&mut img, label, bx + 34, by + 8, sc, [0xe8, 0xea, 0xf6]);
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_thumbnail() {
        assert_eq!(safe_name("My Boat: v2/final"), "My Boat_ v2_final");
        assert_eq!(safe_name("  "), "keel_vehicle");
        let t = thumbnail(&[]);
        assert_eq!(t.dimensions(), (512, 512));
        let dir = std::env::temp_dir().join(format!("keel_export_{}", std::process::id()));
        let p = export(&dir, "Test", "<vehicle/>", &t).unwrap();
        assert!(p.exists() && dir.join("Test.png").exists());
        assert_eq!(list_vehicles(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
