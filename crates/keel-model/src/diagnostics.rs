//! Validation and diagnostics framework (plan phase 1). Each check is cheap and returns located findings.
use crate::catalogue::Catalogue;
use crate::edit::{CompId, Model};
use crate::geometry::occupied_voxels;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub components: Vec<CompId>,
}

pub fn check(model: &Model, cat: &Catalogue) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let comps = model.components();
    let mut unknown: HashMap<String, Vec<CompId>> = HashMap::new();
    let mut cells: HashMap<(usize, [i32; 3]), Vec<CompId>> = HashMap::new();
    let mut bad_rot = Vec::new();
    for (id, c) in &comps {
        let def = cat.get(&c.def);
        if def.is_none() {
            unknown.entry(c.def.clone()).or_default().push(*id);
        }
        let r = c.rotation;
        let det = r[0] * (r[4] * r[8] - r[5] * r[7]) - r[1] * (r[3] * r[8] - r[5] * r[6]) + r[2] * (r[3] * r[7] - r[4] * r[6]);
        if det != 1 || r.iter().any(|x| x.abs() > 1) {
            bad_rot.push(*id);
            continue; // scaled / zero-size transforms (XML edits) have no well-defined voxel footprint
        }
        for v in occupied_voxels(def, c.voxel, &r) {
            cells.entry((id.body, v)).or_default().push(*id);
        }
    }
    for (d, ids) in unknown {
        out.push(Diagnostic {
            severity: Severity::Warning,
            code: "unknown-definition",
            message: format!("{} component(s) use definition '{}' not found in the catalogue", ids.len(), d),
            components: ids,
        });
    }
    let mut overlaps: Vec<CompId> = cells.values().filter(|v| v.len() > 1).flatten().copied().collect();
    overlaps.sort();
    overlaps.dedup();
    if !overlaps.is_empty() {
        out.push(Diagnostic {
            severity: Severity::Info,
            code: "voxel-overlap",
            message: format!(
                "{} component(s) share voxels with another component (usually an XML merge edit for styling; the in-game editor cannot place these)",
                overlaps.len()
            ),
            components: overlaps,
        });
    }
    if !bad_rot.is_empty() {
        out.push(Diagnostic {
            severity: Severity::Info,
            code: "xml-edited-transform",
            message: format!(
                "{} component(s) use an XML scale edit (scaled, mirrored or zero-size transform, e.g. styled railings, windows, wedges); rendered scaled and preserved exactly",
                bad_rot.len()
            ),
            components: bad_rot,
        });
    }
    if comps.is_empty() {
        out.push(Diagnostic { severity: Severity::Info, code: "empty", message: "the vehicle has no components".into(), components: vec![] });
    }
    out.push(Diagnostic {
        severity: Severity::Info,
        code: "waterline-estimate",
        message: "Draft and waterline predictions are estimates (docs/OPEN-ITEMS.md A1)".into(),
        components: vec![],
    });
    out.sort_by(|a, b| b.severity.cmp(&a.severity));
    out
}
