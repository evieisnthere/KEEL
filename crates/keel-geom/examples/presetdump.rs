//! Dump the generated hull for a named preset: cargo run --example presetdump -- <name> out.json
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let p = keel_geom::hull::presets().into_iter().find(|(n, _)| n.eq_ignore_ascii_case(&a[1])).expect("preset").1;
    std::fs::write(&a[2], serde_json::to_string(&p).unwrap()).unwrap();
}
