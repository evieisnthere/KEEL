//! Modular engine (RE-0006): cylinders, crank inertia, fuel use, and the modular gearbox ratios.
use crate::clamp;
use serde::{Deserialize, Serialize};

pub const REV_LIMIT: f64 = 60.0;

/// Modular gearbox ratios by the in-game setting (output / input).
pub const GEARS: [(f64, &str); 8] = [(-1.0, "1:-1 (reverse)"), (1.0, "1:1"), (5.0 / 6.0, "6:5"), (2.0 / 3.0, "3:2"), (5.0 / 9.0, "9:5"), (0.5, "2:1"), (0.4, "5:2"), (1.0 / 3.0, "3:1")];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cylinder {
    Small,
    Medium,
    Large,
}

impl Cylinder {
    pub const ALL: [Cylinder; 3] = [Cylinder::Small, Cylinder::Medium, Cylinder::Large];
    pub fn name(self) -> &'static str {
        match self {
            Cylinder::Small => "1×1 cylinder",
            Cylinder::Medium => "3×3 piston",
            Cylinder::Large => "5×5 piston",
        }
    }
    /// Cylinder size V_c. Assumption (as in the reference model): the definition's voxel count.
    pub fn size(self) -> f64 {
        match self {
            Cylinder::Small => 7.0,
            Cylinder::Medium => 33.0,
            Cylinder::Large => 131.0,
        }
    }
    /// Crank inertia contribution per cylinder (its crankshaft) and the matching flywheel, and masses.
    pub fn crank(self) -> f64 {
        match self {
            Cylinder::Small => 0.7,
            Cylinder::Medium => 15.0,
            Cylinder::Large => 25.0,
        }
    }
    pub fn flywheel(self) -> f64 {
        match self {
            Cylinder::Small => 150.0,
            Cylinder::Medium => 600.0,
            Cylinder::Large => 2400.0,
        }
    }
    /// Mass of one cylinder + its crankshaft section, and of the flywheel.
    pub fn masses(self) -> (f64, f64) {
        match self {
            Cylinder::Small => (2.0, 100.0),
            Cylinder::Medium => (54.0, 200.0),
            Cylinder::Large => (200.0, 300.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSpec {
    pub cylinder: Cylinder,
    pub count: usize,
    pub flywheel: bool,
}

impl EngineSpec {
    pub fn inertia(&self) -> f64 {
        self.cylinder.crank() * self.count as f64 + if self.flywheel { self.cylinder.flywheel() } else { 0.0 }
    }
    pub fn mass(&self) -> f64 {
        let (c, f) = self.cylinder.masses();
        c * self.count as f64 + if self.flywheel { f } else { 0.0 }
    }
    pub fn describe(&self) -> String {
        format!("{} × {}{}", self.count, self.cylinder.name(), if self.flywheel { " + flywheel" } else { "" })
    }
}

/// Engine efficiency against crank speed: 1 at 0 rps, 0.5 at 50, 0 at ≈ 121.
pub fn rps_efficiency(rps: f64) -> f64 {
    let x = 0.02 * rps;
    clamp(4.0 * (x + 0.5) / ((x + 1.0) * (x + 1.0)) - 1.0, 0.0, 1.0)
}

/// One tick of all cylinders at crank speed `rps`: (Δrps, fuel used per tick, efficiency). Air and fuel are assumed
/// unlimited, the engine warm (90 °C) and the fuel valve set for the best mixture (stoichiometry 0.6).
pub fn tick(e: &EngineSpec, rps: f64, throttle: f64) -> (f64, f64, f64) {
    if rps <= 2.0 || e.count == 0 {
        return (0.0, 0.0, 0.0);
    }
    let th = if rps > REV_LIMIT { 0.0 } else { clamp(throttle, 0.0, 1.0) };
    let vc = e.cylinder.size();
    let air = rps * 4e-6 * vc * th;
    // best mixture: stoich = 0.6 inside the warm window (rich 19.6, lean 10.2 at 90 °C)
    let t = 0.9;
    let (rich, lean) = (16.0 + 4.0 * t, 12.0 - 2.0 * t);
    let afr = rich - 0.6 * (rich - lean);
    let fuel = afr * air / 600.0;
    let eff = rps_efficiency(rps);
    let energy = air;
    let d = 80_000.0 * energy * eff / e.inertia().max(1e-6) * e.count as f64;
    (d, fuel * e.count as f64, eff)
}
