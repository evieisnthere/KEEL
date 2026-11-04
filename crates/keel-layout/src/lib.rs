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
    /// an edited room programme (None = the boat type's default for this size)
    pub rooms: Option<Vec<programme::RoomReq>>,
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
    keel_geom::lattice::path_height(&hull.model.sheer, 0.5).floor() as i32 - 1 + hull.origin[1]
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
        let o = self.hull.origin;
        self.hull.model_inside_m([(v[0] - o[0]) as f64 * VOXEL, (v[1] - o[1]) as f64 * VOXEL + 0.5 * VOXEL, (v[2] - o[2]) as f64 * VOXEL])
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
    plan::make_plan(decks, &opt.room_list(), &opt.plan)
}

impl LayoutOptions {
    /// The room programme in use.
    pub fn room_list(&self) -> Vec<programme::RoomReq> {
        self.rooms.clone().unwrap_or_else(|| programme::programme(&self.programme))
    }
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
    let prog = opt.room_list();
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
            rooms: None,
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

    #[test]
    fn regenerating_replaces_parts_and_wires() {
        let h = HullParams::default().build().unwrap();
        let hb = keel_geom::hullblocks::build(&h, &Default::default());
        let opt = LayoutOptions {
            programme: programme::ProgrammeInput { boat: programme::BoatType::Tug, length: h.params.length, beam: h.params.beam, crew: 0, passengers: 0 },
            hull_floors: None,
            rooms: None,
            superstructure: None,
            plan: Default::default(),
            finish: Default::default(),
            lighting: Default::default(),
        };
        let l = generate(&hb, &opt);
        let mut m = keel_model::Model::new_empty();
        let (_, a1, l1) = m.apply_build(0, &[], &l.build.parts, &l.build.links);
        let n1 = m.components().len();
        let rem: Vec<_> = l.build.parts.iter().map(|p| (p.def.clone(), p.vp, p.r)).collect();
        let (r2, a2, l2) = m.apply_build_replacing(0, &rem, &l.build.links, &l.build.parts, &l.build.links);
        assert_eq!((a1, l1), (a2, l2));
        assert_eq!(r2, a1);
        assert_eq!(m.components().len(), n1);
        let links = m.to_xml().matches("<logic_node_link ").count();
        assert_eq!(links, l1);
    }

    /// With the game's definitions (KEEL_DEFINITIONS), every generated wire must land on a port of its type.
    #[test]
    fn generated_wires_land_on_ports() {
        let Some(dir) = std::env::var_os("KEEL_DEFINITIONS") else { return };
        let cat = keel_model::catalogue::Catalogue::load_dir(std::path::Path::new(&dir)).unwrap();
        let h = HullParams::default().build().unwrap();
        let hb = keel_geom::hullblocks::build(&h, &Default::default());
        let half = (h.params.beam / 2.0 / VOXEL) as i32;
        let zl = (h.params.length / 2.0 / VOXEL) as i32;
        let opt = LayoutOptions {
            programme: programme::ProgrammeInput { boat: programme::BoatType::Workboat, length: h.params.length, beam: h.params.beam, crew: 0, passengers: 0 },
            hull_floors: None,
            rooms: None,
            superstructure: Some(superstructure::Superstructure::default_for(deck_row(&hb), half, -zl, zl, 2)),
            plan: Default::default(),
            finish: Default::default(),
            lighting: Default::default(),
        };
        let l = generate(&hb, &opt);
        let mut m = keel_model::Model::new_empty();
        m.apply_build(0, &[], &l.build.parts, &l.build.links);
        let bad = keel_model::diagnostics::wire_problems(&m, &cat);
        assert!(bad.is_empty(), "{} bad wire ends, e.g. {:?}", bad.len(), &bad[..bad.len().min(10)]);
    }
}

