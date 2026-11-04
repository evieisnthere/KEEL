use keel_model::{diagnostics, geometry, Catalogue, Model};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = std::time::Instant::now();
    let cat = Catalogue::load_dir(std::path::Path::new(&a[1])).unwrap();
    println!("catalogue: {} defs, {} errors ({:?})", cat.defs.len(), cat.errors.len(), t.elapsed());
    for p in &a[2..] {
        let t = std::time::Instant::now();
        let m = Model::parse(&std::fs::read_to_string(p).unwrap()).unwrap();
        let faces = geometry::build_faces(&m, &cat);
        let d = diagnostics::check(&m, &cat);
        println!("{p}: {} comps, {} bodies, {} faces, {:?}", m.components().len(), m.body_count(), faces.len(), t.elapsed());
        for x in d { println!("  {:?} {} {}", x.severity, x.code, x.message); }
    }
}
