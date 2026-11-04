//! Electric drive (RE-0011): motors, generators, alternators and batteries.
use crate::clamp;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MotorDef {
    pub id: &'static str,
    pub name: &'static str,
    pub max_force: f64,
    pub magnitude: f64,
    pub mass: f64,
}

pub const MOTORS: [MotorDef; 3] = [
    MotorDef { id: "motor_small", name: "Small motor", max_force: 2.0, magnitude: 1.2, mass: 1.0 },
    MotorDef { id: "motor_medium", name: "Medium motor", max_force: 60.0, magnitude: 1.1, mass: 8.0 },
    MotorDef { id: "motor_large", name: "Large motor", max_force: 250.0, magnitude: 1.05, mass: 27.0 },
];

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BatteryDef {
    pub id: &'static str,
    pub name: &'static str,
    pub capacity: f64,
    pub mass: f64,
}

pub const BATTERIES: [BatteryDef; 3] = [
    BatteryDef { id: "battery_small", name: "Small battery", capacity: 1600.0, mass: 10.0 },
    BatteryDef { id: "battery_medium", name: "Medium battery", capacity: 12800.0, mass: 60.0 },
    BatteryDef { id: "battery_large", name: "Large battery", capacity: 256000.0, mass: 600.0 },
];

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GeneratorDef {
    pub id: &'static str,
    pub name: &'static str,
    pub max_force: f64,
    pub magnitude: f64,
}

pub const GENERATORS: [GeneratorDef; 4] = [
    GeneratorDef { id: "generator_small", name: "Small generator", max_force: 5.0, magnitude: 0.95 },
    GeneratorDef { id: "generator_medium", name: "Medium generator", max_force: 25.0, magnitude: 0.9 },
    GeneratorDef { id: "generator_large", name: "Large generator", max_force: 125.0, magnitude: 0.85 },
    GeneratorDef { id: "modular_engine_alternator", name: "Alternator (modular)", max_force: 100.0, magnitude: 0.05 },
];

/// One motor tick at shaft speed ω with throttle t, given the delivered fraction of demand `f` (1 with charge):
/// returns (new ω, demand per tick).
pub fn motor_tick(m: &MotorDef, omega: f64, throttle: f64, f: f64) -> (f64, f64) {
    let t = clamp(throttle, -1.0, 1.0);
    let target = if t >= 0.0 { 20.0 } else { -20.0 };
    let a = t.abs();
    let mut d = clamp((target - omega) * a, -2.0 * a, 2.0 * a);
    if (t < 0.0 && d >= 0.0) || (t >= 0.0 && d <= 0.0) {
        d = 0.0;
    }
    let demand = d.abs() * m.magnitude * m.max_force * 0.1;
    ((omega + f * d) * 0.995, demand)
}

/// Generator output per tick at shaft speed ω, and the shaft speed it leaves.
pub fn generator_tick(g: &GeneratorDef, omega: f64) -> (f64, f64) {
    let w = omega.abs();
    let k = (w / 560.0).min(0.25);
    (g.magnitude * g.max_force * 0.1 * w * k, (1.0 - k) * omega)
}

/// Battery bank: usable charge (units). The 10-unit pool window is negligible for the bank sizes boats use.
pub fn bank_capacity(b: &BatteryDef, count: usize) -> f64 {
    b.capacity * count as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_generator_follows_the_recovered_law() {
        // produced = magnitude · max_force · 0.1 · |ω| · min(0.25, |ω|/560): 0.85 · 125 · 0.1 · 10 · 10/560 = 1.897
        // (RE-0011's worked example says 0.19, a factor of 10 off the formula; logged in OPEN-ITEMS)
        let (p, _) = generator_tick(&GENERATORS[2], 10.0);
        assert!((p - 1.897).abs() < 0.005, "{p}");
    }

    #[test]
    fn motor_spins_up_toward_20_rps() {
        let mut w = 0.0;
        for _ in 0..600 {
            w = motor_tick(&MOTORS[1], w, 1.0, 1.0).0;
        }
        assert!(w > 15.0 && w < 20.0, "{w}");
    }
}
