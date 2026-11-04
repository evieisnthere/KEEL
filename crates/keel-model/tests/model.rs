use keel_model::{diagnostics, geometry, Catalogue, CompId, Model};

const SRC: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<vehicle data_version=\"3\" bodies_id=\"1\" custom=\"keep\">\n<bodies>\n<body unique_id=\"0\">\n<components>\n<c><o sc=\"6,x,FF0000,x,x,x,x\"><vp x=\"1\"/><mystery a=\"1\"/></o></c>\n<c><o><vp x=\"2\"/></o></c>\n</components>\n</body>\n</bodies>\n</vehicle>\n";

#[test]
fn untouched_roundtrip_is_identical() {
    assert_eq!(Model::parse(SRC).unwrap().to_xml(), SRC);
}

#[test]
fn translate_keeps_unknown_data_and_undoes() {
    let mut m = Model::parse(SRC).unwrap();
    m.translate(&[CompId { body: 0, comp: 0 }], [0, 1, 0]);
    let s = m.to_xml();
    assert!(s.contains("<mystery a=\"1\"/>") && s.contains("custom=\"keep\""));
    assert_eq!(m.components()[0].1.voxel, [1, 1, 0]);
    assert!(m.undo());
    assert_eq!(m.to_xml(), SRC);
    assert!(m.redo());
    assert_eq!(m.components()[0].1.voxel, [1, 1, 0]);
}

#[test]
fn rotate_four_times_is_identity() {
    let mut m = Model::parse(SRC).unwrap();
    let ids = [CompId { body: 0, comp: 0 }];
    for _ in 0..4 {
        m.rotate_quarter(&ids, 1, [0, 0, 0]);
    }
    let c = &m.components()[0].1;
    assert_eq!(c.voxel, [1, 0, 0]);
    assert_eq!(c.rotation, keel_io::vehicle::IDENTITY);
    m.rotate_quarter(&ids, 1, [0, 0, 0]);
    assert_eq!(m.components()[0].1.voxel, [0, 0, -1]);
}

#[test]
fn delete_duplicate_add() {
    let mut m = Model::parse(SRC).unwrap();
    let new = m.duplicate(&[CompId { body: 0, comp: 1 }], [0, 0, 1]);
    assert_eq!(new, vec![CompId { body: 0, comp: 2 }]);
    assert_eq!(m.components()[2].1.voxel, [2, 0, 1]);
    assert_eq!(m.delete(&[CompId { body: 0, comp: 0 }]), 1);
    assert_eq!(m.components().len(), 2);
    let a = m.add(0, "02_wedge", [5, 0, 0], keel_io::vehicle::IDENTITY).unwrap();
    assert_eq!(m.components()[a.comp].1.def, "02_wedge");
    let reparsed = Model::parse(&m.to_xml()).unwrap();
    assert_eq!(reparsed.components().len(), 3);
}

#[test]
fn faces_cull_internal_walls() {
    let m = Model::parse(SRC).unwrap();
    let cat = Catalogue::fallback();
    let faces = geometry::build_faces(&m, &cat);
    // two adjacent unit blocks: 12 faces minus the 2 shared ones (a full face may be emitted as 1 polygon)
    assert_eq!(faces.len(), 10, "faces: {}", faces.len());
    assert!(faces.iter().any(|f| f.comp.comp == 0 && f.colour == [255, 0, 0, 255]));
}

#[test]
fn diagnostics_find_overlap() {
    let mut m = Model::parse(SRC).unwrap();
    m.translate(&[CompId { body: 0, comp: 1 }], [-1, 0, 0]);
    let d = diagnostics::check(&m, &Catalogue::fallback());
    assert!(d.iter().any(|x| x.code == "voxel-overlap" && x.components.len() == 2));
}

#[test]
fn scaled_xml_edit_renders_scaled_and_roundtrips() {
    let src = "<vehicle data_version=\"3\"><bodies><body><components><c><o r=\"1,0,0,0,5,0,0,0,1\"><vp/></o></c><c><o r=\"0,0,0,0,0,0,0,0,0\"><vp x=\"3\"/></o></c></components></body></bodies></vehicle>";
    let m = Model::parse(src).unwrap();
    assert_eq!(m.to_xml(), src);
    let faces = geometry::build_faces(&m, &Catalogue::fallback());
    let ymax = faces.iter().flat_map(|f| f.verts.iter().map(|v| v[1])).fold(f32::MIN, f32::max);
    assert!((ymax - 0.625).abs() < 1e-6, "block stretched 5x in Y: {ymax}");
    assert!(faces.iter().all(|f| f.comp.comp == 0), "zero-size block draws nothing");
    let d = diagnostics::check(&m, &Catalogue::fallback());
    assert!(d.iter().any(|x| x.code == "xml-edited-transform" && x.severity == diagnostics::Severity::Info));
}

#[test]
fn user_scale_matches_workshop_form() {
    let mut m = Model::parse(SRC).unwrap();
    let id = [CompId { body: 0, comp: 1 }];
    m.set_scale(&id, [1, 5, 1]);
    assert!(m.to_xml().contains("r=\"1,0,0,0,5,0,0,0,1\""));
    m.rotate_quarter(&id, 0, [2, 0, 0]);
    assert_eq!(Model::scale_of(&m.components()[1].1.rotation), [1, 5, 1]);
    m.set_scale(&id, [1, 1, 1]);
    assert_eq!(Model::scale_of(&m.components()[1].1.rotation), [1, 1, 1]);
    m.set_scale(&id, [0, 0, 0]);
    m.set_scale(&id, [2, 2, 2]);
    assert_eq!(Model::scale_of(&m.components()[1].1.rotation), [2, 2, 2]);
}

#[test]
fn block_solids_match_game_hulls() {
    use keel_model::geometry::{block_points, convex_hull_faces};
    let count = |bt| convex_hull_faces(&block_points(bt).unwrap()).len();
    assert_eq!(count(0), 6); // cube
    assert_eq!(count(1), 5); // wedge
    assert_eq!(count(2), 4); // corner pyramid
    assert_eq!(count(3), 7); // inverse pyramid: cube minus a corner
    assert_eq!(count(7), 5); // wedge 1x4
    // wedge: top edge at +z, slope falling to -z (0x14031ab90 hull points)
    let w = block_points(1).unwrap();
    assert!(w.contains(&[0.125, 0.125, 0.125]) && !w.contains(&[0.125, 0.125, -0.125]));
}
