//! List placements (with source) inside a voxel box (debug): cargo run --example blocksat -- x0 x1 y0 y1 z0 z1
fn main() {
    let a: Vec<i32> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let h = keel_geom::hull::HullParams::default().build().unwrap();
    let b = keel_geom::hullblocks::build(&h, &Default::default());
    let mut v: Vec<_> = b.placements.iter().zip(&b.sources).filter(|(p, _)| p.vp[0] >= a[0] && p.vp[0] <= a[1] && p.vp[1] >= a[2] && p.vp[1] <= a[3] && p.vp[2] >= a[4] && p.vp[2] <= a[5]).collect();
    v.sort_by_key(|(p, _)| (p.vp[2], p.vp[1], p.vp[0]));
    for (p, s) in v {
        println!("{:?} {:8} {} {:?}", p.vp, s, p.def, p.r);
    }
}
