//! KEEL geometry: NURBS, parametric hulls, hydrostatics and conversion to Stormworks blocks.
pub mod hull;
pub mod nurbs;
pub mod voxel;

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
