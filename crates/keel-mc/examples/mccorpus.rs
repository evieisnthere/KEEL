//! Round-trip every microcontroller in the given vehicle files: `mccorpus <vehicle.xml>...`.
use keel_mc::graph::Graph;
fn main() {
    let mut exprs = 0;
    let s0 = |g: &Graph| { let mut s = keel_mc::sim::Sim::default(); s.step(g); s.unsupported };
    let (mut n, mut same, mut graph_ok, mut unsupported) = (0, 0, 0, std::collections::BTreeMap::new());
    for f in std::env::args().skip(1) {
        let Ok(src) = std::fs::read_to_string(&f) else { continue };
        let Ok(doc) = keel_io::xml::parse(&src) else { continue };
        let mut mcs = vec![];
        doc.root.walk(&mut |e| if e.tag == "microprocessor_definition" { mcs.push(e.clone()) });
        for e in mcs {
            n += 1;
            let mut a = String::new();
            e.write(&mut a);
            let g = Graph::parse(&e);
            let mut b = String::new();
            g.to_element().write(&mut b);
            if a == b { same += 1 } else if n - same < 4 {
                let k = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
                let lo = k.saturating_sub(200);
                println!("DIFF {f}\n{}\n{}", &a[lo..(k + 200).min(a.len())], &b[lo..(k + 200).min(b.len())]);
            }
            let g2 = Graph::parse(&keel_io::xml::parse(&b).unwrap().root);
            let key = |g: &Graph| g.comps.iter().map(|c| (c.id, c.ty, c.bridge, c.pos, c.inputs.clone())).collect::<Vec<_>>();
            if key(&g) == key(&g2) && g.nodes.len() == g2.nodes.len() { graph_ok += 1 } else {
                for (x, y) in key(&g).iter().zip(key(&g2).iter()) { if x != y { println!("GRAPH {f} {x:?} {y:?}"); break } }
            }
            for (id, m) in &s0(&g) { if m.starts_with("expression") && exprs < 8 { exprs += 1; println!("EXPR {id} {m} {:?}", g.comp(*id).and_then(|c| c.attr("e"))); } }
            let mut s = keel_mc::sim::Sim::default();
            for _ in 0..3 { s.step(&g); }
            for (_, m) in s.unsupported { *unsupported.entry(m.split(':').next().unwrap().to_string()).or_insert(0) += 1; }
        }
    }
    println!("{n} microcontrollers: {same} byte-identical, {graph_ok} graph-identical after round trip; unsupported {unsupported:?}");
}
