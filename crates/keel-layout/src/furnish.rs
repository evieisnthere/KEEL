//! Furniture: block-built prefabs (toilets, basins, counters, cookers, fridges, wardrobes, tables, sofas, consoles,
//! shelving) plus real parts where the game has them (beds, seats, helm). Items stand along walls, clear of door
//! approaches and the light switch, and never reach the ceiling row (lights go there).
use crate::build::{dir_of, dir_vec, orient, Build};
use crate::plan::{Cell, Plan, Room};
use crate::programme::RoomKind;
use std::collections::{BTreeSet, HashSet};

pub const WOOD: &str = "8A6A4A";
pub const DARK_WOOD: &str = "5A4232";
pub const WHITE: &str = "F2F2EE";
pub const STEEL: &str = "9AA0A6";
pub const DARK: &str = "3A3D40";
pub const CUSHION: &str = "3E5C7A";
pub const CONSOLE: &str = "2E3238";

/// One piece of an item in its local frame: u along the wall, v away from the wall (into the room), y up from the
/// floor surface (0 = the first row above the floor).
enum Piece {
    Block { u: i32, v: i32, y: i32, colour: &'static str },
    /// a real part: definition, local origin, and the local direction its +z should face (0 = along +v, 1 = along +u)
    Part { def: &'static str, u: i32, v: i32, y: i32, face_v: bool, flip: bool },
}

struct Item {
    /// width along the wall and depth into the room (cells)
    w: i32,
    d: i32,
    pieces: Vec<Piece>,
}

fn blocks(w: i32, d: i32, h: i32, colour: &'static str) -> Vec<Piece> {
    let mut v = vec![];
    for u in 0..w {
        for dv in 0..d {
            for y in 0..h {
                v.push(Piece::Block { u, v: dv, y, colour });
            }
        }
    }
    v
}

fn item(kind: &str) -> Item {
    match kind {
        // seat_bed: 3 wide (local x −1..1), 7 long (local z −3..3), mattress row 0; long axis into the room
        "bed" => Item { w: 3, d: 7, pieces: vec![Piece::Part { def: "seat_bed", u: 1, v: 3, y: 0, face_v: true, flip: false }] },
        "wardrobe" => Item { w: 3, d: 2, pieces: blocks(3, 2, 7, WOOD) },
        "bedside" => Item { w: 1, d: 1, pieces: blocks(1, 1, 2, DARK_WOOD) },
        "toilet" => Item {
            w: 2,
            d: 3,
            pieces: vec![
                // cistern against the wall, bowl in front
                Piece::Block { u: 0, v: 0, y: 0, colour: WHITE },
                Piece::Block { u: 1, v: 0, y: 0, colour: WHITE },
                Piece::Block { u: 0, v: 0, y: 1, colour: WHITE },
                Piece::Block { u: 1, v: 0, y: 1, colour: WHITE },
                Piece::Block { u: 0, v: 0, y: 2, colour: WHITE },
                Piece::Block { u: 1, v: 0, y: 2, colour: WHITE },
                Piece::Block { u: 0, v: 1, y: 0, colour: WHITE },
                Piece::Block { u: 1, v: 1, y: 0, colour: WHITE },
                Piece::Block { u: 0, v: 1, y: 1, colour: WHITE },
                Piece::Block { u: 1, v: 1, y: 1, colour: WHITE },
            ],
        },
        "basin" => Item { w: 2, d: 1, pieces: vec![Piece::Block { u: 0, v: 0, y: 3, colour: WHITE }, Piece::Block { u: 1, v: 0, y: 3, colour: WHITE }] },
        "counter" => {
            let mut p = blocks(4, 2, 3, WOOD);
            p.extend((0..4).flat_map(|u| (0..2).map(move |v| Piece::Block { u, v, y: 3, colour: DARK })));
            Item { w: 4, d: 2, pieces: p }
        }
        "cooker" => {
            let mut p = blocks(2, 2, 3, STEEL);
            p.extend((0..2).flat_map(|u| (0..2).map(move |v| Piece::Block { u, v, y: 3, colour: DARK })));
            Item { w: 2, d: 2, pieces: p }
        }
        "fridge" => Item { w: 2, d: 2, pieces: blocks(2, 2, 7, WHITE) },
        "shelf" => {
            let mut p = vec![];
            for u in 0..4 {
                for y in [0, 2, 4, 6] {
                    p.push(Piece::Block { u, v: 0, y, colour: STEEL });
                }
            }
            Item { w: 4, d: 1, pieces: p }
        }
        "sofa" => {
            let mut p = vec![];
            for u in 0..5 {
                p.push(Piece::Block { u, v: 0, y: 0, colour: CUSHION });
                p.push(Piece::Block { u, v: 0, y: 1, colour: CUSHION });
                p.push(Piece::Block { u, v: 0, y: 2, colour: CUSHION });
                p.push(Piece::Block { u, v: 1, y: 0, colour: CUSHION });
            }
            Item { w: 5, d: 2, pieces: p }
        }
        "console" => {
            let mut p = blocks(5, 2, 3, CONSOLE);
            p.extend((0..5).map(|u| Piece::Block { u, v: 0, y: 3, colour: CONSOLE }));
            Item { w: 5, d: 2, pieces: p }
        }
        "machine" => Item { w: 2, d: 2, pieces: blocks(2, 2, 4, WHITE) },
        "cabinet" => Item { w: 2, d: 1, pieces: blocks(2, 1, 5, WHITE) },
        // passenger seat: 3 wide (local x −1..1), 2 deep (local z −1..0); backrest at −z, so it faces +z
        "seat_row" => Item { w: 3, d: 2, pieces: vec![Piece::Part { def: "seat_passenger", u: 1, v: 1, y: 0, face_v: true, flip: false }] },
        "medical_bed" => Item { w: 3, d: 7, pieces: vec![Piece::Part { def: "seat_medical", u: 1, v: 3, y: 0, face_v: true, flip: false }] },
        _ => Item { w: 1, d: 1, pieces: vec![] },
    }
}

/// What goes in each room, in order of priority: (item, how many; 0 = as many as fit).
fn contents(kind: RoomKind) -> Vec<(&'static str, usize)> {
    use RoomKind::*;
    match kind {
        CrewCabin | PassengerCabin | GuestCabin => vec![("bed", 2), ("wardrobe", 1), ("bedside", 1)],
        CaptainCabin | OwnerCabin | Suite => vec![("bed", 1), ("wardrobe", 2), ("bedside", 2), ("sofa", 1)],
        Head => vec![("toilet", 1), ("basin", 1)],
        Galley => vec![("fridge", 1), ("cooker", 1), ("counter", 0)],
        CrewMess | Restaurant => vec![("sofa", 0)],
        Saloon | Lounge => vec![("sofa", 0)],
        Bar => vec![("counter", 2), ("sofa", 0)],
        Bridge => vec![("console", 0)],
        PassengerSeating | SurvivorBay => vec![("seat_row", 0)],
        Store | Workshop | Armoury => vec![("shelf", 0)],
        Shop | Reception => vec![("counter", 2), ("shelf", 2)],
        Laundry => vec![("machine", 0)],
        Medical => vec![("medical_bed", 1), ("cabinet", 2)],
        Gym => vec![("shelf", 1)],
        EngineRoom | Electrical | CarDeck => vec![],
    }
}

/// Free floor cells next to a wall, by the wall's direction (0 +x, 1 −x, 4 +z, 5 −z): (cell, wall dir).
fn wall_cells(room: &Room) -> Vec<((i32, i32), usize)> {
    let mut out = vec![];
    for &(x, z) in &room.cells {
        for s in [0usize, 1, 4, 5] {
            let d = dir_vec(s);
            if !room.cells.contains(&(x + d[0], z + d[2])) {
                out.push(((x, z), s));
            }
        }
    }
    out
}

/// Cells kept clear: door approaches (2 deep inside the room) and the switch.
fn keep_clear(plan: &Plan, room: &Room) -> HashSet<(i32, i32)> {
    let mut k = HashSet::new();
    for door in &room.doors {
        for &(x, z) in &door.cells {
            for dx in -2..=2 {
                for dz in -2..=2 {
                    k.insert((x + dx, z + dz));
                }
            }
        }
    }
    if let Some((c, _)) = room.switch_at {
        k.insert(c);
    }
    let _ = plan;
    k
}

/// Furnish every room. Returns the build and the used voxels.
pub fn furnish(plan: &Plan) -> (Build, HashSet<[i32; 3]>) {
    let mut b = Build::default();
    let mut used: HashSet<[i32; 3]> = HashSet::new();
    for room in &plan.rooms {
        let deck = &plan.decks[room.deck];
        let y0 = deck.floor_y + 1;
        let clear = keep_clear(plan, room);
        let mut taken: BTreeSet<(i32, i32)> = BTreeSet::new();
        let walls = wall_cells(room);
        for (name, n) in contents(room.kind) {
            let it = item(name);
            let mut placed = 0;
            'cands: for &((x, z), s) in &walls {
                if n > 0 && placed >= n {
                    break;
                }
                let sv = dir_vec(s);
                // u runs along the wall (perpendicular to s), v into the room (−s)
                let uv = if sv[0] != 0 { [0, 0, 1] } else { [1, 0, 0] };
                let cell = |u: i32, v: i32| (x + u * uv[0] - v * sv[0], z + u * uv[2] - v * sv[2]);
                for u in 0..it.w {
                    for v in 0..it.d {
                        let c = cell(u, v);
                        if !room.cells.contains(&c) || taken.contains(&c) || clear.contains(&c) {
                            continue 'cands;
                        }
                    }
                    // the whole back row must be against the wall
                    let (bx, bz) = cell(u, 0);
                    if room.cells.contains(&(bx + sv[0], bz + sv[2])) {
                        continue 'cands;
                    }
                }
                for u in 0..it.w {
                    for v in 0..it.d {
                        taken.insert(cell(u, v));
                    }
                }
                let into = dir_of([-sv[0], 0, -sv[2]]);
                let along = dir_of(uv);
                for p in &it.pieces {
                    match p {
                        Piece::Block { u, v, y, colour } => {
                            let c = cell(*u, *v);
                            let vp = [c.0, y0 + y, c.1];
                            if used.insert(vp) {
                                b.block(vp, colour);
                            }
                        }
                        Piece::Part { def, u, v, y, face_v, flip } => {
                            let c = cell(*u, *v);
                            let fwd = if *face_v { into } else { along };
                            let fwd = if *flip { fwd ^ 1 } else { fwd };
                            b.part(def, [c.0, y0 + y, c.1], orient(2, fwd), None);
                            used.insert([c.0, y0 + y, c.1]);
                        }
                    }
                }
                placed += 1;
            }
        }
        let _ = Cell::Wall;
    }
    (b, used)
}
