//! Usable interior space as decks of voxel cells.
//!
//! A deck is a floor row plus the (x, z) cells above it with [`CLEAR`] free rows: inside the hull or superstructure,
//! one voxel clear of the shell, and not already occupied by shell blocks. Hull decks are stacked down from the weather
//! deck every [`PITCH`] rows; superstructure decks are its tiers.
use crate::superstructure::Superstructure;
use std::collections::{BTreeSet, HashSet};

pub const PITCH: i32 = 9;
pub const CLEAR: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckKind {
    /// below the weather deck: 0 = the one just under it
    Below(usize),
    /// superstructure tier (0 = on the weather deck)
    Tier(usize),
}

#[derive(Debug, Clone)]
pub struct Deck {
    pub kind: DeckKind,
    /// row of the floor (the voxel row walked on is `floor_y + 1`)
    pub floor_y: i32,
    /// free rows above the floor (headroom), and floor-to-floor height (clear + 1)
    pub clear: i32,
    pub pitch: i32,
    /// usable cells (x, z), both halves
    pub cells: BTreeSet<(i32, i32)>,
    /// floor blocks to build (x, z): every inside, unoccupied voxel of the floor row (hull decks and upper tiers)
    pub floor: BTreeSet<(i32, i32)>,
}

impl Deck {
    pub fn name(&self) -> String {
        match self.kind {
            DeckKind::Below(0) => "Lower deck".into(),
            DeckKind::Below(k) => format!("Lower deck {}", k + 1),
            DeckKind::Tier(0) => "Main deck".into(),
            DeckKind::Tier(k) => format!("Deck {}", k + 1),
        }
    }
    pub fn z_range(&self) -> Option<(i32, i32)> {
        let z0 = self.cells.iter().map(|c| c.1).min()?;
        let z1 = self.cells.iter().map(|c| c.1).max()?;
        Some((z0, z1))
    }
    /// Largest starboard x of the deck at `z` (−1 when the deck has no cell there).
    pub fn xmax(&self, z: i32) -> i32 {
        self.cells.iter().filter(|c| c.1 == z).map(|c| c.0).max().unwrap_or(-1)
    }
    pub fn area_m2(&self) -> f64 {
        self.cells.len() as f64 * 0.0625
    }
}

/// The interior: an inside test at voxel centres (hull) and the occupied shell voxels.
pub struct Interior<'a> {
    pub hull_inside: &'a dyn Fn([i32; 3]) -> bool,
    pub occupied: &'a HashSet<[i32; 3]>,
    /// weather deck plating row (at midships)
    pub deck_y: i32,
    pub bounds: ([i32; 3], [i32; 3]),
}

fn usable(inside: &dyn Fn([i32; 3]) -> bool, occupied: &HashSet<[i32; 3]>, x: i32, z: i32, f: i32, clear: i32) -> bool {
    (f + 1..=f + clear).all(|y| {
        [[0, 0], [1, 0], [-1, 0], [0, 1], [0, -1]].iter().all(|d| inside([x + d[0], y, z + d[1]])) && !occupied.contains(&[x, y, z])
    })
}

/// Suggested hull deck floors (rows, top first): every [`PITCH`] rows down from the weather deck while the deck keeps a
/// useful size.
pub fn suggest_hull_floors(it: &Interior) -> Vec<i32> {
    let mut floors = vec![];
    let mut first = 0usize;
    for k in 1.. {
        let f = it.deck_y - PITCH * k;
        if f < it.bounds.0[1] {
            break;
        }
        let n = deck_cells(it, f, CLEAR).0.len();
        if k == 1 {
            first = n;
        }
        if n < 24 || n * 5 < first {
            break;
        }
        floors.push(f);
    }
    floors
}

fn deck_cells(it: &Interior, f: i32, clear: i32) -> (BTreeSet<(i32, i32)>, BTreeSet<(i32, i32)>) {
    let (lo, hi) = it.bounds;
    let mut cells = BTreeSet::new();
    let mut floor = BTreeSet::new();
    for z in lo[2]..=hi[2] {
        for x in lo[0]..=hi[0] {
            if usable(it.hull_inside, it.occupied, x, z, f, clear) {
                cells.insert((x, z));
            }
            if (it.hull_inside)([x, f, z]) && !it.occupied.contains(&[x, f, z]) {
                floor.insert((x, z));
            }
        }
    }
    (cells, floor)
}

/// Hull decks at the given floor rows (any order). Each deck's headroom runs up to the next floor above it, or to the
/// weather deck. Returned top first.
pub fn hull_decks_at(it: &Interior, floors: &[i32]) -> Vec<Deck> {
    let mut fs: Vec<i32> = floors.iter().copied().filter(|f| *f < it.deck_y - 1).collect();
    fs.sort_unstable_by(|a, b| b.cmp(a));
    fs.dedup();
    let mut out = vec![];
    let mut ceiling = it.deck_y;
    for (k, f) in fs.into_iter().enumerate() {
        let clear = ceiling - f - 1;
        let (cells, floor) = deck_cells(it, f, clear);
        out.push(Deck { kind: DeckKind::Below(k), floor_y: f, clear, pitch: clear + 1, cells, floor });
        ceiling = f;
    }
    out
}

/// Decks inside the hull below the weather deck at the suggested floors, top first.
pub fn hull_decks(it: &Interior) -> Vec<Deck> {
    hull_decks_at(it, &suggest_hull_floors(it))
}

/// Decks inside the superstructure tiers, bottom first. The lowest stands on the weather deck (no floor to build).
pub fn superstructure_decks(s: &Superstructure, occupied: &HashSet<[i32; 3]>) -> Vec<Deck> {
    let solid = s.solid();
    let inside = |v: [i32; 3]| solid.voxel_inside(v);
    let mut out = vec![];
    for (k, t) in s.tiers.iter().enumerate() {
        let f = s.tier_floor(k);
        let clear = t.height - 1;
        let mut cells = BTreeSet::new();
        let mut floor = BTreeSet::new();
        for z in t.aft - 1..=t.fwd + 1 {
            for x in -t.half_width - 1..=t.half_width + 1 {
                if usable(&inside, occupied, x, z, f, clear) {
                    cells.insert((x, z));
                }
                // an upper tier's floor: under that tier only (its own bottom row is inside the tier above f)
                if k > 0 && inside([x, f, z]) && inside([x, f + 1, z]) && !occupied.contains(&[x, f, z]) && !occupied.contains(&[x, f + 1, z]) {
                    floor.insert((x, z));
                }
            }
        }
        if !cells.is_empty() {
            out.push(Deck { kind: DeckKind::Tier(k), floor_y: f, clear, pitch: t.height, cells, floor });
        }
    }
    out
}
