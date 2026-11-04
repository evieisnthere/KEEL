//! Building the plan: floors (open over the access core), walls with door openings, sliding doors, stairs and
//! ladders.
use crate::build::{orient, Build, ID};
use crate::plan::{AccessKind, Cell, Plan};

#[derive(Debug, Clone)]
pub struct Finish {
    pub wall: String,
    pub floor: String,
    pub corridor_floor: String,
    /// "door_manual" (sliding) or "" for open doorways
    pub door: String,
}

impl Default for Finish {
    fn default() -> Self {
        Finish { wall: "D9D4C7".into(), floor: "7A6048".into(), corridor_floor: "5C6670".into(), door: "door_manual".into() }
    }
}

pub fn build_structure(plan: &Plan, f: &Finish) -> Build {
    let mut b = Build::default();
    let core = plan.core;
    let in_core = |x: i32, z: i32| core.is_some_and(|(x0, x1, z0, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1);
    let top_core_deck = plan.core_decks.iter().max().copied();
    for (di, d) in plan.decks.iter().enumerate() {
        let g = &plan.grids[di];
        // floor: open over the core on every deck the core continues up from (not the lowest one)
        let lowest_core = plan.core_decks.iter().min().copied();
        for &(x, z) in &d.floor {
            if plan.core_decks.contains(&di) && Some(di) != lowest_core && in_core(x, z) {
                continue;
            }
            let colour = if matches!(g.get(&(x, z)), Some(Cell::Corridor)) { &f.corridor_floor } else { &f.floor };
            b.block([x, d.floor_y, z], colour);
        }
        // walls, minus door openings
        let mut openings: std::collections::HashSet<(i32, i32)> = Default::default();
        for r in plan.rooms.iter().filter(|r| r.deck == di) {
            for door in &r.doors {
                openings.extend(door.cells.iter().copied());
            }
        }
        for (&(x, z), c) in g {
            if *c != Cell::Wall {
                continue;
            }
            for y in d.floor_y + 1..=d.floor_y + d.clear {
                // door openings are 7 rows tall; the lintel above stays
                if openings.contains(&(x, z)) && y <= d.floor_y + 7 {
                    continue;
                }
                b.block([x, y, z], &f.wall);
            }
        }
        // doors
        if !f.door.is_empty() {
            for r in plan.rooms.iter().filter(|r| r.deck == di) {
                for door in &r.doors {
                    let y = d.floor_y + 4;
                    if door.along_z {
                        // panel plane x = const, local z −2..0 → world z: anchor at the highest z of the opening
                        let x = door.cells[0].0;
                        let zh = door.cells.iter().map(|c| c.1).max().unwrap();
                        b.part(&f.door, [x, y, zh], ID, None);
                    } else {
                        // plane z = const: local x → world +z, local z → world −x, so local z −2..0 covers x .. x + 2
                        let z = door.cells[0].1;
                        let xl = door.cells.iter().map(|c| c.0).min().unwrap();
                        b.part(&f.door, [xl, y, z], orient(2, 1), None);
                    }
                }
            }
        }
        // access core
        if let (Some((x0, _x1, z0, _z1)), true) = (core, plan.core_decks.contains(&di)) {
            if Some(di) == top_core_deck {
                continue;
            }
            match plan.access {
                AccessKind::Stairs => {
                    // steps rise 1 row per 2 voxels toward +z from the floor to the next floor
                    for k in 0..d.pitch {
                        let def = if k + 1 == d.pitch { "stair_top" } else { "stair_segment" };
                        b.part(def, [x0, d.floor_y + 1 + k, z0 + 1 + 2 * k], ID, None);
                    }
                }
                AccessKind::Ladder => {
                    // a ladder against the core's aft side, every 4 rows up to the floor above
                    let mut y = d.floor_y + 1;
                    while y <= d.floor_y + d.pitch {
                        b.part("ladder_small", [x0 + 1, y, z0], ID, None);
                        y += 4;
                    }
                }
            }
        }
    }
    b
}
