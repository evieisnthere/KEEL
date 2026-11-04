//! Generate the interior for a hull + boat type and dump painted block faces for scripts/hullshot.py (`--paint`).
//! cargo run --release -p keel-layout --example layoutdump -- out.json <boat type index> [params.json] [tiers]
use keel_geom::voxel::VOXEL;
use keel_layout::*;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let boat = programme::BoatType::ALL[a.get(2).and_then(|s| s.parse().ok()).unwrap_or(2)];
    let params: keel_geom::hull::HullParams = match a.get(3).filter(|s| *s != "-") {
        Some(p) => serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap(),
        None => Default::default(),
    };
    let tiers: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(2);
    let h = params.build().unwrap();
    let hb = keel_geom::hullblocks::build(&h, &Default::default());
    let half = (h.params.beam / 2.0 / VOXEL) as i32;
    let zl = (h.params.length / 2.0 / VOXEL) as i32;
    let s = superstructure::Superstructure::default_for(deck_row(&hb), half, -zl, zl, tiers);
    let opt = LayoutOptions {
        programme: programme::ProgrammeInput { boat, length: h.params.length, beam: h.params.beam, crew: 0, passengers: 0 },
        hull_floors: None,
        superstructure: Some(s),
        plan: Default::default(),
        finish: Default::default(),
        lighting: Default::default(),
    };
    let l = generate(&hb, &opt);
    eprintln!("{}: {} rooms, unplaced {:?}, {} parts, {} links", boat.name(), l.plan.rooms.len(), l.plan.unplaced, l.build.parts.len(), l.build.links.len());
    for r in &l.plan.rooms {
        eprintln!("  {:18} {:12} {:5.1} m²", r.kind.name(), l.plan.decks[r.deck].name(), r.area_m2());
    }
    let mut faces = vec![];
    let mut emit = |def: &str, vp: [i32; 3], r: [i32; 9], colour: &str| {
        let Some(&(bt, _)) = keel_geom::lattice::ALL_BTS.iter().find(|(_, d)| *d == def) else {
            // non-block parts: a small marker cube so they show up
            let c = match def {
                d if d.contains("light") => "FFE08A",
                d if d.contains("button") => "FF4040",
                d if d.contains("gate") => "40FF80",
                d if d.contains("door") => "6A4A2A",
                d if d.contains("window") => "88CCFF",
                d if d.contains("seat") => "C04080",
                d if d.contains("ladder") || d.contains("stair") => "FFA500",
                _ => "FF00FF",
            };
            let pts: Vec<[f64; 3]> = (0..8).map(|k| [vp[0] as f64 * VOXEL + if k & 1 == 1 { 0.1 } else { -0.1 }, (vp[1] as f64 + 0.5) * VOXEL + if k & 2 == 2 { 0.1 } else { -0.1 }, vp[2] as f64 * VOXEL + if k & 4 == 4 { 0.1 } else { -0.1 }]).collect();
            for (n, f) in keel_model::geometry::convex_hull_faces(&pts) {
                faces.push(serde_json::json!({"p": f, "n": n, "s": false, "d": def, "c": c}));
            }
            return;
        };
        let pts: Vec<[f64; 3]> = keel_model::geometry::block_points(bt)
            .unwrap()
            .into_iter()
            .map(|p| {
                let q = [0, 1, 2].map(|i| r[i] as f64 * p[0] + r[3 + i] as f64 * p[1] + r[6 + i] as f64 * p[2]);
                [q[0] + vp[0] as f64 * VOXEL, q[1] + (vp[1] as f64 + 0.5) * VOXEL, q[2] + vp[2] as f64 * VOXEL]
            })
            .collect();
        for (n, f) in keel_model::geometry::convex_hull_faces(&pts) {
            faces.push(serde_json::json!({"p": f, "n": n, "s": false, "d": def, "c": colour}));
        }
    };
    for p in &hb.placements {
        emit(&p.def, p.vp, p.r, "B8B8B0");
    }
    for p in &l.build.parts {
        emit(&p.def, p.vp, p.r, p.bc.as_deref().or(p.ac.as_deref()).unwrap_or("CCCCCC"));
    }
    std::fs::write(&a[1], serde_json::to_string(&serde_json::json!({"faces": faces, "summary": format!("{} — {} rooms", boat.name(), l.plan.rooms.len())})).unwrap()).unwrap();
}
