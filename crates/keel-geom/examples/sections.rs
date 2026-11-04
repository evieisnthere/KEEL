//! Print half-breadth (voxels) per row for sections along z (debug the hull ends).
fn main() {
    let h = keel_geom::hull::HullParams::default().build().unwrap();
    let p = &h.params;
    println!("L {} B {} D {} draft {}", p.length, p.beam, p.depth, p.draft);
    let v = 0.25;
    let mut z = -p.length / 2.0;
    while z <= p.length / 2.0 + 1e-9 {
        let s = h.section_at(z, 64);
        let row: String = match &s {
            Some(s) => (0..16).map(|r| match keel_geom::hull::Hull::half_breadth(s, r as f64 * v + 0.01) { Some(b) => format!("{:5.1}", b / v), None => "    .".into() }).collect(),
            None => "none".into(),
        };
        println!("z {:6.2} (vz {:4}) {}", z, (z / v).round(), row);
        z += 0.5;
    }
}
