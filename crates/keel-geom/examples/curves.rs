//! Print the swept curves of a hull (default or a params JSON).
fn main() {
    let params: keel_geom::hull::HullParams = match std::env::args().nth(1) {
        Some(p) => serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap(),
        None => Default::default(),
    };
    let h = params.build().unwrap();
    let b = keel_geom::hullblocks::build(&h, &Default::default());
    println!("{}", b.model.describe());
    println!("plan fwd pts {:?}", b.model.plan_fwd.pts);
    println!("flare (from row {}) {:?}", b.model.flare_lo, b.model.flare.iter().map(|s| (s.start, s.run, s.h0, s.rise)).collect::<Vec<_>>());
    println!("deadrise {:?}", b.model.deadrise.iter().map(|s| (s.start, s.run, s.h0, s.rise)).collect::<Vec<_>>());
    println!("sheer {:?}", b.model.sheer.iter().filter(|s| s.rise != 0).map(|s| (s.start, s.run, s.h0, s.rise)).collect::<Vec<_>>());
    println!("keel fwd pts {:?}", b.model.keel_fwd.pts);
}
