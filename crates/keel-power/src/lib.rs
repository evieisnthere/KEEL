//! KEEL powertrain designer and calculators, using the recovered game models (60 Hz):
//!
//! * [`drag`]: steady water drag from the hull's exterior faces (RE-0003), and the in-game draft for a mass.
//! * [`prop`]: propeller "speed sink" law (RE-0007).
//! * [`engine`]: modular-engine cylinders, crank inertia, clutch, gearbox ratios (RE-0006/0008).
//! * [`electric`]: motors, generators and the battery pool (RE-0011).
//! * [`solve`]: steady operating point (shaft speed, thrust, boat speed) and range/endurance; design search.
//!
//! Inputs not fully identified in the game code are marked as assumptions where they enter.
pub mod drag;
pub mod electric;
pub mod engine;
pub mod prop;
pub mod solve;

pub const TICK: f64 = 1.0 / 60.0;

pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    x.max(lo).min(hi)
}
