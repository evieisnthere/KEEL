//! Steady operating points and design search.
//!
//! One propulsion line is: engine (or motors) → clutch → network A → gearbox (ratio g) → network B → propeller. The
//! torque-network step is the reference model's (RE-0008): slots add `weight · rps` to an accumulator and each
//! network relaxes to `(rps + Σ) / (1 + Σweights)`. Lines are identical and run in parallel; the boat accelerates
//! with their summed thrust against the hull drag until it settles.
use crate::drag::DragModel;
use crate::electric::{motor_tick, BatteryDef, MotorDef};
use crate::engine::{self, EngineSpec, GEARS};
use crate::prop::{self, PropDef};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub enum Drive {
    Engine(EngineSpec),
    Electric { motor: MotorDef, motors: usize },
}

#[derive(Debug, Clone, Serialize)]
pub struct Line {
    pub drive: Drive,
    /// gearbox ratio (output / input; 1 = direct)
    pub ratio: f64,
    pub prop: PropDef,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Boat {
    /// mass in definition units (≈ kg)
    pub mass: f64,
    /// in-game waterline height (m above the baseline)
    pub waterline: f64,
    /// thrust calibration factor (1 = the recovered law as is; set from an in-game top-speed test)
    pub thrust_scale: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Operating {
    pub speed: f64,
    pub engine_rps: f64,
    pub shaft_rps: f64,
    pub thrust: f64,
    pub drag: f64,
    pub efficiency: f64,
    /// fuel per second (game fuel units), engines only
    pub fuel_per_s: f64,
    /// electric demand per second (charge units), motors only
    pub power_per_s: f64,
    pub converged: bool,
}

impl Operating {
    pub fn knots(&self) -> f64 {
        self.speed * 1.943_844
    }
    /// Endurance (s) on `fuel` units or `charge` units.
    pub fn endurance(&self, fuel: f64, charge: f64) -> Option<f64> {
        if self.fuel_per_s > 0.0 {
            Some(fuel / self.fuel_per_s)
        } else if self.power_per_s > 0.0 {
            Some(charge / self.power_per_s)
        } else {
            None
        }
    }
}

/// The drivetrain at a fixed boat speed `v`: run the shaft until it settles and average the last second.
/// Returns the operating point with `speed = v` and `thrust` = mean total thrust (calibrated).
pub fn at_speed(line: &Line, lines: usize, throttle: f64, v: f64, boat: &Boat) -> Operating {
    let (mut we, mut wa, mut wb) = (5.0f64, 0.0f64, 0.0f64);
    let g = if line.ratio.abs() < 1e-6 { 1.0 } else { line.ratio };
    let n = lines.max(1) as f64;
    let mut out = Operating { speed: v, converged: true, ..Default::default() };
    let (settle, avg) = (1800usize, 60usize);
    let (mut t_sum, mut f_sum, mut p_sum, mut e_sum, mut wa_sum, mut wb_sum) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for t in 0..settle + avg {
        let (mut fuel, mut demand, mut eff) = (0.0, 0.0, 0.0);
        let mut acc_a = (0.0, 0.0);
        let mut acc_b = (0.0, 0.0);
        match &line.drive {
            Drive::Engine(e) => {
                let (d, f, ef) = engine::tick(e, we, throttle);
                we += d;
                fuel = f;
                eff = ef;
                let i = e.inertia();
                acc_a.0 += i * we;
                acc_a.1 += i;
            }
            Drive::Electric { motor, motors } => {
                let (motor_w, dm) = motor_tick(motor, wa, throttle, 1.0);
                demand = dm * *motors as f64;
                let m = motor.max_force * *motors as f64;
                acc_a.0 += m * motor_w;
                acc_a.1 += m;
            }
        }
        let (newr, thrust, _) = prop::tick(&line.prop, wb, 1.0, -v, true);
        let m = prop::net_weight(&line.prop);
        acc_b.0 += m * newr;
        acc_b.1 += m;
        // gearbox between the networks: each pulls the other toward ratio × its speed with weight W · 0.95 / |r|
        let f = 0.95;
        let (wta, wtb) = (1.0 + acc_a.1, 1.0 + acc_b.1);
        acc_b.0 += g * wa * wta * f / g.abs();
        acc_b.1 += wta * f / g.abs();
        acc_a.0 += wb / g * wtb * f * g.abs();
        acc_a.1 += wtb * f * g.abs();
        wa = (wa + acc_a.0) / (1.0 + acc_a.1);
        wb = (wb + acc_b.0) / (1.0 + acc_b.1);
        if let Drive::Engine(_) = line.drive {
            we += wa - we;
        }
        if t >= settle {
            t_sum += thrust;
            f_sum += fuel;
            p_sum += demand;
            e_sum += eff;
            wa_sum += if let Drive::Engine(_) = line.drive { we } else { wa };
            wb_sum += wb;
        }
    }
    let k = avg as f64;
    out.thrust = t_sum / k * n * boat.thrust_scale;
    out.fuel_per_s = f_sum / k * 60.0 * n;
    out.power_per_s = p_sum / k * 60.0 * n;
    out.efficiency = e_sum / k;
    out.engine_rps = wa_sum / k;
    out.shaft_rps = wb_sum / k;
    out
}

/// Steady top speed: the speed where the calibrated thrust equals the hull drag (bisection on 0 … 80 m/s).
pub fn operate(line: &Line, lines: usize, throttle: f64, hull: &DragModel, boat: &Boat, _max_s: f64) -> Operating {
    let excess = |v: f64| -> (f64, Operating) {
        let op = at_speed(line, lines, throttle, v, boat);
        (op.thrust - hull.drag(v, boat.waterline, boat.mass), op)
    };
    let (mut lo, mut hi) = (0.0f64, 80.0f64);
    let (e_hi, op_hi) = excess(hi);
    if e_hi > 0.0 {
        let mut op = op_hi;
        op.drag = hull.drag(hi, boat.waterline, boat.mass);
        op.converged = false;
        return op;
    }
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if excess(mid).0 > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (_, mut op) = excess(lo);
    op.drag = hull.drag(lo, boat.waterline, boat.mass);
    op
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub line: Line,
    pub lines: usize,
    pub op: Operating,
    pub mass: f64,
    pub describe: String,
}

/// Search engine lines (cylinder type × count × flywheel × gear × propeller × 1–2 lines) for `target` m/s at full
/// throttle. Returns candidates that reach it, lowest fuel use first (then lightest).
pub fn search_engine(target: f64, hull: &DragModel, boat: &Boat, max_count: usize) -> Vec<Candidate> {
    let mut out = vec![];
    for cyl in engine::Cylinder::ALL {
        for count in 1..=max_count {
            for flywheel in [false, true] {
                let e = EngineSpec { cylinder: cyl, count, flywheel };
                for &(ratio, gname) in GEARS.iter().skip(1) {
                    for p in prop::PROPS {
                        for lines in 1..=2 {
                            let line = Line { drive: Drive::Engine(e.clone()), ratio, prop: p };
                            let op = operate(&line, lines, 1.0, hull, &Boat { mass: boat.mass + (e.mass() + p.mass) * lines as f64, ..*boat }, 90.0);
                            if op.speed >= target {
                                let mass = (e.mass() + p.mass) * lines as f64;
                                let describe = format!("{} line{}: {} · gearbox {} · {}", lines, if lines > 1 { "s" } else { "" }, e.describe(), gname, p.name);
                                out.push(Candidate { line, lines, op, mass, describe });
                            }
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.op.fuel_per_s.partial_cmp(&b.op.fuel_per_s).unwrap().then(a.mass.partial_cmp(&b.mass).unwrap()));
    out
}

/// Search electric lines (motor type × count × gear × propeller × 1–2 lines) for `target` m/s; lowest power first.
pub fn search_electric(target: f64, hull: &DragModel, boat: &Boat, max_motors: usize) -> Vec<Candidate> {
    let mut out = vec![];
    for motor in crate::electric::MOTORS {
        for motors in 1..=max_motors {
            for &(ratio, gname) in GEARS.iter().skip(1) {
                for p in prop::PROPS {
                    for lines in 1..=2 {
                        let line = Line { drive: Drive::Electric { motor, motors }, ratio, prop: p };
                        let extra = (motor.mass * motors as f64 + p.mass) * lines as f64;
                        let op = operate(&line, lines, 1.0, hull, &Boat { mass: boat.mass + extra, ..*boat }, 90.0);
                        if op.speed >= target {
                            let describe = format!("{} line{}: {} × {} · gearbox {} · {}", lines, if lines > 1 { "s" } else { "" }, motors, motor.name, gname, p.name);
                            out.push(Candidate { line, lines, op, mass: extra, describe });
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.op.power_per_s.partial_cmp(&b.op.power_per_s).unwrap().then(a.mass.partial_cmp(&b.mass).unwrap()));
    out
}

/// Batteries needed for `hours` at `power_per_s`.
pub fn batteries_for(power_per_s: f64, hours: f64, b: &BatteryDef) -> usize {
    ((power_per_s * hours * 3600.0) / b.capacity).ceil().max(1.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drag::Face;

    fn box_hull() -> DragModel {
        // a 10 m × 3 m × 1 m deep box: bow and stern faces, bottom, sides
        DragModel {
            faces: vec![
                Face { y: 0.5, n: [0.0, 0.0, 1.0], area: 3.0 },
                Face { y: 0.5, n: [0.0, 0.0, -1.0], area: 3.0 },
                Face { y: 0.0, n: [0.0, -1.0, 0.0], area: 30.0 },
                Face { y: 0.5, n: [1.0, 0.0, 0.0], area: 10.0 },
                Face { y: 0.5, n: [-1.0, 0.0, 0.0], area: 10.0 },
            ],
        }
    }

    #[test]
    fn more_engine_goes_faster() {
        let hull = box_hull();
        // uncalibrated thrust saturates the 80 m/s search range; a small factor keeps the comparison meaningful
        let boat = Boat { mass: 5000.0, waterline: 1.0, thrust_scale: 0.01 };
        let line = |n| Line { drive: Drive::Engine(EngineSpec { cylinder: engine::Cylinder::Medium, count: n, flywheel: true }), ratio: 1.0, prop: prop::PROPS[1] };
        let a = operate(&line(2), 1, 1.0, &hull, &boat, 120.0);
        let b = operate(&line(6), 1, 1.0, &hull, &boat, 120.0);
        eprintln!("2 cyl: {:.2} m/s rps {:.1}/{:.1} fuel {:.4}; 6 cyl: {:.2} m/s", a.speed, a.engine_rps, a.shaft_rps, a.fuel_per_s, b.speed);
        assert!(a.speed > 0.5, "{a:?}");
        assert!(b.speed > a.speed);
        assert!(a.engine_rps <= engine::REV_LIMIT + 2.0);
    }

    #[test]
    fn electric_line_moves_the_boat_and_draws_power() {
        let hull = box_hull();
        let boat = Boat { mass: 3000.0, waterline: 1.0, thrust_scale: 0.01 };
        let line = Line { drive: Drive::Electric { motor: crate::electric::MOTORS[2], motors: 2 }, ratio: 1.0, prop: prop::PROPS[0] };
        let op = operate(&line, 1, 1.0, &hull, &boat, 120.0);
        eprintln!("electric: {:.2} m/s shaft {:.1} power {:.3}/s", op.speed, op.shaft_rps, op.power_per_s);
        assert!(op.speed > 0.3, "{op:?}");
        assert!(op.power_per_s > 0.0);
    }
}

impl Default for Boat {
    fn default() -> Self {
        Boat { mass: 1000.0, waterline: 1.0, thrust_scale: 1.0 }
    }
}

/// The thrust factor that makes `line` reach `measured` m/s in the model (bisection on the factor).
pub fn calibrate(line: &Line, lines: usize, hull: &DragModel, boat: &Boat, measured: f64) -> f64 {
    let (mut lo, mut hi) = (1e-4f64, 10.0f64);
    for _ in 0..30 {
        let mid = (lo * hi).sqrt();
        let v = operate(line, lines, 1.0, hull, &Boat { thrust_scale: mid, ..*boat }, 120.0).speed;
        if v < measured {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo * hi).sqrt()
}
