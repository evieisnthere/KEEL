//! Propulsion installer: puts the drive designed in Power mode into the boat and connects everything.
//!
//! Per shaft line:
//! * **Engine** (1×1 modular): crankshaft line along the keel with cylinders on top, the air, fuel and coolant
//!   manifolds beside the cylinders, an exhaust manifold on top, the clutch at the aft end, and at the forward end a
//!   drive belt with starter, alternator and the belt-driven coolant pump. This follows the arrangement of working
//!   engines in the vehicle corpus (OPEN-ITEMS C2: to be confirmed in game).
//! * **Or an electric motor** fed from a battery bank.
//! * A 1×1 gearbox on the clutch (ratio from Power mode), then the shaft to a propeller under the stern: pipe pieces
//!   chosen by an A* router, enclosed (watertight) pieces where it passes through the hull or a floor.
//! * A fin rudder behind the propeller.
//! * **Fuel**: stacked diesel tanks, an electric fuel pump, and pipes to the fuel manifold.
//! * **Air and exhaust**: pipes up through the deck to an intake and an exhaust outlet.
//! * **Cooling**: coolant manifold → belt pump → radiator → water header tank → coolant manifold.
//! * **Controls**: helm, throttle lever and key on the bridge (or on deck), an engine-controller microcontroller
//!   (idle throttle, clutch engagement, starter cut-out), and a battery feeding everything electric.
//!
//! Everything is regenerated together with the interior as one undo step.
use crate::build::{dir_vec, orient, Build, PartExtra, ID};
use crate::plan::Plan;
use crate::programme::RoomKind;
use keel_model::edit::port_voxel;
use std::collections::{BinaryHeap, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct PropulsionOptions {
    pub electric: bool,
    /// 1×1 cylinders per engine
    pub cylinders: usize,
    /// electric motor size 0 small, 1 medium, 2 large
    pub motor: usize,
    /// gearbox ratio index (game order: 0 reverse, 1 1:1, … 7 3:1)
    pub gear: usize,
    /// 0 small propeller, 1 large, 2 giga
    pub prop: usize,
    /// shaft lines (1 or 2)
    pub lines: usize,
    /// fuel tanks per engine (medium tanks, stacked)
    pub tanks: usize,
    pub batteries: usize,
    pub rudder: bool,
    pub colour: String,
}

impl Default for PropulsionOptions {
    fn default() -> Self {
        PropulsionOptions { electric: false, cylinders: 4, motor: 1, gear: 5, prop: 0, lines: 1, tanks: 1, batteries: 1, rudder: true, colour: "4A4F63".into() }
    }
}

/// What the installer did, for the user.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

// ---- geometry helpers ---------------------------------------------------------------------------------------------

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn neg(a: [i32; 3]) -> [i32; 3] {
    [-a[0], -a[1], -a[2]]
}
/// R·v for a column-major rotation.
fn rot(r: &[i32; 9], v: [i32; 3]) -> [i32; 3] {
    [0, 1, 2].map(|i| r[i] * v[0] + r[3 + i] * v[1] + r[6 + i] * v[2])
}
fn dir(d: usize) -> [i32; 3] {
    dir_vec(d)
}
fn dir_index(v: [i32; 3]) -> usize {
    (0..6).find(|&d| dir_vec(d) == v).unwrap_or(0)
}

/// The 24 proper rotations.
fn rotations() -> Vec<[i32; 9]> {
    let mut out = vec![];
    for up in 0..6 {
        for fwd in 0..6 {
            let (a, b) = (dir(up), dir(fwd));
            if a[0] * b[0] + a[1] * b[1] + a[2] * b[2] == 0 {
                out.push(orient(up, fwd));
            }
        }
    }
    out
}

/// A port of a placed part: voxel and facing direction (physical ports connect to whatever sits in front of them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Port {
    pub voxel: [i32; 3],
    pub facing: [i32; 3],
}

fn port(vp: [i32; 3], r: &[i32; 9], pos: [i32; 3], orient: usize) -> Port {
    Port { voxel: port_voxel(vp, r, pos), facing: rot(r, dir(orient)) }
}

/// Footprints (local voxels) of the multi-voxel parts the installer places.
fn footprint(def: &str) -> Vec<[i32; 3]> {
    let cube = |r: i32| {
        let mut v = vec![];
        for x in -r..=r {
            for y in -r..=r {
                for z in -r..=r {
                    v.push([x, y, z]);
                }
            }
        }
        v
    };
    match def {
        "water_pump" => vec![[-1, 0, 0], [0, 0, 0], [1, 0, 0]],
        "fluid_tank_medium" => (0..2).flat_map(|x| (-1..1).flat_map(move |y| (-1..2).map(move |z| [x, y, z]))).collect(),
        "fluid_tank_small" => vec![[0, 0, 0], [0, 0, -1]],
        "fluid_intake" => (-1..2).flat_map(|x| (0..2).map(move |y| [x, y, 0])).collect(),
        "fluid_exhaust" => vec![[0, 0, 0], [0, 1, 0]],
        "fluid_radiator" => (-1..2).flat_map(|x| (-1..2).map(move |y| [x, y, 0])).collect(),
        "battery_small" => vec![[0, 0, 0], [-1, 0, 0]],
        "propeller" => vec![[0, -1, 0], [0, 0, 0], [0, 1, 0]],
        "large_propeller" | "giga_prop_small" => (-2..3).flat_map(|x| (-2..3).map(move |z| [x, 1, z])).chain([[0, 0, 0]]).collect(),
        "rudder_surface" => vec![[0, 0, -1], [0, 0, 0], [0, 0, 1], [0, 1, -1], [0, 1, 0], [0, 1, 1]],
        "seat_helm" => (-1..2).flat_map(|x| (-3..2).flat_map(move |y| (-4..1).map(move |z| [x, y, z]))).collect(),
        "button_throttle_lever" => vec![[0, 0, 0], [0, 1, 0], [1, 0, 0], [1, 1, 0]],
        "button_key" => vec![[0, 0, 0], [0, 1, 0]],
        "motor_medium" => cube(1),
        "motor_large" => cube(2),
        _ => vec![[0, 0, 0]],
    }
}

// ---- occupancy ----------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Cell {
    /// a hull block: a pipe may replace it with an enclosed piece (the app restores it if the route moves)
    Hull,
    /// a generated plain block (floor, wall): a pipe may replace it
    Structure(usize),
    Blocked,
}

#[derive(Clone)]
struct Space<'a> {
    cells: HashMap<[i32; 3], Cell>,
    inside: &'a dyn Fn([i32; 3]) -> bool,
}

impl Space<'_> {
    /// Unoccupied (anywhere: engine room, deck, superstructure, water).
    fn free(&self, v: [i32; 3]) -> bool {
        !self.cells.contains_key(&v)
    }
    fn take(&mut self, v: [i32; 3]) {
        self.cells.insert(v, Cell::Blocked);
    }
    fn fits(&self, def: &str, vp: [i32; 3], r: &[i32; 9]) -> bool {
        footprint(def).iter().all(|l| self.free(add(vp, rot(r, *l))))
    }
    fn place(&mut self, b: &mut Build, def: &str, vp: [i32; 3], r: [i32; 9], colour: &str) {
        for l in footprint(def) {
            self.take(add(vp, rot(&r, l)));
        }
        b.part(def, vp, r, Some(colour));
    }
}

// ---- pipe router --------------------------------------------------------------------------------------------------

/// A routed pipe: cells with their two open ends, and the voxels it had to clear.
struct Route {
    cells: Vec<([i32; 3], [usize; 2], bool)>,
}

/// Route a pipe from port `a` to port `b` (both facing into the first/last pipe cell). Allowed cells: free cells
/// inside the hull, hull blocks (cost 6, replaced by enclosed pieces), generated plain blocks (cost 3), and the
/// `extra` cells (e.g. water under the stern around a propeller).
fn route(sp: &Space, a: Port, b: Port, extra: &HashSet<[i32; 3]>, limit: usize) -> Option<Route> {
    let start = add(a.voxel, a.facing);
    let goal = add(b.voxel, b.facing);
    let cost_of = |v: [i32; 3]| -> Option<u32> {
        match sp.cells.get(&v) {
            Some(Cell::Hull) => Some(60),
            Some(Cell::Structure(_)) => Some(30),
            Some(Cell::Blocked) => None,
            None if extra.contains(&v) || (sp.inside)(v) => Some(10),
            None => None,
        }
    };
    cost_of(start)?;
    cost_of(goal)?;
    let h = |v: [i32; 3]| ((v[0] - goal[0]).abs() + (v[1] - goal[1]).abs() + (v[2] - goal[2]).abs()) as u32 * 10;
    // state: cell and the direction the pipe arrived from (index of the open end pointing back)
    type St = ([i32; 3], usize);
    let mut best: HashMap<St, u32> = HashMap::new();
    let mut prev: HashMap<St, St> = HashMap::new();
    let mut heap = BinaryHeap::new();
    let back0 = dir_index(neg(a.facing));
    let s0 = (start, back0);
    best.insert(s0, cost_of(start)?);
    heap.push(std::cmp::Reverse((h(start) + cost_of(start)?, s0)));
    let want_end = dir_index(neg(b.facing));
    let mut n = 0;
    while let Some(std::cmp::Reverse((_, st))) = heap.pop() {
        n += 1;
        if n > limit {
            return None;
        }
        let (v, back) = st;
        let g = best[&st];
        if v == goal && back != want_end {
            // finish: the last cell's ends are `back` and `want_end`
            let mut cells = vec![];
            let mut cur = st;
            let mut next_end = want_end;
            loop {
                let (cv, cb) = cur;
                let replaced = matches!(sp.cells.get(&cv), Some(Cell::Hull) | Some(Cell::Structure(_)));
                cells.push((cv, [cb, next_end], replaced));
                match prev.get(&cur) {
                    Some(p) => {
                        next_end = dir_index(neg(dir(cb)));
                        cur = *p;
                    }
                    None => break,
                }
            }
            cells.reverse();
            return Some(Route { cells });
        }
        for d in 0..6 {
            if d == back {
                continue;
            }
            let nv = add(v, dir(d));
            let Some(c) = cost_of(nv) else { continue };
            let turn = if d == dir_index(neg(dir(back))) { 0 } else { 4 };
            let ns = (nv, dir_index(neg(dir(d))));
            let ng = g + c + turn;
            if best.get(&ns).is_none_or(|&o| ng < o) {
                best.insert(ns, ng);
                prev.insert(ns, st);
                heap.push(std::cmp::Reverse((ng + h(nv), ns)));
            }
        }
    }
    None
}

/// Place the pieces of a route: straight or angle, enclosed where replacing a block.
fn lay(sp: &mut Space, b: &mut Build, r: &Route, colour: &str) {
    let rots = rotations();
    for &(v, [e1, e2], replaced) in &r.cells {
        let (d1, d2) = (dir(e1), dir(e2));
        let straight = d1 == neg(d2);
        let def = match (straight, replaced) {
            (true, false) => "trans_straight",
            (false, false) => "trans_angle",
            (true, true) => "trans_block_straight",
            (false, true) => "trans_block_angle",
        };
        // local open ends: straight ±x; angle −x and +y
        let local: [[i32; 3]; 2] = if straight { [[1, 0, 0], [-1, 0, 0]] } else { [[-1, 0, 0], [0, 1, 0]] };
        let r = rots
            .iter()
            .find(|r| {
                let w = [rot(r, local[0]), rot(r, local[1])];
                (w[0] == d1 && w[1] == d2) || (w[0] == d2 && w[1] == d1)
            })
            .copied()
            .unwrap_or(ID);
        match sp.cells.get(&v) {
            Some(Cell::Hull) => b.clear.push(v),
            Some(Cell::Structure(i)) => b.parts[*i].def = "__removed__".into(),
            _ => {}
        }
        sp.take(v);
        b.part(def, v, r, Some(colour));
    }
}

fn connect(sp: &mut Space, b: &mut Build, rep: &mut Report, what: &str, a: Port, z: Port, extra: &HashSet<[i32; 3]>, colour: &str) -> bool {
    match route(sp, a, z, extra, 400_000) {
        Some(r) => {
            let n = r.cells.len();
            lay(sp, b, &r, colour);
            rep.notes.push(format!("{what}: {n} pipe pieces"));
            true
        }
        None => {
            rep.warnings.push(format!("Could not route the {what}; connect it by hand"));
            false
        }
    }
}

// ---- the engine controller microcontroller ------------------------------------------------------------------------

/// The engine controller: idle throttle while the key is on, clutch engaging with the throttle, and the starter
/// running until the crank turns, and full clutch pressure for the belt accessories (coolant pump, alternator) while
/// the key is on. Ports (cell x, z): Throttle (0,0), Key (1,0), RPS (2,0) in; Engine throttle (0,1), Clutch (1,1),
/// Starter (2,1), Accessories (3,1) out.
pub fn engine_controller() -> String {
    use keel_mc::graph::Graph;
    use keel_mc::Sig;
    let mut g = Graph::new_empty("KEEL engine controller", 4, 2);
    g.description = "Generated by KEEL: idle throttle, clutch and starter for a modular engine".into();
    let ins: Vec<i32> = [("Throttle", Sig::Num), ("Key", Sig::Bool), ("RPS", Sig::Num)].iter().map(|(l, s)| g.add_io(true, *s, l, [-3.0, 0.0])).collect();
    let outs: Vec<i32> = [("Engine throttle", Sig::Num), ("Clutch", Sig::Num), ("Starter", Sig::Bool), ("Accessories", Sig::Num)].iter().map(|(l, s)| g.add_io(false, *s, l, [3.0, 0.0])).collect();
    // IO cells: inputs on the first row, outputs on the second; bridges tidily left and right
    for (k, nid) in ins.iter().enumerate() {
        let n = g.nodes.iter_mut().find(|n| n.id == *nid).unwrap();
        n.pos = [k as i32, 0];
        let cid = n.comp;
        g.comp_mut(cid).unwrap().pos = [-3.5, 1.0 - k as f32];
    }
    for (k, nid) in outs.iter().enumerate() {
        let n = g.nodes.iter_mut().find(|n| n.id == *nid).unwrap();
        n.pos = [k as i32, 1];
        let cid = n.comp;
        g.comp_mut(cid).unwrap().pos = [3.0, 1.0 - k as f32];
    }
    let bridge = |g: &Graph, nid: i32| g.nodes.iter().find(|n| n.id == nid).unwrap().comp;
    let (t_in, k_in, r_in) = (bridge(&g, ins[0]), bridge(&g, ins[1]), bridge(&g, ins[2]));
    let (t_out, c_out, s_out, a_out) = (bridge(&g, outs[0]), bridge(&g, outs[1]), bridge(&g, outs[2]), bridge(&g, outs[3]));
    let set = |g: &mut Graph, id: i32, key: &str, v: &str| {
        let t = keel_mc::comp_type(g.comp(id).unwrap().ty, 1).unwrap();
        if let Some(p) = t.props.iter().find(|p| p.key == key) {
            g.comp_mut(id).unwrap().set_prop(p, v);
        }
    };
    let idle = g.add(45, [-2.0, 1.0]);
    set(&mut g, idle, "e", "max(x,0.12)");
    let zero = g.add(15, [-2.0, 0.25]);
    set(&mut g, zero, "n", "0");
    let sw_t = g.add(22, [0.0, 1.0]);
    let clutch = g.add(45, [-2.0, -0.5]);
    set(&mut g, clutch, "e", "clamp((x-0.05)*4,0,1)");
    let sw_c = g.add(22, [0.0, -0.25]);
    let four = g.add(15, [-2.0, -1.5]);
    set(&mut g, four, "n", "4");
    let lt = g.add(18, [-0.5, -1.75]);
    let and = g.add(1, [1.0, -1.75]);
    let one = g.add(15, [-2.0, -2.5]);
    set(&mut g, one, "n", "1");
    let sw_a = g.add(22, [0.0, -2.75]);
    for (s, o, d, i) in [
        (t_in, 0, idle, 0),
        (idle, 0, sw_t, 0),
        (zero, 0, sw_t, 1),
        (k_in, 0, sw_t, 2),
        (sw_t, 0, t_out, 0),
        (t_in, 0, clutch, 0),
        (clutch, 0, sw_c, 0),
        (zero, 0, sw_c, 1),
        (k_in, 0, sw_c, 2),
        (sw_c, 0, c_out, 0),
        (r_in, 0, lt, 0),
        (four, 0, lt, 1),
        (k_in, 0, and, 0),
        (lt, 0, and, 1),
        (and, 0, s_out, 0),
        (one, 0, sw_a, 0),
        (zero, 0, sw_a, 1),
        (k_in, 0, sw_a, 2),
        (sw_a, 0, a_out, 0),
    ] {
        g.connect(s, o, d, i).expect("engine controller wiring");
    }
    let mut s = String::new();
    g.to_element().write(&mut s);
    s
}

// ---- the installer ------------------------------------------------------------------------------------------------

/// Kinds of logic link.
const BOOL: i32 = 0;
const NUM: i32 = 1;
const ELEC: i32 = 4;

/// Install the propulsion into a generated layout. `hull` holds the hull's block voxels; `inside` tests the hull's
/// interior; `build` is everything generated so far (structure, furniture, electrics), whose plain blocks pipes may
/// replace.
pub fn install(plan: &Plan, hull: &HashSet<[i32; 3]>, inside: &dyn Fn([i32; 3]) -> bool, build: &mut Build, opt: &PropulsionOptions) -> Report {
    let mut rep = Report::default();
    let colour = opt.colour.as_str();
    let mut cells: HashMap<[i32; 3], Cell> = hull.iter().map(|v| (*v, Cell::Hull)).collect();
    for (i, p) in build.parts.iter().enumerate() {
        let plain = matches!(p.def.as_str(), "01_block" | "02_wedge" | "03_pyramid" | "block_floor");
        cells.insert(p.vp, if plain { Cell::Structure(i) } else { Cell::Blocked });
    }
    let mut sp = Space { cells, inside };
    let mut b = Build::default();

    // engine room (or the aft part of the lowest deck)
    let room = plan.rooms.iter().find(|r| r.kind == RoomKind::EngineRoom);
    let (floor_y, room_cells): (i32, HashSet<(i32, i32)>) = match room {
        Some(r) => (plan.decks[r.deck].floor_y, r.cells.iter().copied().collect()),
        None => match plan.decks.first() {
            Some(d) => {
                let zmin = d.cells.iter().map(|c| c.1).min().unwrap_or(0);
                let zmax = d.cells.iter().map(|c| c.1).max().unwrap_or(0);
                let cut = zmin + (zmax - zmin) * 2 / 5;
                rep.warnings.push("No engine room in the plan: using the aft part of the lowest deck".into());
                (d.floor_y, d.cells.iter().filter(|c| c.1 <= cut).copied().collect())
            }
            None => {
                rep.warnings.push("No decks to install the propulsion in".into());
                return rep;
            }
        },
    };
    let y0 = floor_y + 1;
    let in_room = |v: [i32; 3]| room_cells.contains(&(v[0], v[2]));

    // hull bottom per column, stern
    let mut bottom: HashMap<(i32, i32), i32> = HashMap::new();
    let mut top: HashMap<(i32, i32), i32> = HashMap::new();
    for v in hull {
        let e = bottom.entry((v[0], v[2])).or_insert(v[1]);
        *e = (*e).min(v[1]);
        let t = top.entry((v[0], v[2])).or_insert(v[1]);
        *t = (*t).max(v[1]);
    }
    let z_stern = hull.iter().map(|v| v[2]).min().unwrap_or(0);

    let lines = opt.lines.clamp(1, 2);
    let mut battery_ports: Vec<Port> = vec![];
    let mut elec_users: Vec<[i32; 3]> = vec![];
    let mut controls: Vec<(String, [i32; 3], i32)> = vec![]; // (what, port voxel, kind) for helm/lever wiring
    let crank_r = orient(5, 2); // local +Y aft, +Z up: crank, clutch, gearbox
    for line in 0..lines {
        let s: i32 = if lines == 1 { -1 } else if line == 0 { -1 } else { 1 };
        let x0 = if lines == 1 { 0 } else { s * 3 };
        // ---- drive unit
        let n = opt.cylinders.clamp(2, 12) as i32;
        // engine cells relative to the clutch z (zc): gearbox zc−1 … belt zc+n+1
        let engine_cells = |zc: i32| -> Vec<[i32; 3]> {
            let mut v = vec![[x0, y0, zc - 1], [x0, y0, zc]];
            for k in 1..=n {
                v.push([x0, y0, zc + k]);
                v.push([x0, y0 + 1, zc + k]);
            }
            v.extend([[x0, y0 + 2, zc + 1], [x0 + s, y0 + 1, zc + 1], [x0 + s, y0 + 1, zc + n.min(2)], [x0 - s, y0 + 1, zc + n]]);
            v.extend([[x0, y0, zc + n + 1], [x0 - s, y0, zc + n + 1], [x0, y0 + 1, zc + n + 1], [x0 + s, y0, zc + n + 1]]);
            v
        };
        let electric = opt.electric;
        let motor_def = ["motor_small", "motor_medium", "motor_large"][opt.motor.min(2)];
        let motor_r = orient(4, 2); // local +Y forward, so the RPS port (local −Y) faces aft
        let motor_reach = [0, 1, 2][opt.motor.min(2)];
        let zs: Vec<i32> = {
            let mut z: Vec<i32> = room_cells.iter().filter(|c| c.0 == x0).map(|c| c.1).collect();
            z.sort();
            z.dedup();
            z
        };
        let pick = zs.iter().copied().find(|&zc| {
            if electric {
                let vp = [x0, y0 + motor_reach, zc + motor_reach];
                footprint(motor_def).iter().all(|l| {
                    let v = add(vp, rot(&motor_r, *l));
                    sp.free(v) && in_room(v)
                }) && sp.free([x0, y0 + motor_reach, zc - 1]) && in_room([x0, y0, zc - 1])
            } else {
                engine_cells(zc).iter().all(|v| sp.free(*v) && in_room(*v))
            }
        });
        let Some(zc) = pick else {
            rep.warnings.push(format!("Line {}: the engine room has no space for the {}", line + 1, if electric { "motor" } else { "engine" }));
            continue;
        };
        let mut shaft_from: Port;
        if electric {
            let vp = [x0, y0 + motor_reach, zc + motor_reach];
            sp.place(&mut b, motor_def, vp, motor_r, colour);
            let rps = port(vp, &motor_r, [0, -motor_reach, 0], 3);
            let elec = port_voxel(vp, &motor_r, [0, motor_reach, 0]);
            let thr = port_voxel(vp, &motor_r, [0, 0, 0]);
            elec_users.push(elec);
            controls.push(("motor throttle".into(), thr, NUM));
            // gearbox right behind the motor
            let gv = add(rps.voxel, rps.facing);
            sp.place(&mut b, "modular_engine_gearbox_1x1", gv, crank_r, colour);
            b.extras.push(PartExtra { def: "modular_engine_gearbox_1x1".into(), vp: gv, attrs: vec![("gear_ratio_1".into(), opt.gear.min(7).to_string())], mc: None });
            elec_users.push(gv);
            shaft_from = port(gv, &crank_r, [0, 0, 0], 2);
            rep.notes.push(format!("Line {}: {} with a gearbox", line + 1, motor_def.replace('_', " ")));
        } else {
            // crankshafts, cylinders
            for k in 1..=n {
                sp.place(&mut b, "modular_engine_crankshaft", [x0, y0, zc + k], crank_r, colour);
                sp.place(&mut b, "modular_engine_cylinder_straight", [x0, y0 + 1, zc + k], ID, colour);
            }
            sp.place(&mut b, "modular_engine_clutch", [x0, y0, zc], crank_r, colour);
            sp.place(&mut b, "modular_engine_gearbox_1x1", [x0, y0, zc - 1], crank_r, colour);
            b.extras.push(PartExtra { def: "modular_engine_gearbox_1x1".into(), vp: [x0, y0, zc - 1], attrs: vec![("gear_ratio_1".into(), opt.gear.min(7).to_string())], mc: None });
            elec_users.push([x0, y0, zc - 1]);
            shaft_from = port([x0, y0, zc - 1], &crank_r, [0, 0, 0], 2);
            // manifolds: air and fuel on the outboard side with ports up; exhaust on top; coolant inboard
            let side_r = orient(dir_index([-s, 0, 0]), 2);
            let air_v = [x0 + s, y0 + 1, zc + 1];
            let fuel_v = [x0 + s, y0 + 1, zc + n.min(2)];
            let fuel_v = if fuel_v == air_v { [x0 - s, y0 + 1, zc + 1] } else { fuel_v };
            sp.place(&mut b, "modular_engine_air_manifold", air_v, side_r, colour);
            let fuel_r = orient(dir_index([fuel_v[0].cmp(&x0) as i32 * -1, 0, 0]), 2);
            sp.place(&mut b, "modular_engine_intake_manifold", fuel_v, fuel_r, colour);
            let ex_v = [x0, y0 + 2, zc + 1];
            sp.place(&mut b, "modular_engine_exhaust_manifold_straight", ex_v, ID, colour);
            let cool_v = [x0 - s, y0 + 1, zc + n];
            let cool_r = orient(dir_index([s, 0, 0]), 5);
            sp.place(&mut b, "modular_engine_coolant_manifold", cool_v, cool_r, colour);
            // belt end
            let belt_v = [x0, y0, zc + n + 1];
            sp.place(&mut b, "modular_engine_drive_belt", belt_v, orient(4, 3), colour);
            let starter_v = [x0 - s, y0, zc + n + 1];
            sp.place(&mut b, "modular_engine_starter", starter_v, orient(dir_index([-s, 0, 0]), 4), colour);
            elec_users.push(starter_v);
            let alt_v = [x0, y0 + 1, zc + n + 1];
            sp.place(&mut b, "modular_engine_alternator", alt_v, ID, colour);
            battery_ports.push(Port { voxel: alt_v, facing: [0, 0, 0] });
            let pump_v = [x0 + s, y0, zc + n + 1];
            let pump_r = orient(dir_index([s, 0, 0]), dir_index([0, 0, -s]));
            sp.place(&mut b, "modular_engine_fluid_pump", pump_v, pump_r, colour);
            // logic of the engine (wired to the controller below)
            controls.push(("air throttle".into(), air_v, NUM));
            controls.push(("fuel throttle".into(), fuel_v, NUM));
            controls.push(("clutch".into(), [x0, y0, zc], NUM));
            controls.push(("belt pump".into(), pump_v, NUM));
            controls.push(("alternator".into(), alt_v, NUM));
            controls.push(("starter".into(), starter_v, BOOL));
            controls.push(("rps".into(), [x0, y0, zc + 1], NUM));
            rep.notes.push(format!("Line {}: {}-cylinder modular engine at z {}..{}", line + 1, n, zc - 1, zc + n + 1));

            // ---- cooling loop: coolant B → belt pump → radiator → header tank → coolant A. The most constrained
            // piping, so it goes first; radiator and tank positions are tried together until all four pipes route.
            let cool_a = port(cool_v, &cool_r, [0, 0, 0], 4);
            let cool_b = port(cool_v, &cool_r, [0, 0, 0], 5);
            let pump_in = port(pump_v, &pump_r, [0, 0, 0], 5);
            let pump_out = port(pump_v, &pump_r, [0, 0, 0], 0);
            let rads: Vec<[i32; 9]> = [4usize, 5, 0, 1].iter().map(|&f| orient(2, f)).collect();
            let mut rad_sites: Vec<([i32; 3], [i32; 9])> = room_cells
                .iter()
                .flat_map(|&(x, z)| rads.iter().map(move |r| ([x, y0 + 1, z], *r)))
                .filter(|(v, r)| sp.fits("fluid_radiator", *v, r) && footprint("fluid_radiator").iter().all(|l| in_room(add(*v, rot(r, *l)))))
                .filter(|(v, r)| [[1, 1, 0], [-1, 1, 0]].iter().all(|p| sp.free(add(port_voxel(*v, r, *p), rot(r, [0, 0, 1])))))
                .collect();
            rad_sites.sort_by_key(|(v, _)| (v[0] - cool_v[0]).abs() + (v[2] - cool_v[2]).abs());
            // the engine-to-pump leg does not depend on the radiator: lay it first, keeping clear the cells where the
            // pump outlet and coolant inlet pipes must start
            let keep = [add(pump_out.voxel, pump_out.facing), add(cool_a.voxel, cool_a.facing)];
            let held: Vec<[i32; 3]> = keep.iter().copied().filter(|v| sp.free(*v)).collect();
            for v in &held {
                sp.take(*v);
            }
            let mut done = connect(&mut sp, &mut b, &mut rep, "coolant (engine to pump)", cool_b, pump_in, &HashSet::new(), colour);
            for v in &held {
                sp.cells.remove(v);
            }
            let first_ok = done;
            done = false;
            'cool: for (rv, rr) in rad_sites.into_iter().take(if first_ok { 16 } else { 0 }) {
                let mut tank_sites: Vec<[i32; 3]> = room_cells
                    .iter()
                    // raised one block so both its ports (top and bottom) have room for a pipe
                    .map(|&(x, z)| [x, y0 + 1, z])
                    .filter(|v| in_room(*v) && in_room(add(*v, [0, 0, -1])))
                    .collect();
                tank_sites.sort_by_key(|v| (v[0] - cool_v[0]).abs() + (v[2] - cool_v[2]).abs());
                'cool_tank: for tv in tank_sites.into_iter().take(16) {
                    let (mut sp2, mut b2, mut rep2) = (sp.clone(), b.clone(), Report::default());
                    if !sp2.fits("fluid_radiator", rv, &rr) {
                        continue;
                    }
                    sp2.place(&mut b2, "fluid_radiator", rv, rr, colour);
                    if !(sp2.fits("fluid_tank_small", tv, &ID) && sp2.free(add(tv, [0, 1, 0])) && sp2.free(add(tv, [0, -1, 0]))) {
                        continue;
                    }
                    sp2.place(&mut b2, "fluid_tank_small", tv, ID, colour);
                    let rad_a = port(rv, &rr, [1, 1, 0], 4);
                    let rad_b = port(rv, &rr, [-1, 1, 0], 4);
                    let t_top = port(tv, &ID, [0, 0, 0], 2);
                    let t_bot = port(tv, &ID, [0, 0, 0], 3);
                    let legs = [("coolant (pump to radiator)", pump_out, rad_a), ("coolant (radiator to header tank)", rad_b, t_top), ("coolant (header tank to engine)", t_bot, cool_a)];
                    for (what, from, to) in legs {
                        match route(&sp2, from, to, &HashSet::new(), 60_000) {
                            Some(r) => {
                                rep2.notes.push(format!("{what}: {} pipe pieces", r.cells.len()));
                                lay(&mut sp2, &mut b2, &r, colour);
                            }
                            None => {
                                if std::env::var_os("KEEL_DEBUG_PROP").is_some() {
                                    eprintln!("    cooling trial rad {rv:?} tank {tv:?}: {what} failed ({:?} -> {:?})", add(from.voxel, from.facing), add(to.voxel, to.facing));
                                }
                                continue 'cool_tank;
                            }
                        }
                    }
                    sp = sp2;
                    b = b2;
                    rep.notes.extend(rep2.notes);
                    rep.notes.push("Cooling: belt pump, radiator and a water header tank".into());
                    done = true;
                    break 'cool;
                }
            }
            if !done {
                rep.warnings.push("Could not fit the cooling loop (radiator and header tank): the engine has no cooling yet".into());
            }

            // ---- fuel: stacked medium tanks, electric pump, pipes to the fuel manifold
            let fuel_port = port(fuel_v, &fuel_r, [0, 0, 0], 4);
            let tanks = opt.tanks.clamp(1, 3) as i32;
            let tank_site = room_cells
                .iter()
                .map(|&(x, z)| [x, y0 + 1, z])
                .filter(|v| (0..tanks).all(|k| sp.fits("fluid_tank_medium", add(*v, [0, 2 * k, 0]), &ID)) && footprint("fluid_tank_medium").iter().all(|l| in_room(add(*v, *l))))
                .min_by_key(|v| (v[0] - (x0 + s * 2)).abs() + (v[2] - (zc + n + 3)).abs());
            match tank_site {
                Some(tv) => {
                    for k in 0..tanks {
                        let v = add(tv, [0, 2 * k, 0]);
                        sp.place(&mut b, "fluid_tank_medium", v, ID, colour);
                        b.extras.push(PartExtra { def: "fluid_tank_medium".into(), vp: v, attrs: vec![("fluid_type".into(), "1".into())], mc: None });
                    }
                    let tank_top = port(add(tv, [0, 2 * (tanks - 1), 0]), &ID, [0, 0, 0], 2);
                    // electric fuel pump on the floor near the tank
                    let pump = room_cells
                        .iter()
                        .map(|&(x, z)| [x, y0, z])
                        .filter(|v| sp.fits("water_pump", *v, &ID) && in_room(add(*v, [-1, 0, 0])) && in_room(add(*v, [1, 0, 0])) && sp.free(add(*v, [-2, 0, 0])) && sp.free(add(*v, [2, 0, 0])))
                        .min_by_key(|v| (v[0] - tv[0]).abs() + (v[2] - tv[2]).abs() + 2 * (v[2] - fuel_v[2]).abs());
                    match pump {
                        Some(pv) => {
                            sp.place(&mut b, "water_pump", pv, ID, colour);
                            elec_users.push(pv);
                            controls.push(("fuel pump".into(), pv, BOOL));
                            let p_in = port(pv, &ID, [-1, 0, 0], 1);
                            let p_out = port(pv, &ID, [1, 0, 0], 0);
                            connect(&mut sp, &mut b, &mut rep, "fuel line (tank to pump)", tank_top, p_in, &HashSet::new(), colour);
                            connect(&mut sp, &mut b, &mut rep, "fuel line (pump to engine)", p_out, fuel_port, &HashSet::new(), colour);
                        }
                        None => {
                            rep.warnings.push("No room for the fuel pump: tank piped straight to the engine".into());
                            connect(&mut sp, &mut b, &mut rep, "fuel line", tank_top, fuel_port, &HashSet::new(), colour);
                        }
                    }
                    rep.notes.push(format!("Fuel: {tanks} medium diesel tank(s)"));
                }
                None => rep.warnings.push("No room for a fuel tank in the engine room".into()),
            }

            // ---- air intake and exhaust on the weather deck
            let air_port = port(air_v, &side_r, [0, 0, 0], 4);
            let ex_port = port(ex_v, &ID, [0, 0, 0], 2);
            for (what, def, from) in [("air intake", "fluid_intake", air_port), ("exhaust", "fluid_exhaust", ex_port)] {
                // a free spot on deck near the engine: the part sits on the deck, its port faces down through it
                let mut sites: Vec<[i32; 3]> = top
                    .iter()
                    .map(|(&(x, z), &t)| [x, t + 1, z])
                    .filter(|v| (v[0] - x0).abs() <= 8 && v[2] >= zc - 8 && v[2] <= zc + n + 8)
                    .filter(|v| sp.fits(def, *v, &ID) && footprint(def).iter().all(|l| top.get(&(v[0] + l[0], v[2] + l[2])) == Some(&(v[1] - 1))))
                    .collect();
                sites.sort_by_key(|v| (v[0] - from.voxel[0]).abs() * 2 + (v[2] - from.voxel[2]).abs() + if what == "exhaust" { (v[2] - zc).abs() } else { 0 });
                if sites.is_empty() {
                    rep.warnings.push(format!("No free deck space for the {what}"));
                }
                let mut ok = false;
                for vp in sites.into_iter().take(8) {
                    let to = Port { voxel: vp, facing: [0, -1, 0] };
                    if let Some(r) = route(&sp, from, to, &HashSet::new(), 150_000) {
                        rep.notes.push(format!("{what}: {} pipe pieces", r.cells.len()));
                        lay(&mut sp, &mut b, &r, colour);
                        sp.place(&mut b, def, vp, ID, colour);
                        ok = true;
                        break;
                    }
                }
                if !ok {
                    rep.warnings.push(format!("Could not route the {what}; connect it by hand"));
                }
            }

        }

        // ---- propeller, shaft and rudder
        let prop_def = ["propeller", "large_propeller", "giga_prop_small"][opt.prop.min(2)];
        let clear_r = if opt.prop == 0 { 1 } else { 3 };
        let prop_r = crank_r; // local +Y aft: the RPS port (local −Y) faces forward
        let mut placed = None;
        for dz in 3..12 {
            let z = z_stern + dz;
            let low = (-clear_r..=clear_r).filter_map(|dx| bottom.get(&(x0 + dx, z))).min().copied();
            let Some(low) = low else { continue };
            let vp = [x0, low - 1 - clear_r, z];
            let ok = footprint(prop_def).iter().all(|l| {
                let v = add(vp, rot(&prop_r, *l));
                !sp.cells.contains_key(&v) && !inside(v)
            });
            if ok {
                placed = Some(vp);
                break;
            }
        }
        match placed {
            Some(vp) => {
                for l in footprint(prop_def) {
                    sp.take(add(vp, rot(&prop_r, l)));
                }
                b.part(prop_def, vp, prop_r, Some(colour));
                let pos = if opt.prop == 0 { [0, -1, 0] } else { [0, 0, 0] };
                let prop_port = port(vp, &prop_r, pos, 3);
                // water cells the shaft may use: the column above the propeller up to the hull, and a little ahead
                let mut extra = HashSet::new();
                let bz = prop_port.voxel[2];
                for z in bz..bz + 8 {
                    for y in prop_port.voxel[1]..prop_port.voxel[1] + 8 {
                        let v = [x0, y, z];
                        if !inside(v) && !sp.cells.contains_key(&v) {
                            extra.insert(v);
                        }
                    }
                }
                if connect(&mut sp, &mut b, &mut rep, "propeller shaft", shaft_from, prop_port, &extra, colour) {
                    shaft_from = prop_port;
                }
                let _ = shaft_from;
                rep.notes.push(format!("Line {}: {} under the stern", line + 1, prop_def.replace('_', " ")));
                if opt.rudder {
                    // fin rudder behind the propeller: blade vertical, input on top, its top a block below the hull
                    let rr = orient(5, 2);
                    let rz = vp[2] - 2 - clear_r;
                    let hb = (rz - 2..=rz + 1).filter_map(|z| bottom.get(&(x0, z))).min().copied().unwrap_or(vp[1] + 4);
                    let rv = [x0, (hb - 3).min(vp[1] + 1), rz];
                    if footprint("rudder_surface").iter().all(|l| {
                        let v = add(rv, rot(&rr, *l));
                        !sp.cells.contains_key(&v) && !inside(v)
                    }) {
                        for l in footprint("rudder_surface") {
                            sp.take(add(rv, rot(&rr, l)));
                        }
                        b.part("rudder_surface", rv, rr, Some(colour));
                        elec_users.push(rv);
                        controls.push(("rudder".into(), port_voxel(rv, &rr, [0, 0, 1]), NUM));
                        rep.notes.push(format!("Line {}: fin rudder", line + 1));
                    } else {
                        rep.warnings.push(format!("Line {}: no room for a rudder behind the propeller", line + 1));
                    }
                }
            }
            None => rep.warnings.push(format!("Line {}: no room under the stern for the {}", line + 1, prop_def)),
        }
    }

    // ---- battery and electrics
    let bat_site = room_cells
        .iter()
        .map(|&(x, z)| [x, y0, z])
        .filter(|v| sp.fits("battery_small", *v, &ID) && in_room(add(*v, [-1, 0, 0])))
        .min_by_key(|v| v[0].abs() + v[2]);
    let battery = bat_site.map(|v| {
        sp.place(&mut b, "battery_small", v, ID, colour);
        v
    });
    if battery.is_none() {
        rep.warnings.push("No room for a battery in the engine room".into());
    }

    // ---- controls: helm, throttle lever and key on the bridge (or on deck amidships)
    let bridge = plan.rooms.iter().find(|r| r.kind == RoomKind::Bridge);
    let helm_r = ID; // faces +z
    let helm_site = match bridge {
        Some(r) => {
            let fy = plan.decks[r.deck].floor_y + 4;
            let rc: HashSet<(i32, i32)> = r.cells.iter().copied().collect();
            let mut c: Vec<[i32; 3]> = r.cells.iter().map(|&(x, z)| [x, fy, z]).collect();
            c.sort_by_key(|v| (v[0].abs(), -v[2]));
            c.into_iter().find(|v| sp.fits("seat_helm", *v, &helm_r) && footprint("seat_helm").iter().all(|l| rc.contains(&(v[0] + l[0], v[2] + l[2]))))
        }
        None => {
            let mut c: Vec<[i32; 3]> = top.iter().map(|(&(x, z), &t)| [x, t + 4, z]).collect();
            c.sort_by_key(|v| (v[0].abs(), v[2].abs()));
            c.into_iter().find(|v| sp.fits("seat_helm", *v, &helm_r) && footprint("seat_helm").iter().all(|l| top.get(&(v[0] + l[0], v[2] + l[2])) == Some(&(v[1] - 4))))
        }
    };
    let Some(hv) = helm_site else {
        rep.warnings.push("No room for a helm: steering and throttle are not wired".into());
        build.extend(b);
        build.parts.retain(|p| p.def != "__removed__");
        return rep;
    };
    sp.place(&mut b, "seat_helm", hv, helm_r, colour);
    let floor = hv[1] - 3;
    // throttle lever and key beside the helm (on the floor, starboard side)
    let lever = (2..6).map(|dx| [hv[0] + dx, floor, hv[2]]).find(|v| sp.fits("button_throttle_lever", *v, &ID));
    let key = (2..8).map(|dx| [hv[0] - dx, floor, hv[2]]).find(|v| sp.fits("button_key", *v, &ID));
    if let Some(v) = lever {
        sp.place(&mut b, "button_throttle_lever", v, ID, colour);
        elec_users.push(v);
    }
    if let Some(v) = key {
        sp.place(&mut b, "button_key", v, ID, colour);
        elec_users.push(v);
    }
    let helm_axis_ad = port_voxel(hv, &helm_r, [-1, 0, 0]);
    for (what, v, kind) in &controls {
        if what == "rudder" && *kind == NUM {
            b.link(NUM, helm_axis_ad, *v);
        }
    }
    rep.notes.push("Controls: helm (A/D steers), throttle lever, key".into());

    // ---- the engine controller (combustion engines) or direct throttle (electric)
    let lever_out = lever.map(|v| port_voxel(v, &ID, [0, 0, 0]));
    let key_out = key.map(|v| port_voxel(v, &ID, [0, 0, 0]));
    if opt.electric {
        if let Some(t) = lever_out {
            for (what, v, _) in &controls {
                if what == "motor throttle" {
                    b.link(NUM, t, *v);
                }
            }
        }
    } else {
        let mc_site = room_cells
            .iter()
            .map(|&(x, z)| [x, y0, z])
            .find(|v| (0..4).all(|dx| (0..2).all(|dz| sp.free(add(*v, [dx, 0, dz])) && in_room(add(*v, [dx, 0, dz])))));
        match mc_site {
            Some(mv) => {
                for dx in 0..4 {
                    for dz in 0..2 {
                        sp.take(add(mv, [dx, 0, dz]));
                    }
                }
                b.part("microprocessor", mv, ID, Some("454545"));
                b.extras.push(PartExtra { def: "microprocessor".into(), vp: mv, attrs: vec![], mc: Some(engine_controller()) });
                let pin = |x: i32, z: i32| add(mv, [x, 0, z]);
                if let Some(t) = lever_out {
                    b.link(NUM, t, pin(0, 0));
                }
                if let Some(k) = key_out {
                    b.link(BOOL, k, pin(1, 0));
                }
                let mut rps_wired = false;
                for (what, v, _) in &controls {
                    match what.as_str() {
                        // one tachometer is enough (an input takes one wire)
                        "rps" if !rps_wired => {
                            b.link(NUM, *v, pin(2, 0));
                            rps_wired = true;
                        }
                        "belt pump" | "alternator" => b.link(NUM, pin(3, 1), *v),
                        "air throttle" | "fuel throttle" => b.link(NUM, pin(0, 1), *v),
                        "clutch" => b.link(NUM, pin(1, 1), *v),
                        "starter" => b.link(BOOL, pin(2, 1), *v),
                        "fuel pump" => {
                            if let Some(k) = key_out {
                                b.link(BOOL, k, *v);
                            }
                        }
                        _ => {}
                    }
                }
                rep.notes.push("Engine controller microcontroller: idle throttle, clutch, starter".into());
            }
            None => rep.warnings.push("No room for the engine controller: wire the throttle by hand".into()),
        }
    }
    if let Some(bv) = battery {
        let store = port_voxel(bv, &ID, [0, 0, 0]);
        for v in &elec_users {
            b.link(ELEC, store, *v);
        }
        for p in &battery_ports {
            b.link(ELEC, p.voxel, store);
        }
        rep.notes.push(format!("Battery feeding {} electric parts", elec_users.len()));
    }
    let _ = battery_ports;
    build.extend(b);
    build.parts.retain(|p| p.def != "__removed__");
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_are_24_and_proper() {
        let r = rotations();
        assert_eq!(r.len(), 24);
        let mut set: HashSet<[i32; 9]> = HashSet::new();
        for m in &r {
            assert!(set.insert(*m));
            let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[3] * (m[1] * m[8] - m[2] * m[7]) + m[6] * (m[1] * m[5] - m[2] * m[4]);
            assert_eq!(det, 1);
        }
        // crank orientation as seen in working engines: local Y → −Z, local Z → +Y
        assert_eq!(orient(5, 2), [1, 0, 0, 0, 0, -1, 0, 1, 0]);
    }

    #[test]
    fn router_connects_ports_with_matching_ends() {
        let inside = |v: [i32; 3]| v.iter().all(|c| c.abs() < 10);
        let mut sp = Space { cells: HashMap::new(), inside: &inside };
        sp.take([0, 0, 0]);
        let a = Port { voxel: [-3, 0, 0], facing: [1, 0, 0] };
        let z = Port { voxel: [3, 2, 0], facing: [0, -1, 0] };
        let r = route(&sp, a, z, &HashSet::new(), 100_000).expect("route");
        let first = r.cells.first().unwrap();
        let last = r.cells.last().unwrap();
        assert_eq!(first.0, [-2, 0, 0]);
        assert!(first.1.contains(&1), "first piece opens back towards the start port");
        assert_eq!(last.0, [3, 1, 0]);
        assert!(last.1.contains(&2), "last piece opens up into the end port");
        // consecutive pieces face each other
        for w in r.cells.windows(2) {
            let d = [w[1].0[0] - w[0].0[0], w[1].0[1] - w[0].0[1], w[1].0[2] - w[0].0[2]];
            assert!(w[0].1.contains(&dir_index(d)) && w[1].1.contains(&dir_index(neg(d))));
        }
        assert!(r.cells.iter().all(|c| c.0 != [0, 0, 0]));
        let mut b = Build::default();
        lay(&mut sp, &mut b, &r, "FFFFFF");
        assert_eq!(b.parts.len(), r.cells.len());
    }

    #[test]
    fn engine_controller_round_trips() {
        let xml = engine_controller();
        let doc = keel_io::xml::parse(&xml).unwrap();
        let g = keel_mc::graph::Graph::parse(&doc.root);
        assert_eq!(g.nodes.len(), 7);
        // simulate: key on, throttle 0.5, engine stopped → starter on, engine throttle 0.5
        let mut s = keel_mc::sim::Sim::default();
        let id = |l: &str| g.nodes.iter().find(|n| n.label == l).unwrap().id;
        s.external.insert(id("Throttle"), keel_mc::sim::Value::num(0.5));
        s.external.insert(id("Key"), keel_mc::sim::Value::bool(true));
        for _ in 0..4 {
            s.step(&g);
        }
        assert!(s.output(&g, id("Starter")).b);
        assert!((s.output(&g, id("Engine throttle")).n - 0.5).abs() < 1e-9);
        assert!(s.output(&g, id("Clutch")).n > 0.9);
        assert_eq!(s.output(&g, id("Accessories")).n, 1.0);
        s.external.insert(id("RPS"), keel_mc::sim::Value::num(10.0));
        s.external.insert(id("Throttle"), keel_mc::sim::Value::num(0.0));
        for _ in 0..4 {
            s.step(&g);
        }
        assert!(!s.output(&g, id("Starter")).b);
        assert!((s.output(&g, id("Engine throttle")).n - 0.12).abs() < 1e-9, "idles at 12 %");
        assert_eq!(s.output(&g, id("Clutch")).n, 0.0);
    }
}

#[cfg(test)]
mod install_tests {
    use super::*;
    use crate::{deck_row, generate, programme, superstructure, LayoutOptions};
    use keel_geom::hull::HullParams;
    use keel_geom::voxel::VOXEL;

    fn workboat(p: PropulsionOptions) -> crate::Layout {
        let h = HullParams::default().build().unwrap();
        let hb = keel_geom::hullblocks::build(&h, &Default::default());
        let half = (h.params.beam / 2.0 / VOXEL) as i32;
        let zl = (h.params.length / 2.0 / VOXEL) as i32;
        let s = superstructure::Superstructure::default_for(deck_row(&hb), half, -zl, zl, 2);
        let opt = LayoutOptions {
            programme: programme::ProgrammeInput { boat: programme::BoatType::Workboat, length: h.params.length, beam: h.params.beam, crew: 0, passengers: 0 },
            hull_floors: None,
            rooms: None,
            superstructure: Some(s),
            plan: Default::default(),
            finish: Default::default(),
            lighting: Default::default(),
            propulsion: Some(p),
        };
        generate(&hb, &opt)
    }

    /// With the game's definitions: wires land on ports, and the physical ports the installer uses face a pipe end
    /// or a matching port.
    #[test]
    fn installed_drive_connects() {
        for p in [PropulsionOptions::default(), PropulsionOptions { electric: true, ..Default::default() }, PropulsionOptions { lines: 2, cylinders: 3, ..Default::default() }] {
            let l = workboat(p.clone());
            let rep = l.propulsion.clone().unwrap();
            eprintln!("--- electric {} lines {}", p.electric, p.lines);
            for n in &rep.notes {
                eprintln!("  note: {n}");
            }
            for w in &rep.warnings {
                eprintln!("  WARN: {w}");
            }
            let Some(dir) = std::env::var_os("KEEL_DEFINITIONS") else { continue };
            let cat = keel_model::catalogue::Catalogue::load_dir(std::path::Path::new(&dir)).unwrap();
            let b = &l.build;
            // ports by voxel
            let mut ports: HashMap<([i32; 3], i32), Vec<(String, [i32; 3], i32)>> = HashMap::new();
            for part in &b.parts {
                let Some(d) = cat.get(&part.def) else { panic!("unknown definition {}", part.def) };
                for n in &d.logic_nodes {
                    let v = port_voxel(part.vp, &part.r, n.position);
                    ports.entry((v, n.kind)).or_default().push((part.def.clone(), rot(&part.r, dir_vec(n.orientation as usize)), n.mode));
                }
            }
            for part in b.parts.iter().filter(|p| p.def == "microprocessor") {
                for x in 0..4 {
                    for z in 0..2 {
                        for k in [0, 1] {
                            ports.entry((add(part.vp, [x, 0, z]), k)).or_default().push(("microprocessor".into(), [0, 0, 0], 2));
                        }
                    }
                }
            }
            let bad: Vec<_> = b.links.iter().filter(|w| !ports.contains_key(&(w.a, w.kind)) || !ports.contains_key(&(w.b, w.kind))).collect();
            assert!(bad.is_empty(), "{} wires miss a port, e.g. {:?}", bad.len(), &bad[..bad.len().min(5)]);
            // pipe open ends by voxel
            let mut ends: HashMap<[i32; 3], Vec<[i32; 3]>> = HashMap::new();
            for part in &b.parts {
                let local: Vec<[i32; 3]> = match part.def.as_str() {
                    "trans_straight" | "trans_block_straight" => vec![[1, 0, 0], [-1, 0, 0]],
                    "trans_angle" | "trans_block_angle" => vec![[-1, 0, 0], [0, 1, 0]],
                    _ => continue,
                };
                ends.insert(part.vp, local.iter().map(|e| rot(&part.r, *e)).collect());
            }
            let mut unconnected = vec![];
            for ((v, kind), list) in &ports {
                if !matches!(kind, 2 | 3) {
                    continue;
                }
                for (def, facing, _) in list {
                    let front = add(*v, *facing);
                    let pipe = ends.get(&front).is_some_and(|e| e.contains(&neg(*facing)));
                    let port_back = ports.get(&(front, *kind)).is_some_and(|l| l.iter().any(|q| q.1 == neg(*facing)));
                    if !pipe && !port_back {
                        unconnected.push((def.clone(), *v, *facing));
                    }
                }
            }
            // every pipe piece's ends face a pipe or a port
            let mut loose = 0;
            for (v, e) in &ends {
                for d in e {
                    let f = add(*v, *d);
                    let ok = ends.get(&f).is_some_and(|x| x.contains(&neg(*d))) || [2, 3].iter().any(|k| ports.get(&(f, *k)).is_some_and(|l| l.iter().any(|q| q.1 == neg(*d))));
                    if !ok {
                        loose += 1;
                        eprintln!("  loose pipe end at {v:?} towards {d:?}");
                    }
                }
            }
            unconnected.sort_by(|a, b| a.0.cmp(&b.0));
            for u in &unconnected {
                eprintln!("  unconnected physical port: {u:?}");
            }
            assert_eq!(loose, 0, "pipe ends must meet pipes or ports");
            let must = ["modular_engine_clutch", "modular_engine_intake_manifold", "modular_engine_air_manifold", "modular_engine_exhaust_manifold_straight", "propeller", "motor_medium"];
            let missing: Vec<_> = unconnected.iter().filter(|u| must.contains(&u.0.as_str())).collect();
            assert!(missing.is_empty() || !rep.warnings.is_empty(), "unconnected without a warning: {missing:?}");
        }
    }
}
