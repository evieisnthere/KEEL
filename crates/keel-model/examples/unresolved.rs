use keel_model::{geometry, Catalogue, Model};
use std::collections::BTreeMap;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let cat = Catalogue::load_dir(std::path::Path::new(&a[1])).unwrap();
    let t = geometry::tables();
    let mut by_shape: BTreeMap<u8, (usize, BTreeMap<String, usize>)> = BTreeMap::new();
    for p in &a[2..] {
        let m = Model::parse(&std::fs::read_to_string(p).unwrap()).unwrap();
        for (_, c) in m.components() {
            let Some(d) = cat.get(&c.def) else { continue };
            if d.mesh_data_name.is_empty() { continue; }
            for s in &d.surfaces {
                if s.shape != 0 && !t.conv.contains_key(&(s.orientation, s.rotation, s.shape)) {
                    let e = by_shape.entry(s.shape).or_default();
                    e.0 += 1;
                    *e.1.entry(c.def.clone()).or_default() += 1;
                }
            }
        }
    }
    for (s, (n, defs)) in by_shape {
        let mut v: Vec<_> = defs.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        println!("shape {s:2}: {n:6} surfaces  e.g. {:?}", &v[..v.len().min(5)]);
    }
}
