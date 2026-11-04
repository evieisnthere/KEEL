//! Corner masks over a box of voxels (debug): cargo run --example masks -- x0 x1 y0 y1 z0 z1
fn main() {
    let a: Vec<i32> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let h = keel_geom::hull::HullParams::default().build().unwrap();
    let b = keel_geom::hullblocks::build(&h, &Default::default());
    for x in a[0]..=a[1] {
        println!("x = {x}");
        for y in (a[2]..=a[3]).rev() {
            let mut line = format!("  y{y:3} |");
            for z in a[4]..=a[5] {
                let mut m = 0u8;
                for k in 0..8 {
                    let l = [(x + (k & 1)) as f64, (y + ((k >> 1) & 1)) as f64, (z + ((k >> 2) & 1)) as f64];
                    if b.model.inside(l[0], l[1], l[2]) {
                        m |= 1 << k;
                    }
                }
                line += &format!(" {m:3}");
            }
            println!("{line}");
        }
    }
    for lz in a[4]..=a[5] + 1 {
        print!("X({lz})={:?} ", b.model.plan_fwd.across_at(lz as f64));
    }
    println!();
}
