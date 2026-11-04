//! Floor plans: rooms, corridors, walls, doors and the vertical access core (stairs or ladders).
//!
//! * A deck 15 voxels wide or more gets a 3-wide centre corridor; rooms fill the strips on each side (interior from
//!   |x| = 3; the corridor walls are at |x| = 2) and open onto it.
//! * Narrow decks, and large rooms on any deck, use full-width rooms in sequence with doorways on the centre line.
//! * Rooms are assigned to decks by preference (below, main, upper, top) and laid out along the deck by zone
//!   (aft → mid → fwd); a room that does not fit goes to the next suitable deck.
//! * One vertical core sits at the same z on every deck: stairs in the port strip on long, wide ships, otherwise a
//!   ladder on the centre line; the floors above are opened over it.
use crate::programme::{DeckPref, RoomKind, RoomReq, ZonePref};
use crate::space::{Deck, DeckKind};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    Room(usize),
    Corridor,
    Core,
    Wall,
    /// unused space behind walls (not built)
    Void,
}

#[derive(Debug, Clone)]
pub struct Door {
    /// opening cells (x, z) in the wall
    pub cells: Vec<(i32, i32)>,
    /// the wall plane runs along z (side door, normal ±x) or along x (cross wall, normal ±z)
    pub along_z: bool,
}

#[derive(Debug, Clone)]
pub struct Room {
    pub id: usize,
    pub kind: RoomKind,
    pub deck: usize,
    pub cells: BTreeSet<(i32, i32)>,
    pub doors: Vec<Door>,
    /// the room's light switch goes on the wall next to its first door, on the room side: (cell, wall direction)
    pub switch_at: Option<((i32, i32), usize)>,
}

impl Room {
    pub fn bbox(&self) -> (i32, i32, i32, i32) {
        let x0 = self.cells.iter().map(|c| c.0).min().unwrap_or(0);
        let x1 = self.cells.iter().map(|c| c.0).max().unwrap_or(0);
        let z0 = self.cells.iter().map(|c| c.1).min().unwrap_or(0);
        let z1 = self.cells.iter().map(|c| c.1).max().unwrap_or(0);
        (x0, x1, z0, z1)
    }
    pub fn area_m2(&self) -> f64 {
        self.cells.len() as f64 * 0.0625
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    Stairs,
    Ladder,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub decks: Vec<Deck>,
    pub grids: Vec<BTreeMap<(i32, i32), Cell>>,
    pub rooms: Vec<Room>,
    pub access: AccessKind,
    /// access core: x range and z range (inclusive), the same on every deck it serves
    pub core: Option<(i32, i32, i32, i32)>,
    pub core_decks: Vec<usize>,
    pub unplaced: Vec<RoomKind>,
}

#[derive(Debug, Clone)]
pub struct PlanOptions {
    /// decks at least this wide (voxels) get a centre corridor
    pub corridor_width: i32,
    /// force stairs (Some(true)) or ladders (Some(false)); None = stairs on decks ≥ 40 long and ≥ 15 wide
    pub stairs: Option<bool>,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions { corridor_width: 15, stairs: None }
    }
}

fn full_width(kind: RoomKind, area: f64) -> bool {
    use RoomKind::*;
    matches!(kind, EngineRoom | CarDeck | Restaurant | Saloon | Lounge | PassengerSeating | SurvivorBay | Bridge) || area > 30.0
}

fn zone_rank(z: ZonePref) -> i32 {
    match z {
        ZonePref::Aft => 0,
        ZonePref::Mid | ZonePref::Any => 1,
        ZonePref::Fwd => 2,
    }
}

/// Decks in preference order for a room.
fn deck_order(decks: &[Deck], pref: DeckPref, kind: RoomKind) -> Vec<usize> {
    let below: Vec<usize> = (0..decks.len()).filter(|&i| matches!(decks[i].kind, DeckKind::Below(_))).collect();
    let tiers: Vec<usize> = (0..decks.len()).filter(|&i| matches!(decks[i].kind, DeckKind::Tier(_))).collect();
    // `decks` is bottom first: `below` runs lowest → just under the weather deck
    let mut order: Vec<usize> = match (pref, kind) {
        (_, RoomKind::EngineRoom) => below.clone(),
        (_, RoomKind::CarDeck) => below.iter().rev().copied().chain(tiers.first().copied()).collect(),
        (DeckPref::Below, _) => below.iter().rev().copied().collect(),
        (DeckPref::Main, _) => tiers.first().copied().into_iter().chain(below.last().copied()).collect(),
        (DeckPref::Upper, _) => tiers.iter().skip(1).copied().chain(tiers.first().copied()).collect(),
        (DeckPref::Top, _) => tiers.last().copied().into_iter().collect(),
        (DeckPref::Any, _) => below.iter().rev().chain(tiers.iter()).copied().collect(),
    };
    for i in below.iter().chain(tiers.iter()) {
        if !order.contains(i) && kind != RoomKind::Bridge {
            order.push(*i);
        }
    }
    order
}

struct Want {
    kind: RoomKind,
    area: f64,
    zone: ZonePref,
}

/// Lay out one deck. Returns the rooms (with deck-relative ids from `next_id`) and the cell grid, or None when the
/// rooms do not fit.
fn lay_deck(deck: &Deck, di: usize, wants: &[Want], core: Option<(i32, i32, i32, i32)>, corridor: bool, next_id: usize) -> Option<(Vec<Room>, BTreeMap<(i32, i32), Cell>)> {
    let (z0, z1) = deck.z_range()?;
    let mut grid: BTreeMap<(i32, i32), Cell> = BTreeMap::new();
    let has = |x: i32, z: i32| deck.cells.contains(&(x, z));
    let xmax = |z: i32| (0..=200).rev().find(|&x| has(x, z)).unwrap_or(-1);
    if let Some((cx0, cx1, cz0, cz1)) = core {
        for z in cz0..=cz1 {
            for x in cx0..=cx1 {
                if has(x, z) {
                    grid.insert((x, z), Cell::Core);
                }
            }
        }
    }
    let mut order: Vec<&Want> = wants.iter().collect();
    order.sort_by_key(|w| (zone_rank(w.zone), w.kind));
    let mut rooms: Vec<Room> = vec![];
    // cursors: next free z for the port strip, the starboard strip, and full-width rooms
    let (mut zp, mut zs) = (z0, z0);
    let free = |grid: &BTreeMap<(i32, i32), Cell>, x: i32, z: i32| has(x, z) && !grid.contains_key(&(x, z));
    for w in order {
        let fw = !corridor || full_width(w.kind, w.area);
        let need = (w.area / 0.0625).ceil() as usize;
        let id = next_id + rooms.len();
        let mut cells = BTreeSet::new();
        if fw {
            let mut z = zp.max(zs);
            // skip a core in the way
            while z <= z1 && (-xmax(z)..=xmax(z)).any(|x| grid.get(&(x, z)) == Some(&Cell::Core)) && corridor {
                z += 1;
            }
            let start = z;
            while cells.len() < need && z <= z1 {
                for x in -xmax(z)..=xmax(z) {
                    if free(&grid, x, z) {
                        cells.insert((x, z));
                    }
                }
                z += 1;
            }
            if cells.len() < need || z - start < 4 {
                return None;
            }
            // wall across the fwd end (unless at the deck end)
            for x in -xmax(z)..=xmax(z) {
                if free(&grid, x, z) {
                    grid.insert((x, z), Cell::Wall);
                }
            }
            for c in &cells {
                grid.insert(*c, Cell::Room(id));
            }
            zp = z + 1;
            zs = z + 1;
        } else {
            // the strip with the lower cursor; starboard on ties
            let port = zp < zs;
            let mut z = if port { zp } else { zs };
            let sgn = if port { -1 } else { 1 };
            let start = z;
            while cells.len() < need && z <= z1 {
                let xm = xmax(z);
                let row: Vec<(i32, i32)> = (3..=xm).map(|x| (sgn * x, z)).filter(|c| free(&grid, c.0, c.1)).collect();
                if row.len() < 3 || row.len() < (xm - 2) as usize {
                    // core or narrowing: start after it
                    if cells.is_empty() {
                        z += 1;
                        continue;
                    }
                    break;
                }
                cells.extend(row);
                z += 1;
            }
            let _ = start;
            let zl = cells.iter().map(|c| c.1).min().unwrap_or(z) - 1;
            let zh = cells.iter().map(|c| c.1).max().unwrap_or(z) + 1;
            if cells.len() < need || zh - zl < 5 {
                return None;
            }
            for zz in zl..=zh {
                for x in 2..=xmax(zz) {
                    let c = (sgn * x, zz);
                    if free(&grid, c.0, c.1) && !cells.contains(&c) {
                        grid.insert(c, Cell::Wall);
                    }
                }
            }
            for c in &cells {
                grid.insert(*c, Cell::Room(id));
            }
            if port {
                zp = zh + 1;
            } else {
                zs = zh + 1;
            }
        }
        rooms.push(Room { id, kind: w.kind, deck: di, cells, doors: vec![], switch_at: None });
    }
    // corridor: the remaining centre cells; side strips left over are empty voids behind the corridor wall
    for &(x, z) in &deck.cells {
        if grid.contains_key(&(x, z)) {
            continue;
        }
        let c = if corridor && x.abs() <= 1 {
            Cell::Corridor
        } else if corridor && x.abs() == 2 {
            Cell::Wall
        } else if !corridor {
            Cell::Corridor
        } else {
            Cell::Void
        };
        grid.insert((x, z), c);
    }
    // doors
    for r in rooms.iter_mut() {
        let (x0, x1, z0r, z1r) = r.bbox();
        let zc = (z0r + z1r) / 2;
        if x0 >= 3 || x1 <= -3 {
            // side room: door in the corridor wall at x = ±2, 3 wide along z
            let wx = if x0 >= 3 { 2 } else { -2 };
            let cells: Vec<(i32, i32)> = (zc - 1..=zc + 1).map(|z| (wx, z)).collect();
            if cells.iter().all(|c| grid.get(c) == Some(&Cell::Wall)) {
                r.switch_at = Some(((wx.signum() * 3, zc + 2), if wx > 0 { 1 } else { 0 }));
                r.doors.push(Door { cells, along_z: true });
            }
        } else {
            // full-width room: doorways on the centre line in its aft and fwd walls where something is beyond
            for (wz, beyond) in [(z0r - 1, z0r - 2), (z1r + 1, z1r + 2)] {
                let cells: Vec<(i32, i32)> = (-1..=1).map(|x| (x, wz)).collect();
                let open_beyond = (-1..=1).any(|x| matches!(grid.get(&(x, beyond)), Some(Cell::Corridor) | Some(Cell::Room(_)) | Some(Cell::Core)));
                if open_beyond && cells.iter().all(|c| grid.get(c) == Some(&Cell::Wall)) {
                    if r.switch_at.is_none() {
                        let inz = if wz < z0r { wz + 1 } else { wz - 1 };
                        r.switch_at = Some(((2, inz), if wz < z0r { 5 } else { 4 }));
                    }
                    r.doors.push(Door { cells, along_z: false });
                }
            }
        }
    }
    Some((rooms, grid))
}

/// Make the floor plan for `decks` (bottom first) and the programme.
pub fn make_plan(decks: Vec<Deck>, prog: &[RoomReq], opt: &PlanOptions) -> Plan {
    let width = |d: &Deck| {
        let Some((z0, z1)) = d.z_range() else { return 0 };
        let zm = (z0 + z1) / 2;
        2 * (0..=200).rev().find(|&x| d.cells.contains(&(x, zm))).unwrap_or(0) + 1
    };
    let corridors: Vec<bool> = decks.iter().map(|d| width(d) >= opt.corridor_width).collect();
    // access core on every deck that contains it: z centred on the main deck (or the top hull deck)
    let main = decks.iter().position(|d| d.kind == DeckKind::Tier(0)).or(decks.iter().position(|d| d.kind == DeckKind::Below(0)));
    let long = decks.iter().all(|d| d.z_range().map(|(a, b)| b - a >= 40).unwrap_or(false));
    let stairs = opt.stairs.unwrap_or(long && corridors.iter().all(|c| *c));
    let access = if stairs { AccessKind::Stairs } else { AccessKind::Ladder };
    let core = main.and_then(|m| decks[m].z_range()).map(|(z0, z1)| {
        let zc = (z0 + z1) / 2;
        match access {
            // stairs: 3 wide in the port strip, 2 per row of rise plus landings
            AccessKind::Stairs => (-5, -3, zc - 1, zc + 2 * decks.iter().map(|d| d.pitch).max().unwrap_or(9)),
            // a ladder: in the port strip when every deck has a corridor (so it does not block it), else centred
            AccessKind::Ladder if decks.iter().all(|d| d.cells.contains(&(-6, zc)) && d.cells.contains(&(-6, zc + 2))) => (-5, -3, zc, zc + 2),
            AccessKind::Ladder => (-1, 1, zc, zc + 2),
        }
    });
    let core_decks: Vec<usize> = (0..decks.len())
        .filter(|&i| core.is_some_and(|(x0, x1, z0, z1)| (z0..=z1).all(|z| (x0..=x1).all(|x| decks[i].cells.contains(&(x, z))))))
        .collect();
    // expand the programme into rooms and assign decks greedily
    let mut wants: Vec<Vec<Want>> = (0..decks.len()).map(|_| vec![]).collect();
    let mut unplaced = vec![];
    let mut items: Vec<&RoomReq> = prog.iter().collect();
    // essentials first, big rooms first within a priority so they claim their preferred decks
    items.sort_by(|a, b| a.kind.priority().cmp(&b.kind.priority()).then(b.area.partial_cmp(&a.area).unwrap()));
    for q in items {
        for _ in 0..q.count {
            let mut placed = false;
            for di in deck_order(&decks, q.deck, q.kind) {
                let mut trial: Vec<Want> = wants[di].iter().map(|w| Want { kind: w.kind, area: w.area, zone: w.zone }).collect();
                trial.push(Want { kind: q.kind, area: q.area, zone: q.zone });
                let core_here = if core_decks.contains(&di) { core } else { None };
                if lay_deck(&decks[di], di, &trial, core_here, corridors[di], 0).is_some() {
                    wants[di] = trial;
                    placed = true;
                    break;
                }
            }
            if !placed {
                unplaced.push(q.kind);
            }
        }
    }
    let mut rooms = vec![];
    let mut grids = vec![];
    for (di, d) in decks.iter().enumerate() {
        let core_here = if core_decks.contains(&di) { core } else { None };
        let (rs, g) = lay_deck(d, di, &wants[di], core_here, corridors[di], rooms.len()).unwrap_or_else(|| (vec![], BTreeMap::new()));
        rooms.extend(rs);
        grids.push(g);
    }
    Plan { decks, grids, rooms, access, core, core_decks, unplaced }
}
