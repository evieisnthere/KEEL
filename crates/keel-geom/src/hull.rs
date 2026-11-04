//! Parametric realistic hull: sections generated from naval-architecture parameters, lofted into a NURBS surface.
//!
//! Frame (Stormworks): x to starboard, y up, z forward; metres. The baseline (lowest keel point) is y = 0, the
//! midship section is z = 0. The surface describes the starboard half; the port half is its mirror.
use crate::nurbs::{loft, Surface, P3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HullParams {
    pub name: String,
    /// overall length (m)
    pub length: f64,
    /// maximum beam at deck (m)
    pub beam: f64,
    /// moulded depth, baseline to sheer at midship (m)
    pub depth: f64,
    /// design draft (m), used for hydrostatics and the boot-top colour
    pub draft: f64,
    /// bottom V angle at midship (degrees); 0 = flat bottom
    pub deadrise_deg: f64,
    /// extra deadrise at the bow (degrees), giving a finer forefoot
    pub bow_deadrise_deg: f64,
    /// topside flare (degrees from vertical)
    pub flare_deg: f64,
    /// bilge radius as a fraction of the half-beam (0 = hard chine)
    pub bilge: f64,
    /// fraction of length over which the bow tapers (entrance)
    pub entrance: f64,
    /// fullness of the entrance waterlines (1 = straight/fine, 3 = full/bluff)
    pub entrance_fullness: f64,
    /// fraction of length over which the stern tapers (run)
    pub run: f64,
    /// transom half-beam as a fraction of the maximum
    pub transom_width: f64,
    /// height of the transom bottom above the baseline, as a fraction of the draft
    pub transom_lift: f64,
    /// bow keel rise (forefoot cut-up) as a fraction of depth
    pub forefoot: f64,
    /// sheer rise at the bow (m)
    pub sheer_bow: f64,
    /// sheer rise at the stern (m)
    pub sheer_stern: f64,
    /// number of lofting stations (sections)
    pub stations: usize,
}

impl Default for HullParams {
    fn default() -> Self {
        // a 20 m workboat-style displacement hull
        HullParams {
            name: "Hull".into(),
            length: 20.0,
            beam: 5.5,
            depth: 2.75,
            draft: 1.4,
            deadrise_deg: 12.0,
            bow_deadrise_deg: 30.0,
            flare_deg: 8.0,
            bilge: 0.35,
            entrance: 0.45,
            entrance_fullness: 1.8,
            run: 0.3,
            transom_width: 0.85,
            transom_lift: 0.55,
            forefoot: 0.55,
            sheer_bow: 0.75,
            sheer_stern: 0.1,
            stations: 21,
        }
    }
}


fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl HullParams {
    /// Half-beam at the deck edge, as a fraction of the maximum, at s ∈ [0 stern, 1 bow].
    pub fn plan(&self, s: f64) -> f64 {
        let fwd_start = 1.0 - self.entrance.clamp(0.05, 0.95);
        let aft_end = self.run.clamp(0.0, 0.9);
        if s > fwd_start {
            let t = ((s - fwd_start) / (1.0 - fwd_start)).clamp(0.0, 1.0);
            let k = self.entrance_fullness.max(0.5);
            (1.0 - t.powf(1.0 + k)).max(0.0).powf(1.0 / (1.0 + 0.6 * k)).max(0.004)
        } else if s < aft_end && aft_end > 0.0 {
            let t = s / aft_end;
            self.transom_width + (1.0 - self.transom_width) * smooth(t)
        } else {
            1.0
        }
    }
    /// Keel (bottom centreline) height at s.
    pub fn keel(&self, s: f64) -> f64 {
        let fwd_start = 1.0 - self.entrance.clamp(0.05, 0.95);
        let aft_end = self.run.clamp(0.0, 0.9);
        let mut y = 0.0;
        if s > fwd_start {
            let t = ((s - fwd_start) / (1.0 - fwd_start)).clamp(0.0, 1.0);
            y += self.forefoot * self.depth * t.powf(2.2);
        }
        if s < aft_end && aft_end > 0.0 {
            let t = 1.0 - s / aft_end;
            y += self.transom_lift * self.draft * t * t;
        }
        y
    }
    pub fn sheer(&self, s: f64) -> f64 {
        self.depth + self.sheer_bow * s * s + self.sheer_stern * (1.0 - s) * (1.0 - s)
    }
    pub fn deadrise(&self, s: f64) -> f64 {
        let fwd_start = 1.0 - self.entrance.clamp(0.05, 0.95);
        let t = ((s - fwd_start) / (1.0 - fwd_start)).clamp(0.0, 1.0);
        (self.deadrise_deg + (self.bow_deadrise_deg - self.deadrise_deg) * t).to_radians()
    }
    pub fn z_of(&self, s: f64) -> f64 {
        (s - 0.5) * self.length
    }

    /// Section points from the keel (x = 0) up to the deck edge, starboard side, at s. `arc` = points on the bilge
    /// turn (the lofting uses a fixed count so all sections stay compatible).
    pub fn section_n(&self, s: f64, arc: usize) -> Vec<P3> {
        let z = self.z_of(s);
        let yk = self.keel(s);
        let ys = self.sheer(s).max(yk + 0.05);
        let bd = (self.beam / 2.0 * self.plan(s)).max(0.002);
        let flare = self.flare_deg.to_radians().tan();
        let h = ys - yk;
        let y_bilge_top = yk + 0.4 * h;
        let b_side = (bd - (ys - y_bilge_top) * flare).max(bd * 0.3);
        let r = (self.bilge.clamp(0.0, 0.95) * b_side).min(0.35 * h);
        let dr = self.deadrise(s).tan();
        let b_bot = (b_side - r).max(0.0);
        let y_bot = yk + b_bot * dr;
        let y_turn = (y_bot + r).min(y_bilge_top.max(y_bot + 1e-3));
        let mut pts = vec![[0.0, yk, z], [b_bot * 0.5, yk + b_bot * 0.5 * dr, z], [b_bot, y_bot, z]];
        for k in 1..=arc {
            let a = k as f64 / (arc + 1) as f64 * std::f64::consts::FRAC_PI_2;
            pts.push([b_bot + r * a.sin(), y_bot + (y_turn - y_bot) * (1.0 - a.cos()), z]);
        }
        pts.push([b_side, y_turn, z]);
        pts.push([(b_side + bd) / 2.0, (y_turn + ys) / 2.0, z]);
        pts.push([bd, ys, z]);
        for i in 1..pts.len() {
            if pts[i][1] < pts[i - 1][1] + 1e-4 {
                pts[i][1] = pts[i - 1][1] + 1e-4;
            }
        }
        pts
    }
    pub fn section(&self, s: f64) -> Vec<P3> {
        self.section_n(s, 3)
    }

    pub fn build(&self) -> Option<Hull> {
        let n = self.stations.clamp(5, 81);
        // cluster stations towards the ends where the shape changes fastest
        let rows: Vec<Vec<P3>> = (0..n)
            .map(|j| {
                let t = j as f64 / (n - 1) as f64;
                let s = 0.5 - 0.5 * (std::f64::consts::PI * t).cos();
                self.section(s.clamp(0.0, 0.999))
            })
            .collect();
        // hard chines are kept sharp with a degree-1 section direction
        let du = if self.bilge < 0.05 { 1 } else { 3 };
        let surface = loft(&rows, du, 3)?;
        Some(Hull { params: self.clone(), surface })
    }
}

#[derive(Debug, Clone)]
pub struct Hull {
    pub params: HullParams,
    /// starboard half: u runs keel → deck edge, v runs stern → bow
    pub surface: Surface,
}

impl Hull {
    /// Dense section polyline at longitudinal position z (metres), starboard half, keel → deck. Built from the
    /// parametric section directly (exact) so hydrostatics and voxelisation do not depend on surface inversion.
    pub fn section_at(&self, z: f64, n: usize) -> Option<Vec<[f64; 2]>> {
        let p = &self.params;
        let s = z / p.length + 0.5;
        if !(0.0..=1.0).contains(&s) {
            return None;
        }
        let pts = p.section_n(s.min(0.999), n.max(4));
        Some(pts.into_iter().map(|q| [q[0], q[1]]).collect())
    }

    /// Half-breadth of the hull at height y in the section polyline (None outside the hull's height range).
    pub fn half_breadth(sec: &[[f64; 2]], y: f64) -> Option<f64> {
        if y < sec[0][1] || y > sec[sec.len() - 1][1] {
            return None;
        }
        let mut best: Option<f64> = None;
        for w in sec.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a[1] <= y && y <= b[1]) || (b[1] <= y && y <= a[1]) {
                let t = if (b[1] - a[1]).abs() < 1e-12 { 0.0 } else { (y - a[1]) / (b[1] - a[1]) };
                let x = a[0] + (b[0] - a[0]) * t;
                best = Some(best.map_or(x, |v: f64| v.max(x)));
            }
        }
        best
    }
}

/// Real-world hydrostatics at a given draft (metres above baseline), by slicing sections.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Hydrostatics {
    pub draft: f64,
    pub volume_m3: f64,
    /// sea water 1025 kg/m³
    pub displacement_t: f64,
    /// longitudinal centre of buoyancy (m, +forward of midship)
    pub lcb: f64,
    /// vertical centre of buoyancy above baseline
    pub kb: f64,
    pub waterplane_m2: f64,
    /// transverse metacentric radius I_T / V
    pub bm: f64,
    /// KB + BM (metacentre height above baseline)
    pub km: f64,
    pub block_coefficient: f64,
    pub waterline_length: f64,
    pub waterline_beam: f64,
}

pub fn hydrostatics(hull: &Hull, draft: f64) -> Hydrostatics {
    let p = &hull.params;
    let nz = 240;
    let dz = p.length / nz as f64;
    let (mut vol, mut mz, mut my, mut awp, mut it) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut zmin, mut zmax, mut bmax): (f64, f64, f64) = (f64::MAX, f64::MIN, 0.0);
    for k in 0..nz {
        let z = -p.length / 2.0 + (k as f64 + 0.5) * dz;
        let Some(sec) = hull.section_at(z, 80) else { continue };
        // section area below the waterline (both halves) and its centroid height, by strips in y
        let ny = 60;
        let y0 = sec[0][1];
        if draft <= y0 {
            continue;
        }
        let top = draft.min(sec[sec.len() - 1][1]);
        let dy = (top - y0) / ny as f64;
        let (mut a, mut ay) = (0.0, 0.0);
        for j in 0..ny {
            let y = y0 + (j as f64 + 0.5) * dy;
            if let Some(b) = Hull::half_breadth(&sec, y) {
                a += 2.0 * b * dy;
                ay += 2.0 * b * dy * y;
            }
        }
        vol += a * dz;
        mz += a * dz * z;
        my += ay * dz;
        if let Some(b) = Hull::half_breadth(&sec, draft) {
            awp += 2.0 * b * dz;
            it += 2.0 / 3.0 * b.powi(3) * dz;
            if b > 0.01 {
                zmin = zmin.min(z);
                zmax = zmax.max(z);
            }
            bmax = bmax.max(2.0 * b);
        }
    }
    let lwl = if zmax > zmin { zmax - zmin + dz } else { 0.0 };
    let kb = if vol > 0.0 { my / vol } else { 0.0 };
    let bm = if vol > 0.0 { it / vol } else { 0.0 };
    Hydrostatics {
        draft,
        volume_m3: vol,
        displacement_t: vol * 1.025,
        lcb: if vol > 0.0 { mz / vol } else { 0.0 },
        kb,
        waterplane_m2: awp,
        bm,
        km: kb + bm,
        block_coefficient: if lwl > 0.0 && bmax > 0.0 { vol / (lwl * bmax * draft) } else { 0.0 },
        waterline_length: lwl,
        waterline_beam: bmax,
    }
}

/// Draft at which the hull displaces `mass_t` tonnes of sea water (bisection).
pub fn draft_for_mass(hull: &Hull, mass_t: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, hull.params.depth + hull.params.sheer_bow);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if hydrostatics(hull, mid).displacement_t < mass_t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_like_hull_hydrostatics() {
        // a nearly rectangular barge: block coefficient close to 1, volume ≈ L·B·T
        let p = HullParams {
            length: 20.0,
            beam: 6.0,
            depth: 3.0,
            draft: 1.5,
            deadrise_deg: 0.0,
            bow_deadrise_deg: 0.0,
            flare_deg: 0.0,
            bilge: 0.0,
            entrance: 0.05,
            entrance_fullness: 6.0,
            run: 0.0,
            transom_width: 1.0,
            transom_lift: 0.0,
            forefoot: 0.0,
            sheer_bow: 0.0,
            sheer_stern: 0.0,
            ..Default::default()
        };
        let h = p.build().unwrap();
        let hs = hydrostatics(&h, 1.5);
        let ideal = 20.0 * 6.0 * 1.5;
        assert!((hs.volume_m3 / ideal) > 0.93 && (hs.volume_m3 / ideal) < 1.01, "volume {}", hs.volume_m3);
        // BM of a box = B²/(12T) = 2.0
        assert!((hs.bm - 2.0).abs() < 0.15, "bm {}", hs.bm);
        assert!((hs.kb - 0.75).abs() < 0.05, "kb {}", hs.kb);
    }

    #[test]
    fn default_hull_is_plausible() {
        let h = HullParams::default().build().unwrap();
        let hs = hydrostatics(&h, h.params.draft);
        assert!(hs.block_coefficient > 0.35 && hs.block_coefficient < 0.75, "Cb {}", hs.block_coefficient);
        let t = draft_for_mass(&h, hs.displacement_t);
        assert!((t - h.params.draft).abs() < 0.01);
    }
}
