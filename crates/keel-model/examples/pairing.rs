use keel_model::{geometry, Catalogue, Model};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let cat = Catalogue::load_dir(std::path::Path::new(&a[1])).unwrap();
    for p in &a[2..] {
        let m = Model::parse(&std::fs::read_to_string(p).unwrap()).unwrap();
        let (t, c) = geometry::pairing_stats(&m, &cat);
        println!("{} pieces={t} culled={c} ({:.1}%)", p.split('/').rev().nth(1).unwrap(), 100.0 * c as f64 / t.max(1) as f64);
    }
}
