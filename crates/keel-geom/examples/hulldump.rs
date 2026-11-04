//! Dump the generated hull's exposed faces as JSON for offline rendering (scripts/hullshot.py).
//! cargo run --release -p keel-geom --example hulldump -- out.json [preset-json]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let params: keel_geom::hull::HullParams = match args.get(2) {
        Some(p) => serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap(),
        None => Default::default(),
    };
    let h = params.build().unwrap();
    let t = std::time::Instant::now();
    let b = keel_geom::hullblocks::build(&h, &Default::default());
    let dt = t.elapsed();
    let m = keel_geom::metrics::measure_with(&b.placements, &h, Some(&|q| b.model_inside_m(q)));
    eprintln!("build {:?}  {}", dt, m.summary());
    let faces: Vec<serde_json::Value> = m
        .faces
        .iter()
        .map(|(f, n, st, bi)| serde_json::json!({"p": f, "n": n, "s": st, "d": b.placements[*bi].def}))
        .collect();
    std::fs::write(&args[1], serde_json::to_string(&serde_json::json!({"faces": faces, "summary": m.summary()})).unwrap()).unwrap();
}
