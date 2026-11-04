//! KEEL geometry: NURBS, parametric hulls, hydrostatics and conversion to Stormworks blocks.
pub mod hull;
pub mod nurbs;
pub mod voxel;
pub mod blockhull;
pub mod facets;
pub mod lattice;
pub mod hullblocks;
pub mod metrics;

/// A hull as a voxelisable solid (sections cached per sample plane).
pub struct HullSolid<'a> {
    pub hull: &'a hull::Hull,
    cache: std::cell::RefCell<std::collections::HashMap<i64, Option<Vec<[f64; 2]>>>>,
}

impl<'a> HullSolid<'a> {
    pub fn new(hull: &'a hull::Hull) -> Self {
        HullSolid { hull, cache: Default::default() }
    }
}

impl voxel::Solid for HullSolid<'_> {
    fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let p = &self.hull.params;
        let top = p.depth + p.sheer_bow.max(p.sheer_stern) + 0.01;
        ([-p.beam / 2.0, 0.0, -p.length / 2.0], [p.beam / 2.0, top, p.length / 2.0])
    }
    fn inside(&self, q: [f64; 3]) -> bool {
        let key = (q[2] * 4096.0).round() as i64;
        let mut c = self.cache.borrow_mut();
        let sec = c.entry(key).or_insert_with(|| self.hull.section_at(q[2], 24));
        match sec.as_ref().and_then(|s| hull::Hull::half_breadth(s, q[1])) {
            Some(b) => q[0].abs() <= b,
            None => false,
        }
    }
    fn normal(&self, q: [f64; 3]) -> Option<[f64; 3]> {
        let p = &self.hull.params;
        let s = q[2] / p.length + 0.5;
        if !(0.0..=1.0).contains(&s) {
            return None;
        }
        // near the deck edge (sheer) the surface is the horizontal deck plane
        if q[1] > p.sheer(s) - 0.125 {
            // deck: rises with the sheer along the length
            let ds_dz = (2.0 * p.sheer_bow * s - 2.0 * p.sheer_stern * (1.0 - s)) / p.length;
            let l = (1.0 + ds_dz * ds_dz).sqrt();
            return Some([0.0, 1.0 / l, -ds_dz / l]);
        }
        let b = |y: f64, z: f64| -> Option<f64> {
            let sec = self.hull.section_at(z, 48)?;
            let y = y.clamp(sec[0][1], sec[sec.len() - 1][1]);
            hull::Hull::half_breadth(&sec, y)
        };
        let (h, y, z) = (0.06, q[1], q[2]);
        let by = (b(y + h, z)? - b(y - h, z)?) / (2.0 * h);
        let bz = (b(y, z + h).unwrap_or(0.0) - b(y, z - h).unwrap_or(0.0)) / (2.0 * h);
        // starboard surface x = b(y, z): outward normal ∝ (1, -b_y, -b_z); mirror for port
        let sx = if q[0] < 0.0 { -1.0 } else { 1.0 };
        let n = [sx, -by, -bz];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        // at the keel line (x ≈ 0) the bottom is the surface: use the section bottom slope
        Some([n[0] / l, n[1] / l, n[2] / l])
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_hull_voxelises() {
        let h = crate::hull::HullParams::default().build().unwrap();
        let solid = crate::HullSolid::new(&h);
        let p = crate::voxel::voxelise(&solid, &crate::voxel::Options { shell: Some(1), open_top: true });
        assert!(p.len() > 1000, "{}", p.len());
        assert!(p.iter().any(|x| x.def == "02_wedge"));
    }
}
