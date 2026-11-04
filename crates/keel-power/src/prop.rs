//! Propellers (RE-0007): the propeller removes shaft speed each tick and turns it into thrust.
use crate::clamp;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PropDef {
    pub id: &'static str,
    pub name: &'static str,
    pub max_force: f64,
    pub rotation_speed: f64,
    pub blade_length: f64,
    pub blade_efficiency: f64,
    pub efficiency: f64,
    pub mass: f64,
}

/// The water propellers (definition values).
pub const PROPS: [PropDef; 3] = [
    PropDef { id: "propeller", name: "Small propeller", max_force: 20000.0, rotation_speed: 1.0, blade_length: 0.25, blade_efficiency: 0.075, efficiency: 1.0, mass: 2.0 },
    PropDef { id: "large_propeller", name: "Large propeller", max_force: 100000.0, rotation_speed: 1.0, blade_length: 0.5, blade_efficiency: 0.15, efficiency: 1.0, mass: 20.0 },
    PropDef { id: "giga_prop_small", name: "Giga propeller", max_force: 200000.0, rotation_speed: 0.5, blade_length: 1.0, blade_efficiency: 0.25, efficiency: 1.0, mass: 60.0 },
];

/// One tick: returns (new shaft rps, thrust N, absorbed rps). `blades` of 8 active (the water props are bladed).
pub fn tick(p: &PropDef, rps: f64, pitch: f64, v_axial: f64, submerged: bool) -> (f64, f64, f64) {
    let blades = 4.0;
    let rps = rps * 0.9999;
    let f_eff = p.max_force * blades / 8.0;
    let pitch = clamp(pitch, -1.0, 1.0);
    let bite = (2.0 * std::f64::consts::PI * rps * p.rotation_speed * p.blade_length * pitch * 0.75 + pitch * v_axial) * p.blade_efficiency * blades * 0.32;
    let mut lam = if f_eff > 0.0 { clamp(bite * 0.5 / (f_eff / 1000.0), -10.0, 10.0) } else { 0.0 };
    if submerged {
        lam *= 10.0;
    }
    let absorbed = (1.0 - 1.0 / (1.0 + lam * lam)) * rps;
    let thrust = absorbed * (f_eff / 1000.0) * 1500.0 * p.efficiency * if pitch >= 0.0 { 1.0 } else { -1.0 };
    (rps - absorbed, thrust, absorbed)
}

/// The torque-network weight a propeller adds (F_eff / 1000 with 4 blades).
pub fn net_weight(p: &PropDef) -> f64 {
    p.max_force * 0.5 / 1000.0
}
