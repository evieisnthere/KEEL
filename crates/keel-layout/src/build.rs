//! A generated build: parts with paint and wires, plus helpers for orienting parts.
use keel_model::edit::{GenLink, GenPart};

#[derive(Debug, Clone, Default)]
pub struct Build {
    pub parts: Vec<GenPart>,
    pub links: Vec<GenLink>,
}

/// Axis directions: 0 +x, 1 −x, 2 +y, 3 −y, 4 +z, 5 −z (the game's face order).
pub fn dir_vec(d: usize) -> [i32; 3] {
    [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]][d]
}

pub fn dir_of(v: [i32; 3]) -> usize {
    (0..6).find(|&d| dir_vec(d) == v).expect("axis vector")
}

/// Rotation (column-major, column j = image of local axis j) taking local +y to `up` and local +z to `fwd`
/// (perpendicular). Local +x completes a right-handed frame.
pub fn orient(up: usize, fwd: usize) -> [i32; 9] {
    let y = dir_vec(up);
    let z = dir_vec(fwd);
    assert_eq!(y[0] * z[0] + y[1] * z[1] + y[2] * z[2], 0, "up and forward must be perpendicular");
    let x = [y[1] * z[2] - y[2] * z[1], y[2] * z[0] - y[0] * z[2], y[0] * z[1] - y[1] * z[0]];
    [x[0], x[1], x[2], y[0], y[1], y[2], z[0], z[1], z[2]]
}

pub const ID: [i32; 9] = [1, 0, 0, 0, 1, 0, 0, 0, 1];

/// Hex colour from 0..1 RGB.
pub fn hex(rgb: [f64; 3]) -> String {
    rgb.iter().map(|c| format!("{:02X}", (c.clamp(0.0, 1.0) * 255.0).round() as u8)).collect()
}

impl Build {
    pub fn part(&mut self, def: &str, vp: [i32; 3], r: [i32; 9], bc: Option<&str>) -> usize {
        self.parts.push(GenPart { def: def.into(), vp, r, bc: bc.map(|s| s.into()), ac: None });
        self.parts.len() - 1
    }
    pub fn block(&mut self, vp: [i32; 3], colour: &str) {
        self.part("01_block", vp, ID, Some(colour));
    }
    pub fn link(&mut self, kind: i32, a: [i32; 3], b: [i32; 3]) {
        self.links.push(GenLink { kind, a, b });
    }
    pub fn extend(&mut self, o: Build) {
        self.parts.extend(o.parts);
        self.links.extend(o.links);
    }
    /// Voxels occupied by blocks and parts' origins (for clash checks in tests).
    pub fn origins(&self) -> Vec<[i32; 3]> {
        self.parts.iter().map(|p| p.vp).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orient_is_proper_and_maps_axes() {
        for up in 0..6 {
            for fwd in 0..6 {
                if up / 2 == fwd / 2 {
                    continue;
                }
                let r = orient(up, fwd);
                assert_eq!([r[3], r[4], r[5]], dir_vec(up));
                assert_eq!([r[6], r[7], r[8]], dir_vec(fwd));
                let det = r[0] * (r[4] * r[8] - r[5] * r[7]) - r[3] * (r[1] * r[8] - r[2] * r[7]) + r[6] * (r[1] * r[5] - r[2] * r[4]);
                assert_eq!(det, 1);
            }
        }
    }
}
