//! Superstructure (deckhouse) builder.
//!
//! A superstructure is a stack of tiers on the weather deck. Each tier is a box in plan with rounded front corners (a
//! lattice curve, as for the hull's deck edge), optional tumblehome (sides leaning in going up) and front rake (front
//! leaning back going up). Tiers step back from the one below. The exterior is built by the same label-consistent
//! tiler as the hull, so faces are long wedges and pyramid pairs; flat wall panels get windows.
//!
//! Each tier is 9 rows: its floor (the deck or the roof of the tier below), 8 rows of headroom, and the roof / next
//! floor on top.
use crate::build::{orient, Build};
use keel_geom::sweep::{fit_curve, Curve, CurveOptions};
use serde::{Deserialize, Serialize};

pub const PITCH: i32 = 9;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tier {
    /// aft end and forward end (voxel z, inclusive) of the tier at its floor
    pub aft: i32,
    pub fwd: i32,
    /// half width in voxels (the tier spans x = −half_width ..= half_width)
    pub half_width: i32,
    /// radius of the rounded front corners in voxels (0 = square)
    pub corner: i32,
    /// sides lean in by 1 voxel every `tumblehome` rows (0 = vertical)
    pub tumblehome: i32,
    /// front leans back by 1 voxel every `rake` rows (0 = vertical)
    pub rake: i32,
    /// windows along the sides and front
    pub windows: bool,
    /// floor-to-floor height in rows (headroom = height − 1; doors need 8)
    #[serde(default = "default_height")]
    pub height: i32,
}

fn default_height() -> i32 {
    PITCH
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Superstructure {
    /// floor row of the first tier (the weather deck plating row)
    pub base_y: i32,
    pub tiers: Vec<Tier>,
    pub colour: String,
    pub window_kind: String,
}

impl Superstructure {
    /// A default deckhouse for a hull of `half_beam` voxels and deck length `[z0, z1]`: `n` tiers, the lowest spanning
    /// the middle of the deck, the top one a bridge set forward.
    pub fn default_for(base_y: i32, half_beam: i32, z0: i32, z1: i32, n: usize) -> Self {
        let len = z1 - z0;
        let mut tiers = vec![];
        let hw0 = (half_beam - 2).max(3);
        let (mut a, mut f) = (z0 + len * 30 / 100, z0 + len * 70 / 100);
        for k in 0..n.max(1) {
            let hw = (hw0 - 2 * k as i32).max(3);
            let corner = (hw / 3).clamp(1, 6);
            tiers.push(Tier { aft: a, fwd: f, half_width: hw, corner, tumblehome: 0, rake: if k + 1 == n { 3 } else { 0 }, windows: true, height: PITCH });
            a += (f - a) / 4;
            f -= 2;
        }
        Superstructure { base_y, tiers, colour: "E8E8E4".into(), window_kind: "window_2x2".into() }
    }

    pub fn tier_floor(&self, k: usize) -> i32 {
        self.base_y + self.tiers.iter().take(k).map(|t| t.height).sum::<i32>()
    }

    /// Plan curve of a tier's front corner (starboard): from (half_width, fwd − corner) to (half_width − corner, fwd).
    fn corner_curve(t: &Tier) -> Curve {
        let c = t.corner.max(0);
        if c == 0 {
            return Curve { pts: vec![[t.half_width, t.fwd], [t.half_width, t.fwd + 1]], dir: [-1, 1] };
        }
        let target: Vec<[f64; 2]> = (0..=30)
            .map(|i| {
                let a = (i as f64 / 30.0) * std::f64::consts::FRAC_PI_2;
                [(t.half_width - c) as f64 + c as f64 * a.cos() + 0.5, (t.fwd - c) as f64 + c as f64 * a.sin() + 0.5]
            })
            .collect();
        fit_curve([t.half_width + 1, t.fwd - c], [-1, 1], &target, &|q| q[1] == t.fwd + 1, ([0, t.fwd - c], [t.half_width + 1, t.fwd + 1]), &CurveOptions::default())
    }

    /// The solid, with its corner curves fitted once.
    pub fn solid(&self) -> SuperSolid<'_> {
        SuperSolid { s: self, curves: self.tiers.iter().map(Self::corner_curve).collect() }
    }

    /// Inside test at lattice coordinates (see [`SuperSolid::inside`]).
    pub fn inside(&self, lx: f64, ly: f64, lz: f64) -> bool {
        self.solid().inside(lx, ly, lz)
    }

    /// Build the exterior shell (walls, roofs; the floor of the lowest tier is the existing deck) and windows.
    pub fn build(&self) -> Build {
        let solid = self.solid();
        let inside8 = |e: [i32; 3]| solid.inside((e[0].abs() as f64 + 1.0) / 2.0, (e[1] as f64 + 1.0) / 2.0, (e[2] as f64 + 1.0) / 2.0);
        let full = |v: [i32; 3]| (0..8).all(|k| inside8([2 * v[0].abs() - 1 + 2 * (k & 1), 2 * v[1] - 1 + 2 * ((k >> 1) & 1), 2 * v[2] - 1 + 2 * ((k >> 2) & 1)]));
        let (mut z0, mut z1, mut xm) = (i32::MAX, i32::MIN, 0);
        for t in &self.tiers {
            z0 = z0.min(t.aft);
            z1 = z1.max(t.fwd);
            xm = xm.max(t.half_width);
        }
        let ytop = self.tier_floor(self.tiers.len()) + 2;
        let mut shell = vec![];
        for z in z0 - 1..=z1 + 1 {
            for y in self.base_y..=ytop {
                for x in 0..=xm + 1 {
                    let m = (0..8).any(|k| inside8([2 * x - 1 + 2 * (k & 1), 2 * y - 1 + 2 * ((k >> 1) & 1), 2 * z - 1 + 2 * ((k >> 2) & 1)]));
                    if m {
                        shell.push([x, y, z]);
                    }
                }
            }
        }
        let base = self.base_y + 1;
        // the lowest tier stands on the deck: no floor there
        let veto = |v: [i32; 3]| v[1] == base && [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, 1, 0]].iter().all(|d| full([v[0] + d[0], v[1] + d[1], v[2] + d[2]]));
        let allowed = |t: &keel_geom::tiling::Template, a: [i32; 3]| -> bool {
            let xs: Vec<i32> = t.cells.iter().map(|c| c[0] + a[0]).collect();
            xs.iter().all(|&x| x >= 1) || (xs.iter().all(|&x| x == 0) && t.symmetric_x())
        };
        let tiles = keel_geom::tiling::tile(&inside8, &shell, &veto, &allowed, &|_| 0.0);
        let mut b = Build::default();
        let mut wall_cubes: std::collections::HashSet<[i32; 3]> = Default::default();
        for (pl, cells) in tiles {
            if pl.def == "01_block" {
                wall_cubes.insert(pl.vp);
            }
            let mut r = pl.r;
            let mirror = cells.iter().all(|c| c[0] >= 1);
            if mirror {
                for j in 0..3 {
                    r[3 * j] = -r[3 * j];
                }
                b.part(&pl.def, [-pl.vp[0], pl.vp[1], pl.vp[2]], r, Some(&self.colour));
            }
            b.part(&pl.def, pl.vp, pl.r, Some(&self.colour));
        }
        if self.tiers.iter().any(|t| t.windows) {
            self.add_windows(&mut b, &wall_cubes, &full);
        }
        b
    }

    /// Replace runs of flat wall cubes at window height with 2×2 windows (sides and front).
    fn add_windows(&self, b: &mut Build, walls: &std::collections::HashSet<[i32; 3]>, full: &dyn Fn([i32; 3]) -> bool) {
        let mut remove: std::collections::HashSet<[i32; 3]> = Default::default();
        let mut add: Vec<([i32; 3], [i32; 9])> = vec![];
        for (k, t) in self.tiers.iter().enumerate() {
            if !t.windows {
                continue;
            }
            // window sill 4 rows above the floor, 2 rows tall (rows f+4, f+5)
            let f = self.tier_floor(k);
            let (y_lo, y_hi) = (f + t.height / 2, f + t.height / 2 + 1);
            // side walls (starboard; mirrored below): windows of 2 along z every 4 voxels
            let mut z = t.aft + 2;
            while z + 1 <= t.fwd - t.corner - 2 {
                let x = (0..=t.half_width + 1).rev().find(|&x| walls.contains(&[x, y_lo, z])).unwrap_or(-1);
                let ok = x > 0
                    && [[x, y_lo, z], [x, y_hi, z], [x, y_lo, z + 1], [x, y_hi, z + 1]].iter().all(|v| walls.contains(v))
                    && [[x - 1, y_lo, z], [x - 1, y_hi, z + 1]].iter().all(|v| full(*v));
                if ok {
                    for v in [[x, y_lo, z], [x, y_hi, z], [x, y_lo, z + 1], [x, y_hi, z + 1]] {
                        remove.insert(v);
                        remove.insert([-v[0], v[1], v[2]]);
                    }
                    // window_2x2 spans local (0, −1..0, −1..0): its plane is local x = 0, so local x points outboard
                    add.push(([x, y_hi, z + 1], orient(2, 4)));
                    add.push(([-x, y_hi, z + 1], orient(2, 4)));
                }
                z += 4;
            }
            // front: windows of 2 along x across the flat front face
            let zf = (t.aft..=t.fwd + 1).rev().find(|&z| walls.contains(&[0, y_lo, z])).unwrap_or(i32::MIN);
            if zf > i32::MIN {
                // pairs (1, 2), (3, 4), … starboard and mirrored; the centre column stays wall
                let mut x = 1;
                while x + 1 <= t.half_width - t.corner {
                    let cells = [[x, y_lo, zf], [x, y_hi, zf], [x + 1, y_lo, zf], [x + 1, y_hi, zf]];
                    if cells.iter().all(|v| walls.contains(v)) && full([x, y_lo, zf - 1]) {
                        for v in cells {
                            remove.insert(v);
                            remove.insert([-v[0], v[1], v[2]]);
                        }
                        // plane z = zf: local x → world +z, local z → world −x, so local z −1..0 covers x..x+1
                        add.push(([x, y_hi, zf], orient(2, 1)));
                        add.push(([-x - 1, y_hi, zf], orient(2, 1)));
                    }
                    x += 2;
                }
            }
        }
        b.parts.retain(|p| !(p.def == "01_block" && remove.contains(&p.vp)));
        for (vp, r) in add {
            b.part(&self.window_kind, vp, r, None);
        }
    }
}

/// A superstructure ready for inside tests.
pub struct SuperSolid<'a> {
    pub s: &'a Superstructure,
    curves: Vec<Curve>,
}

impl SuperSolid<'_> {
    /// Inside test at lattice coordinates, the hull's convention: ℓx = |x| in voxel-edge units with the centre column
    /// at [0, 1] per half, ℓy = row (voxel y spans [y, y + 1]), ℓz = z + 0.5 (voxel z spans [z, z + 1]).
    pub fn inside(&self, lx: f64, ly: f64, lz: f64) -> bool {
        const EPS: f64 = 1e-9;
        for (k, t) in self.s.tiers.iter().enumerate() {
            let y0 = (self.s.tier_floor(k) + 1) as f64;
            let y1 = (self.s.tier_floor(k) + t.height + 1) as f64;
            if ly < y0 - EPS || ly > y1 + EPS {
                continue;
            }
            let h = ly - y0;
            let tum = if t.tumblehome > 0 { (h / t.tumblehome as f64).floor() } else { 0.0 };
            let rake = if t.rake > 0 { (h / t.rake as f64).floor() } else { 0.0 };
            if lz < t.aft as f64 - EPS || lz > (t.fwd + 1) as f64 - rake + EPS {
                continue;
            }
            let xz = self.curves[k].across_at(lz + rake).unwrap_or(0.0).min((t.half_width + 1) as f64);
            if lx + tum <= xz + EPS {
                return true;
            }
        }
        false
    }

    /// Inside test at a voxel centre.
    pub fn voxel_inside(&self, v: [i32; 3]) -> bool {
        self.inside(v[0].abs() as f64 + 0.5, v[1] as f64 + 0.5, v[2] as f64 + 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_superstructure_builds_a_closed_shell_with_windows() {
        let s = Superstructure::default_for(11, 10, -36, 36, 2);
        assert!(s.inside(0.5, 13.0, 0.5));
        assert!(!s.inside(0.5, 40.0, 0.5));
        let b = s.build();
        assert!(b.parts.len() > 200, "{}", b.parts.len());
        assert!(b.parts.iter().any(|p| p.def == "window_2x2"));
        // no two parts at the same origin
        let mut seen = std::collections::HashSet::new();
        assert!(b.parts.iter().all(|p| seen.insert(p.vp)));
    }
}
