//! Lighting and power.
//!
//! Every lit room (and each deck's corridor) is one circuit:
//!
//! ```text
//! room push button ──A┐
//!                     OR ── push-to-toggle ── lights (Light Switch)
//! master button ─────B┘
//! battery ── circuit breaker ── lights, buttons (Electric)
//! ```
//!
//! The gates sit in a panel on the electrical room floor (or the engine room). The master button is on the bridge.
//! Light colour = colour temperature (ship-wide kelvin, overridable per room and per light) × the room's brightness,
//! painted as the light's additive colour.
use crate::build::{orient, Build, ID};
use crate::light::light_colour;
use crate::plan::{Cell, Plan};
use crate::programme::RoomKind;
use keel_model::edit::port_voxel;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone)]
pub struct LightingOptions {
    /// ship-wide colour temperature
    pub kelvin: f64,
    /// per-room overrides (room id → kelvin, brightness)
    pub room: BTreeMap<usize, (Option<f64>, Option<f64>)>,
    /// per-light kelvin overrides (light voxel → kelvin)
    pub light: BTreeMap<[i32; 3], f64>,
    /// light spacing in voxels
    pub spacing: i32,
}

impl Default for LightingOptions {
    fn default() -> Self {
        LightingOptions { kelvin: 3000.0, room: BTreeMap::new(), light: BTreeMap::new(), spacing: 6 }
    }
}

/// A lighting circuit, for the UI.
#[derive(Debug, Clone)]
pub struct Circuit {
    pub name: String,
    pub room: Option<usize>,
    pub button: [i32; 3],
    pub lights: Vec<[i32; 3]>,
    pub kelvin: f64,
    pub brightness: f64,
}

const PORT_A: [i32; 3] = [1, 0, 1];
const PORT_B: [i32; 3] = [0, 0, 1];
const OR_OUT: [i32; 3] = [0, 0, 0];
const TOGGLE_IN: [i32; 3] = [0, 0, 1];
const TOGGLE_OUT: [i32; 3] = [0, 0, 0];

fn ceiling_lights(cells: &[(i32, i32)], y: i32, spacing: i32, used: &HashSet<[i32; 3]>) -> Vec<[i32; 3]> {
    if cells.is_empty() {
        return vec![];
    }
    let x0 = cells.iter().map(|c| c.0).min().unwrap();
    let z0 = cells.iter().map(|c| c.1).min().unwrap();
    let x1 = cells.iter().map(|c| c.0).max().unwrap();
    let z1 = cells.iter().map(|c| c.1).max().unwrap();
    let set: HashSet<(i32, i32)> = cells.iter().copied().collect();
    let mut out = vec![];
    // a grid centred in the room's bounding box
    let nx = ((x1 - x0) / spacing).max(0) + 1;
    let nz = ((z1 - z0) / spacing).max(0) + 1;
    for i in 0..nx {
        for j in 0..nz {
            let x = x0 + (x1 - x0) * (2 * i + 1) / (2 * nx);
            let z = z0 + (z1 - z0) * (2 * j + 1) / (2 * nz);
            if set.contains(&(x, z)) && !used.contains(&[x, y, z]) {
                out.push([x, y, z]);
            }
        }
    }
    if out.is_empty() {
        let c = cells[cells.len() / 2];
        out.push([c.0, y, c.1]);
    }
    out
}

/// Place lights, switches, gates and the battery bus. `used` = voxels taken by furniture.
pub fn wire(plan: &Plan, used: &HashSet<[i32; 3]>, opt: &LightingOptions) -> (Build, Vec<Circuit>) {
    let mut b = Build::default();
    let mut circuits: Vec<Circuit> = vec![];
    // circuits: rooms with switches, then corridors per deck
    for room in &plan.rooms {
        if !room.kind.has_switch() || room.switch_at.is_none() {
            continue;
        }
        let d = &plan.decks[room.deck];
        let (k_def, br_def) = room.kind.light();
        let (k_ov, br_ov) = opt.room.get(&room.id).copied().unwrap_or((None, None));
        let kelvin = k_ov.or(k_def).unwrap_or(opt.kelvin);
        let brightness = br_ov.unwrap_or(br_def);
        let cells: Vec<(i32, i32)> = room.cells.iter().copied().collect();
        let lights = ceiling_lights(&cells, d.floor_y + d.clear, opt.spacing, used);
        let ((sx, sz), wall) = room.switch_at.unwrap();
        circuits.push(Circuit { name: format!("{} #{}", room.kind.name(), room.id), room: Some(room.id), button: [sx, d.floor_y + 4, sz], lights, kelvin, brightness });
        let _ = wall;
    }
    for (di, d) in plan.decks.iter().enumerate() {
        let cells: Vec<(i32, i32)> = plan.grids[di].iter().filter(|(_, c)| **c == Cell::Corridor).map(|(k, _)| *k).collect();
        if cells.len() < 6 {
            continue;
        }
        // the corridor switch: on the corridor cell nearest the access core (or the corridor's aft end)
        let target = plan.core.map(|(x0, _, z0, _)| (x0, z0 - 1)).unwrap_or(cells[0]);
        let sw = *cells.iter().min_by_key(|c| (c.0 - target.0).abs() + (c.1 - target.1).abs()).unwrap();
        let lights = ceiling_lights(&cells, d.floor_y + d.clear, opt.spacing, used);
        circuits.push(Circuit { name: format!("{} corridor", d.name()), room: None, button: [sw.0, d.floor_y + 4, sw.1], lights, kelvin: opt.kelvin, brightness: 0.5 });
    }
    if circuits.is_empty() {
        return (b, circuits);
    }
    // panel: the electrical room, then the engine room, then any room. Each circuit takes a free 2 × 4 floor patch
    // (OR gate 2 × 2, push-to-toggle 1 × 2 behind it).
    let mut order: Vec<&crate::plan::Room> = plan.rooms.iter().filter(|r| r.kind == RoomKind::Electrical).collect();
    order.extend(plan.rooms.iter().filter(|r| r.kind == RoomKind::EngineRoom));
    order.extend(plan.rooms.iter().filter(|r| !matches!(r.kind, RoomKind::Electrical | RoomKind::EngineRoom)));
    let Some(pr) = order.first().copied() else { return (b, circuits) };
    let mut taken: HashSet<[i32; 3]> = used.clone();
    let mut slot = |taken: &mut HashSet<[i32; 3]>| -> Option<[i32; 3]> {
        for r in &order {
            let y = plan.decks[r.deck].floor_y + 1;
            let near_door = |x: i32, z: i32| r.doors.iter().any(|d| d.cells.iter().any(|c| (c.0 - x).abs() <= 2 && (c.1 - z).abs() <= 2));
            for &(x, z) in &r.cells {
                let patch: Vec<[i32; 3]> = (0..2).flat_map(|dx| (0..4).map(move |dz| [x + dx, y, z + dz])).collect();
                if patch.iter().all(|v| r.cells.contains(&(v[0], v[2])) && !taken.contains(v) && !near_door(v[0], v[2])) {
                    for v in &patch {
                        taken.insert(*v);
                    }
                    return Some([x, y, z]);
                }
            }
        }
        None
    };
    let py = plan.decks[pr.deck].floor_y + 1;
    let Some(bat) = slot(&mut taken) else { return (b, circuits) };
    let brk = [bat[0], bat[1], bat[2] + 2];
    b.part("battery_small", [bat[0] + 1, py, bat[2]], ID, None);
    b.part("electric_curcuit_breaker", brk, ID, None);
    b.link(4, port_voxel([bat[0] + 1, py, bat[2]], &ID, [0, 0, 0]), port_voxel(brk, &ID, [0, 0, 0]));
    let bus = port_voxel(brk, &ID, [0, 1, 0]);
    let (px0, _, pz0, _) = pr.bbox();
    // master button: on the bridge (a wall cell next to the room's door), else beside the panel
    let master = plan
        .rooms
        .iter()
        .find(|r| r.kind == RoomKind::Bridge)
        .and_then(|r| r.switch_at.map(|((x, z), _)| [x + if x >= 0 { 1 } else { -1 }, plan.decks[r.deck].floor_y + 4, z]))
        .unwrap_or([px0, py + 3, pz0]);
    b.part("button_push", master, orient(2, 4), None);
    b.link(4, bus, master);
    for c in circuits.iter() {
        let Some(g) = slot(&mut taken) else { break };
        let or_vp = g;
        let tg_vp = [g[0], g[1], g[2] + 2];
        b.part("gate_bool_or", or_vp, ID, None);
        b.part("gate_push_to_toggle", tg_vp, ID, None);
        // the room's push button, facing into the room from its wall
        let wall = plan.rooms.iter().find(|r| Some(r.id) == c.room).and_then(|r| r.switch_at).map(|(_, w)| w).unwrap_or(0);
        let into = wall ^ 1;
        let up = if into / 2 == 1 { 4 } else { 2 };
        b.part("button_push", c.button, orient(into, up), None);
        b.link(0, c.button, port_voxel(or_vp, &ID, PORT_A));
        b.link(0, master, port_voxel(or_vp, &ID, PORT_B));
        b.link(0, port_voxel(or_vp, &ID, OR_OUT), port_voxel(tg_vp, &ID, TOGGLE_IN));
        b.link(4, bus, c.button);
        for l in &c.lights {
            let k = opt.light.get(l).copied().unwrap_or(c.kelvin);
            // small light: emits along local +y; on the ceiling it points down
            let i = b.part("small_light", *l, orient(3, 4), None);
            b.parts[i].ac = Some(light_colour(k, c.brightness));
            b.link(0, port_voxel(tg_vp, &ID, TOGGLE_OUT), *l);
            b.link(4, bus, *l);
        }
    }
    (b, circuits)
}
