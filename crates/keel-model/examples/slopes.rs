use keel_model::{geometry, Catalogue};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let cat = Catalogue::load_dir(std::path::Path::new(&a[1])).unwrap();
    for id in &a[2..] {
        let d = cat.get(id).unwrap();
        println!("{id}: {:?}", d.surfaces.iter().enumerate().map(|(i, s)| format!("{i}:o{}s{}@{:?}", s.orientation, s.shape, s.position)).collect::<Vec<_>>());
        for (n, s) in geometry::slope_assignments(d) {
            println!("   n=({:.2},{:.2},{:.2}) -> {:?}", n[0], n[1], n[2], s);
        }
    }
}
