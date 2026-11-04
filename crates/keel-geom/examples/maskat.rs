//! Print model corner labels and placed blocks around a voxel (debug): cargo run --example maskat -- x y z
fn main() {
    let a: Vec<i32> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let h = keel_geom::hull::HullParams::default().build().unwrap();
    let b = keel_geom::hullblocks::build(&h, &Default::default());
    for dz in -1..=1 {
        for dy in (-1..=1).rev() {
            let mut line = String::new();
            for dx in -2..=2 {
                let v = [a[0] + dx, a[1] + dy, a[2] + dz];
                let mut m = 0u8;
                for k in 0..8 {
                    let q = [(v[0] + (k & 1)) as f64 - 0.5, (v[1] + ((k >> 1) & 1)) as f64, (v[2] + ((k >> 2) & 1)) as f64 - 0.5];
                    if b.model_inside_m([q[0] * 0.25, q[1] * 0.25, q[2] * 0.25]) {
                        m |= 1 << k;
                    }
                }
                let p = b.placements.iter().zip(&b.sources).find(|(p, _)| p.vp == v).map(|(p, s)| format!("{}:{}", &s[..2], &p.def[..2])).unwrap_or("--".into());
                line += &format!(" {:3}{:>8}", m, p);
            }
            println!("z{:+} y{:+} |{line}", dz, dy);
        }
        println!();
    }
}
