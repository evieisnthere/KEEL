//! Build-mode CAD tools: paint and eyedropper, box fill, measure, mirror (symmetry), placement rotation, copy/paste
//! stamps. Geometry helpers are pure so they can be tested.
use crate::icons::Icon;
use keel_model::edit::{quarter_turn, GenPart};

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Tool {
    Select,
    Place,
    Paint,
    Fill,
    Measure,
    Wire,
}

impl Tool {
    pub const ALL: [(Tool, Icon, &'static str, &'static str); 6] = [
        (Tool::Select, Icon::Select, "Select", "Click to select, Shift/Ctrl to add (Esc)"),
        (Tool::Place, Icon::Place, "Place", "Place the palette part; R / Shift+R / Alt+R rotate it first"),
        (Tool::Paint, Icon::Paint, "Paint", "Click a part to paint it; Alt+click picks its colour"),
        (Tool::Fill, Icon::Fill, "Fill", "Click two corners to fill a box with the palette part"),
        (Tool::Measure, Icon::Measure, "Measure", "Click two points to measure"),
        (Tool::Wire, Icon::Wire, "Wire", "Click an output then an input to wire them; click a wire and press Delete to remove it"),
    ];
}

pub struct Tools {
    pub colour: [u8; 3],
    pub mirror: bool,
    /// rotation applied when placing (column-major)
    pub place_rot: [i32; 9],
    pub fill_start: Option<[i32; 3]>,
    pub measure: Vec<[f32; 3]>,
    /// copied parts, relative to their lowest corner
    pub clipboard: Vec<GenPart>,
    /// placing the clipboard instead of a single part
    pub stamping: bool,
}

impl Default for Tools {
    fn default() -> Self {
        Tools { colour: [222, 222, 218], mirror: false, place_rot: keel_io::vehicle::IDENTITY, fill_start: None, measure: vec![], clipboard: vec![], stamping: false }
    }
}

pub fn hex(c: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// Compose a quarter turn about `axis` onto a column-major rotation.
pub fn turn(r: [i32; 9], axis: usize) -> [i32; 9] {
    let q = quarter_turn(axis);
    let mut out = [0; 9];
    for j in 0..3 {
        for i in 0..3 {
            out[3 * j + i] = (0..3).map(|k| q[i][k] * r[3 * j + k]).sum();
        }
    }
    out
}

/// Mirror a part across the centre line (x = 0): position and rotation (the result is an improper rotation, as the
/// game's own symmetry tool produces).
pub fn mirror(p: &GenPart) -> GenPart {
    let mut r = p.r;
    for j in 0..3 {
        r[3 * j] = -r[3 * j];
    }
    GenPart { vp: [-p.vp[0], p.vp[1], p.vp[2]], r, ..p.clone() }
}

/// Add mirrored copies (skipping parts on the centre line that would land on themselves).
pub fn with_mirror(parts: Vec<GenPart>, on: bool) -> Vec<GenPart> {
    if !on {
        return parts;
    }
    let mut out = parts.clone();
    for p in &parts {
        let m = mirror(p);
        if m.vp != p.vp || m.r != p.r {
            out.push(m);
        }
    }
    out
}

/// A box of parts between two corners (inclusive).
pub fn fill_box(def: &str, a: [i32; 3], b: [i32; 3], r: [i32; 9], colour: &str) -> Vec<GenPart> {
    let lo = [0, 1, 2].map(|i| a[i].min(b[i]));
    let hi = [0, 1, 2].map(|i| a[i].max(b[i]));
    let mut out = vec![];
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                out.push(GenPart { def: def.into(), vp: [x, y, z], r, bc: Some(colour.into()), ac: None });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_turns_are_identity_and_mirror_is_an_involution() {
        let mut r = keel_io::vehicle::IDENTITY;
        for _ in 0..4 {
            r = turn(r, 1);
        }
        assert_eq!(r, keel_io::vehicle::IDENTITY);
        let p = GenPart { def: "02_wedge".into(), vp: [3, 1, 2], r: turn(keel_io::vehicle::IDENTITY, 2), bc: None, ac: None };
        assert_eq!(mirror(&mirror(&p)), p);
        assert_eq!(with_mirror(vec![p.clone()], true).len(), 2);
        assert_eq!(fill_box("01_block", [0, 0, 0], [1, 1, 2], keel_io::vehicle::IDENTITY, "FFFFFF").len(), 12);
    }
}
