use keel_model::{geometry, Catalogue};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let cat = Catalogue::load_dir(std::path::Path::new(&a[1])).unwrap();
    for (id, d) in &cat.defs {
        if d.def_type == 0 && d.mesh_data_name.is_empty() {
            let (n, m, unused) = geometry::block_piece_stats(d);
            let shapes: Vec<(u8, u8, u8, [i32; 3])> = d.surfaces.iter().map(|s| (s.orientation, s.rotation, s.shape, s.position)).collect();
            if std::env::var("DETAIL").is_ok_and(|v| v == *id) { println!("{:?}", shapes); }
            println!("{id:22} bt={:2} pieces={n:2} matched={m:2} unused_surfaces={unused:2}  {:?}", d.block_type, &shapes[..shapes.len().min(6)]);
        }
    }
}
