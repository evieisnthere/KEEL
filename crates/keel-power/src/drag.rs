//! Steady water drag (RE-0003) from exterior hull faces, and the in-game draft for a mass (buoyancy divisor).
use crate::clamp;
use serde::{Deserialize, Serialize};

/// Game constants used by the water kernel (compiled defaults, RE-0001).
pub mod k {
    pub const WATER_DENSITY: f64 = 1000.0;
    pub const DRAG_LIN_P: f64 = 15.0;
    pub const DRAG_QUAD_P: f64 = 17.0;
    pub const DRAG_LIN_S: f64 = 12.0;
    pub const DRAG_QUAD_S: f64 = 5.0;
    pub const FALLOFF_P: f64 = 2.5;
    pub const FALLOFF_S: f64 = 0.3;
    pub const WATER_DRAG_FACTOR: f64 = 1.0;
    pub const WATER_DRAG_WEIGHT_MULT: f64 = 4.0;
    pub const VISCOUS: f64 = 0.5;
    pub const BUOY_MULT: f64 = 10.0;
    pub const BUOY_MOD: f64 = 10.0;
    pub const BUOY_LOWER: f64 = 2000.0;
    pub const BUOY_UPPER: f64 = 23000.0;
}

/// Skin-friction multiplier for a vehicle mass (1 … 5).
pub fn water_drag_scale(mass: f64) -> f64 {
    k::WATER_DRAG_FACTOR + clamp((mass - 2000.0) / 23000.0, 0.0, 1.0) * k::WATER_DRAG_WEIGHT_MULT
}

/// Buoyancy divisor k_buoy (10 … 20): the game floats a vehicle on V = M·k/ρ.
pub fn buoyancy_divisor(mass: f64) -> f64 {
    k::BUOY_MULT + clamp((mass - k::BUOY_LOWER) / k::BUOY_UPPER, 0.0, 1.0) * k::BUOY_MOD
}

/// An exterior face: centroid height (m above the baseline), outward unit normal (x, y, z; +z forward) and area (m²).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Face {
    pub y: f64,
    pub n: [f64; 3],
    pub area: f64,
}

#[derive(Debug, Clone, Default)]
pub struct DragModel {
    pub faces: Vec<Face>,
}

impl DragModel {
    /// Steady forward drag (N) at `speed` (m/s, +z) with the waterline at `waterline` (m), for a vehicle of `mass`.
    /// The per-tick impulse cap is not applied (steady state). The viscous length L is unidentified: at boat speeds
    /// its bracket tends to 1.5 (assumption: L = 1).
    pub fn drag(&self, speed: f64, waterline: f64, mass: f64) -> f64 {
        if speed <= 0.0 {
            return 0.0;
        }
        let s = speed;
        let ks = water_drag_scale(mass);
        let mut f = 0.0;
        for face in self.faces.iter().filter(|f| f.y < waterline) {
            let a = face.area;
            f += k::VISCOUS * k::WATER_DENSITY * (20.0 / (s * 1e6 + 10.0) + 1.5) * 0.00075 * a * ks * s * s;
            let c = face.n[2];
            if c > 0.0 {
                f += c * c.powf(k::FALLOFF_P) * (k::DRAG_LIN_P * s + k::DRAG_QUAD_P * s * s) * a;
            } else if c < 0.0 {
                f += (-c) * (-c).powf(k::FALLOFF_S) * (k::DRAG_LIN_S * s + k::DRAG_QUAD_S * s * s) * a;
            }
        }
        f
    }

    /// Wetted area below a waterline (m²).
    pub fn wetted(&self, waterline: f64) -> f64 {
        self.faces.iter().filter(|f| f.y < waterline).map(|f| f.area).sum()
    }
}

/// In-game waterline for a mass: the height where the submerged volume (from `volume_below`, m³) reaches M·k_buoy/ρ.
/// Returns None when the hull cannot float the mass.
pub fn game_waterline(mass: f64, volume_below: &dyn Fn(f64) -> f64, lo: f64, hi: f64) -> Option<f64> {
    let need = mass * buoyancy_divisor(mass) / k::WATER_DENSITY;
    if volume_below(hi) < need {
        return None;
    }
    let (mut a, mut b) = (lo, hi);
    for _ in 0..60 {
        let m = 0.5 * (a + b);
        if volume_below(m) < need {
            a = m;
        } else {
            b = m;
        }
    }
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_plate_facing_the_flow_drags_like_the_pressure_law() {
        let m = DragModel { faces: vec![Face { y: -1.0, n: [0.0, 0.0, 1.0], area: 1.0 }] };
        let d = m.drag(2.0, 0.0, 1000.0);
        let visc = 0.5 * 1000.0 * (20.0 / (2e6 + 10.0) + 1.5) * 0.00075 * 4.0;
        assert!((d - (15.0 * 2.0 + 17.0 * 4.0 + visc)).abs() < 1e-6, "{d}");
        assert_eq!(m.drag(2.0, -2.0, 1000.0), 0.0);
    }

    #[test]
    fn heavier_boats_float_on_more_volume() {
        assert_eq!(buoyancy_divisor(1000.0), 10.0);
        assert_eq!(buoyancy_divisor(30000.0), 20.0);
        let vol = |wl: f64| wl.max(0.0) * 10.0; // box of 10 m² waterplane
        let w = game_waterline(1000.0, &vol, 0.0, 5.0).unwrap();
        assert!((w - 1.0).abs() < 1e-6);
    }
}
