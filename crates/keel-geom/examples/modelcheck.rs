//! Compare generated blocks against the fitted model (voxels on the shell only: interior voxels are hollow by design).
fn main() {
    let h = keel_geom::hull::HullParams::default().build().unwrap();
    let b = keel_geom::hullblocks::build(&h, &Default::default());
    let mut cnt: std::collections::BTreeMap<&str, usize> = Default::default();
    for s in &b.sources {
        *cnt.entry(s).or_default() += 1;
    }
    println!("placements by source: {cnt:?}");
    let diffs = keel_geom::metrics::compare(&b.placements, &|q| b.model_inside_m(q));
    let extra: usize = diffs.iter().map(|d| d.1).sum();
    let missing: usize = diffs.iter().map(|d| d.2).sum();
    println!("voxels with extra samples {} (total {}), with missing (in placed voxels) {} (total {})", diffs.iter().filter(|d| d.1 > 0).count(), extra, diffs.iter().filter(|d| d.2 > 0).count(), missing);
    let mut by: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for (v, e, m) in &diffs {
        for (p, s) in b.placements.iter().zip(&b.sources) {
            if p.vp == *v {
                let x = by.entry(s).or_default();
                x.0 += e;
                x.1 += m;
            }
        }
    }
    println!("by source (extra, missing): {by:?}");
    for (v, e, m) in diffs.iter().take(25) {
        let defs: Vec<String> = b.placements.iter().zip(&b.sources).filter(|(p, _)| p.vp == *v).map(|(p, s)| format!("{} {} {:?}", s, p.def, p.r)).collect();
        println!("{v:?} extra {e} missing {m} {defs:?}");
    }
}
#[allow(dead_code)]
fn _unused() {}
