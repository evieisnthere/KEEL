//! Histogram of step faces (debug).
fn main() {
    let h = keel_geom::hull::HullParams::default().build().unwrap();
    let b = keel_geom::hullblocks::build(&h, &Default::default());
    let m = keel_geom::metrics::measure_with(&b.placements, &h, Some(&|q| b.model_inside_m(q)));
    println!("{}", m.summary());
    let mut hist: std::collections::BTreeMap<String, (usize, f64, f64)> = Default::default();
    for (f, n, st, _) in &m.faces {
        if !*st {
            continue;
        }
        let c = [0, 1, 2].map(|i| f.iter().map(|p| p[i]).sum::<f64>() / f.len() as f64);
        if c[2].abs() > 7.0 {
            continue;
        }
        let k = format!("n {:+.0},{:+.0},{:+.0} y {:.2} |x| {:.2}", n[0], n[1], n[2], c[1], c[0].abs());
        let e = hist.entry(k).or_insert((0, f64::MAX, f64::MIN));
        // area in cm²
        let mut cr = [0.0; 3];
        for i in 0..f.len() {
            let (u, w) = (f[i], f[(i + 1) % f.len()]);
            cr[0] += u[1] * w[2] - u[2] * w[1];
            cr[1] += u[2] * w[0] - u[0] * w[2];
            cr[2] += u[0] * w[1] - u[1] * w[0];
        }
        e.0 += (5000.0 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt()) as usize;
        e.1 = e.1.min(c[2]);
        e.2 = e.2.max(c[2]);
    }
    let mut v: Vec<_> = hist.into_iter().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1 .0));
    for (k, (n, a, b)) in v.iter().take(25) {
        println!("{n:4} {k} z {a:.2}..{b:.2}");
    }
}
