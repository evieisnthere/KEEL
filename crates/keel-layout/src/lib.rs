//! KEEL interior design (docs/engineering/layout.md).
//!
//! * [`programme`]: which rooms a boat type needs, scaled by size.
//! * [`space`]: the usable interior of the hull and superstructure as decks of voxel cells.
//! * [`plan`]: floor plans: corridors, rooms, stairs and ladders between decks.
//! * [`furnish`]: furniture built from blocks plus real parts (beds, seats, lights, windows, doors).
//! * [`electrics`]: lighting circuits (room push button and master → OR → push-to-toggle → lights) on a battery bus.
//! * [`superstructure`]: deckhouse tiers built with the same swept lattice curves as the hull.
//!
//! Coordinates are vehicle voxels: x athwartships (port −, starboard +), y up, z fore-aft (bow +).
pub mod build;
pub mod electrics;
pub mod furnish;
pub mod light;
pub mod plan;
pub mod programme;
pub mod space;
pub mod structure;
pub mod superstructure;

pub use build::Build;

use keel_geom::hullblocks::HullBlocks;
use keel_geom::voxel::VOXEL;

/// Everything the layout generator needs.
#[derive(Debug, Clone)]
pub struct LayoutOptions {
    pub programme: programme::ProgrammeInput,
    /// hull deck floor rows (None = suggested: every 9 rows down from the weather deck)
    pub hull_floors: Option<Vec<i32>>,
    pub superstructure: Option<superstructure::Superstructure>,
    pub plan: plan::PlanOptions,
    pub finish: structure::Finish,
    pub lighting: electrics::LightingOptions,
}

/// A generated interior.
pub struct Layout {
    pub plan: plan::Plan,
    pub build: Build,
    pub circuits: Vec<electrics::Circuit>,
    pub programme: Vec<programme::RoomReq>,
}

/// Weather deck plating row at midships for a built hull.
pub fn deck_row(hull: &HullBlocks) -> i32 {
    keel_geom::lattice::path_height(&hull.model.sheer, 0.5).floor() as i32 - 1
}

/// The interior space of a built hull: inside test, bounds and the weather deck row.
pub struct HullSpace<'a> {
    pub hull: &'a HullBlocks,
    pub lo: [i32; 3],
    pub hi: [i32; 3],
    pub deck_y: i32,
}

impl<'a> HullSpace<'a> {
    pub fn new(hull: &'a HullBlocks) -> Self {
        let o = &hull.occupied;
        let lo = [0, 1, 2].map(|i| o.iter().map(|v| v[i]).min().unwrap_or(0));
        let hi = [0, 1, 2].map(|i| o.iter().map(|v| v[i]).max().unwrap_or(0));
        HullSpace { hull, lo, hi, deck_y: deck_row(hull) }
    }
    pub fn inside(&self, v: [i32; 3]) -> bool {
        self.hull.model_inside_m([v[0] as f64 * VOXEL, (v[1] as f64 + 0.5) * VOXEL, v[2] as f64 * VOXEL])
    }
    pub fn with<R>(&self, f: impl FnOnce(&space::Interior) -> R) -> R {
        let inside = |v: [i32; 3]| self.inside(v);
        let it = space::Interior { hull_inside: &inside, occupied: &self.hull.occupied, deck_y: self.deck_y, bounds: (self.lo, self.hi) };
        f(&it)
    }
    /// Suggested hull deck floors (rows, top first).
    pub fn suggest_floors(&self) -> Vec<i32> {
        self.with(space::suggest_hull_floors)
    }
}

/// Decks (bottom first) and the floor plan, without building anything: fast enough for a live view.
pub fn plan_only(hull: &HullBlocks, opt: &LayoutOptions) -> plan::Plan {
    let hs = HullSpace::new(hull);
    let mut decks = hs.with(|it| match &opt.hull_floors {
        Some(f) => space::hull_decks_at(it, f),
        None => space::hull_decks(it),
    });
    decks.reverse();
    if let Some(s) = &opt.superstructure {
        decks.extend(space::superstructure_decks(s, &hull.occupied));
    }
    plan::make_plan(decks, &programme::programme(&opt.programme), &opt.plan)
}

/// Generate the interior (and superstructure) for a built hull.
pub fn generate(hull: &HullBlocks, opt: &LayoutOptions) -> Layout {
    let hs = HullSpace::new(hull);
    let mut decks = hs.with(|it| match &opt.hull_floors {
        Some(f) => space::hull_decks_at(it, f),
        None => space::hull_decks(it),
    });
    decks.reverse();
    let mut build = Build::default();
    if let Some(s) = &opt.superstructure {
        let sb = s.build();
        let occ: std::collections::HashSet<[i32; 3]> = hull.occupied.iter().copied().chain(sb.parts.iter().map(|p| p.vp)).collect();
        decks.extend(space::superstructure_decks(s, &occ));
        build.extend(sb);
    }
    let prog = programme::programme(&opt.programme);
    let plan = plan::make_plan(decks, &prog, &opt.plan);
    build.extend(structure::build_structure(&plan, &opt.finish));
    let (fb, used) = furnish::furnish(&plan);
    build.extend(fb);
    let (eb, circuits) = electrics::wire(&plan, &used, &opt.lighting);
    build.extend(eb);
    Layout { plan, build, circuits, programme: prog }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_geom::hull::HullParams;

    #[test]
    fn workboat_interior_on_the_default_hull() {
        let h = HullParams::default().build().unwrap();
        let hb = keel_geom::hullblocks::build(&h, &Default::default());
        let half = (h.params.beam / 2.0 / VOXEL) as i32;
        let zl = (h.params.length / 2.0 / VOXEL) as i32;
        let s = superstructure::Superstructure::default_for(deck_row(&hb), half, -zl, zl, 2);
        let opt = LayoutOptions {
            programme: programme::ProgrammeInput { boat: programme::BoatType::Workboat, length: h.params.length, beam: h.params.beam, crew: 0, passengers: 0 },
            hull_floors: None,
            superstructure: Some(s),
            plan: Default::default(),
            finish: Default::default(),
            lighting: Default::default(),
        };
        let l = generate(&hb, &opt);
        eprintln!("decks:");
        for d in &l.plan.decks {
            eprintln!("  {} floor {} cells {} ({:.0} m²)", d.name(), d.floor_y, d.cells.len(), d.area_m2());
        }
        for r in &l.plan.rooms {
            eprintln!("  room {} on {} {:?} {:.1} m², doors {}", r.kind.name(), l.plan.decks[r.deck].name(), r.bbox(), r.area_m2(), r.doors.len());
        }
        eprintln!("unplaced {:?}; access {:?} core {:?} on decks {:?}", l.plan.unplaced, l.plan.access, l.plan.core, l.plan.core_decks);
        eprintln!("parts {} links {} circuits {}", l.build.parts.len(), l.build.links.len(), l.circuits.len());
        assert!(l.plan.rooms.iter().any(|r| r.kind == programme::RoomKind::Bridge));
        assert!(l.plan.rooms.iter().any(|r| r.kind == programme::RoomKind::EngineRoom));
        assert!(!l.circuits.is_empty());
        // no two parts share an origin
        let mut seen = std::collections::HashSet::new();
        let dups: Vec<_> = l.build.parts.iter().filter(|p| !seen.insert(p.vp)).map(|p| (p.def.clone(), p.vp)).collect();
        assert!(dups.is_empty(), "{} duplicates, e.g. {:?}", dups.len(), &dups[..dups.len().min(8)]);
    }
}
