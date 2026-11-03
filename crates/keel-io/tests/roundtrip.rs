use keel_io::{mesh, vehicle::Vehicle, xml};

const VEHICLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><vehicle data_version=\"3\" bodies_id=\"2\"><authors/>\
<bodies><body unique_id=\"1\"><components>\
<c><o r=\"1,0,0,0,1,0,0,0,1\" bc=\"FFFFFF\" ac=\"FFFFFF\" sc=\"6\"><vp x=\"1\"/></o></c>\
<c d=\"02_wedge\" t=\"2\"><o r=\"0,0,1,1,0,0,0,1,0\" bc=\"141414\" ac=\"141414\" sc=\"5\"><vp x=\"2\" y=\"-2\" z=\"-4\"/></o></c>\
</components></body></bodies><logic_node_links/></vehicle>\n";

const QUIRK: &str = "<definition name=\"X\">\n\t<voxels>\n\t\t<voxel flags=\"1\">\n\
\t\t\t<physics_shape_rotation 00=\"1\" 01=\"0\" 02=\"0\" 10=\"0\" 11=\"1\" 12=\"0\" 20=\"0\" 21=\"0\" 22=\"1\"/>\n\
\t\t</voxel>\n\t</voxels>\n</definition>\n";

#[test]
fn roundtrip_identical() {
    for s in [VEHICLE, QUIRK] {
        assert_eq!(xml::parse(s).unwrap().to_string_lossless(), s);
    }
}

#[test]
fn vehicle_view() {
    let v = Vehicle::from_document(xml::parse(VEHICLE).unwrap()).unwrap();
    let cs = v.components();
    assert_eq!(cs[0].definition(), "01_block");
    assert_eq!(cs[1].voxel(), [2, -2, -4]);
    assert_eq!(cs[1].rotation(), [0, 0, 1, 1, 0, 0, 0, 1, 0]);
}

#[test]
fn local_edit() {
    let mut d = xml::parse(VEHICLE).unwrap();
    let body = d.root.find_mut("bodies").unwrap().find_mut("body").unwrap();
    let comps = body.find_mut("components").unwrap();
    let c = comps.elements_mut().nth(1).unwrap();
    c.find_mut("o").unwrap().find_mut("vp").unwrap().remove_attr("y");
    assert_eq!(d.to_string_lossless(), VEHICLE.replace("<vp x=\"2\" y=\"-2\" z=\"-4\"/>", "<vp x=\"2\" z=\"-4\"/>"));
}

#[test]
fn malformed_reported() {
    assert!(xml::parse("<vehicle><bodies></vehicle>").is_err());
}

/// Opt-in: KEEL_CORPUS=/path/to/your/xml-and-mesh-files cargo test -- --ignored
#[test]
#[ignore]
fn corpus_roundtrip() {
    let dir = std::env::var("KEEL_CORPUS").expect("set KEEL_CORPUS");
    let mut stack = vec![std::path::PathBuf::from(dir)];
    let (mut nx, mut nm) = (0, 0);
    while let Some(p) = stack.pop() {
        for e in std::fs::read_dir(&p).unwrap() {
            let path = e.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().map_or(false, |x| x == "xml") {
                let s = std::fs::read_to_string(&path).unwrap();
                assert_eq!(xml::parse(&s).unwrap().to_string_lossless(), s, "{}", path.display());
                nx += 1;
            } else if path.extension().map_or(false, |x| x == "mesh") {
                mesh::parse(&std::fs::read(&path).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                nm += 1;
            }
        }
    }
    eprintln!("xml files: {nx}, meshes: {nm}");
    assert!(nx + nm > 0);
}
