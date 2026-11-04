//! Tick simulator (RE-0013 §3, §5): input bridges publish, every component latches its inputs from the current
//! outputs, every component computes, output bridges latch. So each component adds one tick of delay, and an input
//! wired through one gate to an output arrives in the same tick. Lua scripts are not run (their outputs stay 0).
use crate::graph::Graph;
use crate::Sig;
use std::collections::HashMap;

/// A signal value. Composite signals carry 32 number and 32 on/off channels.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Value {
    pub n: f64,
    pub b: bool,
    pub ch: Option<Box<([f64; 32], [bool; 32])>>,
}

impl Value {
    pub fn num(n: f64) -> Value {
        Value { n, ..Default::default() }
    }
    pub fn bool(b: bool) -> Value {
        Value { b, ..Default::default() }
    }
    fn channels(&self) -> ([f64; 32], [bool; 32]) {
        self.ch.as_deref().copied().unwrap_or(([0.0; 32], [false; 32]))
    }
    pub fn show(&self, s: Sig) -> String {
        match s {
            Sig::Bool => if self.b { "on" } else { "off" }.into(),
            Sig::Num => crate::graph::fmt_num((self.n * 1e4).round() / 1e4),
            Sig::Comp => {
                let (n, b) = self.channels();
                let used = n.iter().filter(|x| **x != 0.0).count() + b.iter().filter(|x| **x).count();
                format!("{used} ch")
            }
            _ => "—".into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct State {
    v: f64,
    i: f64,
    prev: f64,
    prev_b: bool,
    on: bool,
    count: i64,
    started: bool,
    last_in: Vec<bool>,
}

#[derive(Default)]
pub struct Sim {
    /// outputs of every component (id, output index)
    pub out: HashMap<(i32, usize), Value>,
    /// values driven onto input IO nodes (by node id)
    pub external: HashMap<i32, Value>,
    state: HashMap<i32, State>,
    pub tick: u64,
    /// components the simulator does not model, with the reason
    pub unsupported: Vec<(i32, String)>,
}

fn delta(u: f64) -> f64 {
    if u == 0.0 {
        1.0 / 60.0
    } else {
        1.0
    }
}

impl Sim {
    pub fn reset(&mut self) {
        *self = Sim { external: std::mem::take(&mut self.external), ..Default::default() };
    }

    /// The value on an output IO node (by node id).
    pub fn output(&self, g: &Graph, node: i32) -> Value {
        g.nodes.iter().find(|n| n.id == node).and_then(|n| self.out.get(&(n.comp, 0))).cloned().unwrap_or_default()
    }

    pub fn step(&mut self, g: &Graph) {
        self.tick += 1;
        self.unsupported.clear();
        // a. input bridges publish
        for n in g.nodes.iter().filter(|n| n.input) {
            self.out.insert((n.comp, 0), self.external.get(&n.id).cloned().unwrap_or_default());
        }
        // b. every component latches its inputs
        let latched: HashMap<i32, Vec<Option<Value>>> = g
            .comps
            .iter()
            .filter(|c| !c.bridge)
            .map(|c| (c.id, c.inputs.iter().map(|l| l.map(|l| self.out.get(&(l.src, l.out)).cloned().unwrap_or_default())).collect()))
            .collect();
        // c. every component computes
        let mut new_out: Vec<((i32, usize), Value)> = vec![];
        for c in g.comps.iter().filter(|c| !c.bridge) {
            let ins = &latched[&c.id];
            let get = |k: usize| ins.get(k).cloned().flatten().unwrap_or_default();
            let connected = |k: usize| ins.get(k).is_some_and(|v| v.is_some());
            let n = |k: usize| get(k).n;
            let bl = |k: usize| get(k).b;
            let st = self.state.entry(c.id).or_default();
            let first = !st.started;
            st.started = true;
            let mut put = |i: usize, v: Value| new_out.push(((c.id, i), v));
            let num = |key: &str| c.prop_num(key);
            match c.ty {
                0 => put(0, Value::bool(!bl(0))),
                1 => put(0, Value::bool(bl(0) && bl(1))),
                2 => put(0, Value::bool(bl(0) || bl(1))),
                3 => put(0, Value::bool(bl(0) != bl(1))),
                4 => put(0, Value::bool(!(bl(0) && bl(1)))),
                5 => put(0, Value::bool(!(bl(0) || bl(1)))),
                6 => put(0, Value::num(n(0) + n(1))),
                7 => put(0, Value::num(n(0) - n(1))),
                8 => put(0, Value::num(n(0) * n(1))),
                9 => {
                    let z = n(1).abs() <= 1e-6;
                    put(0, Value::num(if z { 0.0 } else { n(0) / n(1) }));
                    put(1, Value::bool(z));
                }
                10 | 36 | 45 | 46 | 47 => {
                    let names = ["x", "y", "z", "w", "a", "b", "c", "d"];
                    let boolean = matches!(c.ty, 46 | 47);
                    let vars: HashMap<&str, f64> = names.iter().enumerate().map(|(k, nm)| (*nm, if boolean { bl(k) as i32 as f64 } else { n(k) })).collect();
                    let e = c.attr("e").unwrap_or_default();
                    match crate::expr::eval(&e, &vars, boolean) {
                        Ok(v) => put(0, if boolean { Value::bool(v != 0.0) } else { Value::num(v) }),
                        Err(m) => {
                            self.unsupported.push((c.id, format!("expression: {m}")));
                            put(0, Value::default());
                        }
                    }
                }
                11 => {
                    let (lo, hi) = (num("min"), num("max"));
                    put(0, Value::num(n(0).clamp(lo.min(hi), lo.max(hi))));
                }
                12 => {
                    let (lo, hi) = (num("min"), num("max"));
                    put(0, Value::bool(n(0) >= lo.min(hi) && n(0) <= lo.max(hi)));
                }
                13 => {
                    if first {
                        st.v = c.attr("memory").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    }
                    if bl(1) {
                        st.v = num("r");
                    } else if bl(0) {
                        st.v = n(2);
                    }
                    put(0, Value::num(st.v));
                }
                14 => put(0, Value::num(n(0).abs())),
                15 => put(0, Value::num(num("n"))),
                16 => put(0, Value::bool(true)),
                17 => put(0, Value::bool(n(0) > n(1))),
                18 => put(0, Value::bool(n(0) < n(1))),
                19 | 34 => put(0, Value::num(num("v"))),
                20 => put(0, Value::num(num("i"))),
                21 => {
                    put(0, Value::num(if bl(1) { n(0) } else { 0.0 }));
                    put(1, Value::num(if bl(1) { 0.0 } else { n(0) }));
                }
                22 => put(0, Value::num(if bl(2) { n(0) } else { n(1) })),
                23 | 39 => {
                    let (active, sp, pv) = if c.ty == 23 { (bl(2), n(0), n(1)) } else { (bl(5), n(0), n(1)) };
                    let (kp, ki, kd) = if c.ty == 23 { (num("kp"), num("ki"), num("kd")) } else { (n(2), n(3), n(4)) };
                    if active {
                        let e = sp - pv;
                        st.i += e;
                        let d = if st.on { e - st.prev } else { 0.0 };
                        st.prev = e;
                        st.on = true;
                        put(0, Value::num(kp * e + ki * st.i + kd * d));
                    } else {
                        st.i = 0.0;
                        st.on = false;
                        put(0, Value::num(0.0));
                    }
                }
                24 | 25 => {
                    let now = vec![bl(0), bl(1)];
                    if st.last_in != now {
                        match (now[0], now[1]) {
                            (true, true) if c.ty == 25 => st.on = !st.on,
                            (_, true) => st.on = false,
                            (true, false) => st.on = true,
                            _ => {}
                        }
                        st.last_in = now;
                    }
                    put(0, Value::bool(st.on));
                    put(1, Value::bool(!st.on));
                }
                26 => {
                    let inp = bl(0);
                    if inp != st.prev_b {
                        st.count = 0;
                        st.prev_b = inp;
                    }
                    st.count += 1;
                    let need = ((if inp { num("ct") } else { num("dt") }) * 60.0).round() as i64;
                    if st.count >= need {
                        st.on = inp;
                    }
                    put(0, Value::bool(st.on));
                }
                27 => {
                    if bl(0) {
                        let (on, off) = ((num("on") * 60.0).round().max(1.0) as i64, (num("off") * 60.0).round().max(1.0) as i64);
                        let phase = st.count % (on + off);
                        st.count += 1;
                        put(0, Value::bool(phase < on));
                    } else {
                        st.count = 0;
                        put(0, Value::bool(false));
                    }
                }
                28 => {
                    if bl(0) && !st.prev_b {
                        st.on = !st.on;
                    }
                    st.prev_b = bl(0);
                    put(0, Value::bool(st.on));
                }
                29 | 31 => {
                    let ch = if connected(1) { n(1).round() as i64 - 1 } else { num("i") as i64 };
                    let (nn, bb) = get(0).channels();
                    let k = ch.clamp(0, 31) as usize;
                    put(0, if c.ty == 29 { Value::bool(bb[k]) } else { Value::num(nn[k]) });
                }
                30 | 32 => {
                    let k = (num("i") as i64).clamp(0, 31) as usize;
                    let (mut nn, mut bb) = get(0).channels();
                    if c.ty == 30 {
                        bb[k] = bl(1);
                    } else {
                        nn[k] = n(1);
                    }
                    put(0, Value { ch: Some(Box::new((nn, bb))), ..Default::default() });
                }
                40 | 41 => {
                    let (mut nn, mut bb) = get(0).channels();
                    let off = num("offset") as usize;
                    for k in 0..c.count() {
                        if connected(k + 1) && off + k < 32 {
                            if c.ty == 40 {
                                nn[off + k] = n(k + 1);
                            } else {
                                bb[off + k] = bl(k + 1);
                            }
                        }
                    }
                    put(0, Value { ch: Some(Box::new((nn, bb))), ..Default::default() });
                }
                33 => put(0, Value::bool(c.attr("v").is_some_and(|v| v == "true" || v == "1"))),
                35 => {
                    let d = if first { 0.0 } else { n(0) - st.prev };
                    st.prev = n(0);
                    put(0, Value::num(d));
                }
                37 => {
                    if first || bl(2) {
                        st.v = num("r");
                    } else if bl(0) {
                        st.v += num("i");
                    } else if bl(1) {
                        st.v -= num("i");
                    }
                    if num("m") == 1.0 {
                        let (lo, hi) = (num("min"), num("max"));
                        st.v = st.v.clamp(lo.min(hi), lo.max(hi));
                    }
                    put(0, Value::num(st.v));
                }
                38 => put(0, Value::num(if n(1).abs() <= 1e-6 { 0.0 } else { n(0) % n(1) })),
                42 => put(0, Value::bool((n(0) - n(1)).abs() < num("e"))),
                48 => {
                    let (now, was) = (bl(0), st.prev_b);
                    st.prev_b = now;
                    let m = num("m") as i32;
                    put(0, Value::bool(match m {
                        1 => was && !now,
                        2 => was != now,
                        _ => now && !was,
                    } && !first));
                }
                49..=52 => {
                    let retentive = c.ty >= 51;
                    if retentive && bl(2) {
                        st.v = 0.0;
                    }
                    if bl(0) {
                        st.v = (st.v + delta(num("u"))).min(n(1));
                    } else if !retentive {
                        st.v = 0.0;
                    }
                    let complete = st.v >= n(1);
                    put(0, Value::bool(if c.ty == 49 || c.ty == 51 { complete } else { !complete && bl(0) }));
                }
                53 => put(0, if bl(2) { get(0) } else { get(1) }),
                54 => {
                    let bits = n(0) as u32;
                    let mut bb = [false; 32];
                    for (k, b) in bb.iter_mut().enumerate() {
                        *b = bits >> k & 1 == 1;
                    }
                    put(0, Value { ch: Some(Box::new(([0.0; 32], bb))), ..Default::default() });
                }
                55 => {
                    let (_, bb) = get(0).channels();
                    put(0, Value::num(bb.iter().enumerate().map(|(k, b)| if *b { (1u64 << k) as f64 } else { 0.0 }).sum()));
                }
                56 => self.unsupported.push((c.id, "Lua scripts are not simulated".into())),
                57 | 59 => self.unsupported.push((c.id, "video and audio are not simulated".into())),
                _ => {}
            }
        }
        for (k, v) in new_out {
            self.out.insert(k, v);
        }
        // e. output bridges latch
        for c in g.comps.iter().filter(|c| c.bridge && !crate::Sig::from_bridge_type(c.ty).1) {
            let v = c.inputs.first().copied().flatten().and_then(|l| self.out.get(&(l.src, l.out)).cloned()).unwrap_or_default();
            self.out.insert((c.id, 0), v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_delay_and_toggle() {
        let mut g = Graph::new_empty("t", 2, 1);
        let i = g.add_io(true, Sig::Bool, "In", [-2.0, 0.0]);
        let o = g.add_io(false, Sig::Bool, "Out", [2.0, 0.0]);
        let (ib, ob) = (g.nodes[0].comp, g.nodes[1].comp);
        let t = g.add(28, [0.0, 0.0]);
        g.connect(ib, 0, t, 0).unwrap();
        g.connect(t, 0, ob, 0).unwrap();
        let mut s = Sim::default();
        s.external.insert(i, Value::bool(true));
        s.step(&g);
        // input → one gate → output: same tick
        assert!(s.output(&g, o).b);
        s.external.insert(i, Value::bool(false));
        s.step(&g);
        assert!(s.output(&g, o).b);
        s.external.insert(i, Value::bool(true));
        s.step(&g);
        assert!(!s.output(&g, o).b);
    }

    #[test]
    fn expression_and_capacitor() {
        let mut g = Graph::new_empty("t", 1, 1);
        g.add_io(true, Sig::Num, "x", [-2.0, 0.0]);
        let f = g.add(45, [0.0, 0.0]);
        let ib = g.nodes[0].comp;
        g.connect(ib, 0, f, 0).unwrap();
        let mut s = Sim::default();
        s.external.insert(g.nodes[0].id, Value::num(3.0));
        s.step(&g);
        assert_eq!(s.out[&(f, 0)].n, 3.0);
        let k = g.add(26, [0.0, 1.0]);
        let on = g.add(16, [-1.0, 1.0]);
        g.connect(on, 0, k, 0).unwrap();
        let mut s = Sim::default();
        for _ in 0..60 {
            s.step(&g);
        }
        assert!(!s.out[&(k, 0)].b);
        s.step(&g);
        assert!(s.out[&(k, 0)].b);
    }
}
