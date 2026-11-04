//! Print voxels, ports and surfaces of definitions: cargo run --example partinfo -- <defs dir> id...
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let c = keel_model::catalogue::Catalogue::load_dir(std::path::Path::new(&a[1])).unwrap();
    for id in &a[2..] {
        let Some(d) = c.get(id) else { println!("{id}: missing"); continue };
        println!("{id} '{}' type {} mass {} voxels {:?} solid {:?}", d.name, d.def_type, d.mass, d.voxels, d.solid_voxels);
        for n in &d.logic_nodes {
            println!("   node '{}' mode {} kind {} orient {} pos {:?}", n.label, n.mode, n.kind, n.orientation, n.position);
        }
        let faces: Vec<_> = d.surfaces.iter().map(|s| (s.orientation, s.shape, s.position)).collect();
        println!("   surfaces {:?}", faces);
    }
}
