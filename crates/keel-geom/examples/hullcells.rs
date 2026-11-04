//! Debug: list generated hull placements near a voxel (cargo run --example hullcells -- x y z).
fn main() {
    let a: Vec<i32> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let h = keel_geom::hull::HullParams::default().build().unwrap();
    let b = keel_geom::hullblocks::build(&h, &Default::default());
    for p in &b.placements {
        if (0..3).all(|i| (p.vp[i] - a[i]).abs() <= 2) {
            println!("{:?} {} {:?}", p.vp, p.def, p.r);
        }
    }
}
