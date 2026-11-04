use keel_model::{geometry, Catalogue, Model};
use std::collections::BTreeMap;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let cat = Catalogue::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut lib = geometry::MeshLibrary::new(Some(a[2].clone().into()));
    for p in &a[3..] {
        let m = Model::parse(&std::fs::read_to_string(p).unwrap()).unwrap();
        lib.missing.clear();
        let mut uses: BTreeMap<String, usize> = BTreeMap::new();
        for (_, c) in m.components() {
            if let Some(d) = cat.get(&c.def) {
                if !d.mesh_data_name.is_empty() && lib.get(&d.mesh_data_name).is_none() {
                    *uses.entry(format!("{} ({})", c.def, d.mesh_data_name)).or_default() += 1;
                }
            } else {
                *uses.entry(format!("{} (no definition)", c.def)).or_default() += 1;
            }
        }
        let mut v: Vec<_> = uses.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        println!("{}: {} missing kinds, {} parts", p.split('/').rev().nth(1).unwrap(), v.len(), v.iter().map(|x| x.1).sum::<usize>());
        for (k, n) in v.iter().take(12) { println!("   {n:5} {k}"); }
    }
}
