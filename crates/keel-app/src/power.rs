//! Power mode: powertrain designer and calculators (modular engine + gearbox + propeller, electric motors +
//! batteries), using the recovered game laws (`keel-power`).
use crate::theme;
use eframe::egui::{self, RichText};
use keel_geom::hull::{hydrostatics, Hull, HullParams};
use keel_power::drag::{game_waterline, DragModel, Face};
use keel_power::electric::{bank_capacity, BATTERIES, MOTORS};
use keel_power::engine::{Cylinder, EngineSpec, GEARS};
use keel_power::prop::PROPS;
use keel_power::solve::{self, Boat, Candidate, Drive, Line, Operating};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PowerDoc {
    pub electric: bool,
    pub cylinder: usize,
    pub cylinders: usize,
    pub flywheel: bool,
    pub motor: usize,
    pub motors: usize,
    /// index into the gearbox ratios (1 = 1:1)
    pub gear: usize,
    pub prop: usize,
    pub lines: usize,
    pub throttle: f64,
    pub thrust_scale: f64,
    /// extra mass not in the vehicle yet (cargo, interior to come), definition units
    pub extra_mass: f64,
    pub fuel: f64,
    pub battery: usize,
    pub batteries: usize,
    pub target_kn: f64,
}

impl Default for PowerDoc {
    fn default() -> Self {
        PowerDoc { electric: false, cylinder: 1, cylinders: 4, flywheel: true, motor: 2, motors: 2, gear: 1, prop: 1, lines: 1, throttle: 1.0, thrust_scale: 1.0, extra_mass: 0.0, fuel: 1000.0, battery: 1, batteries: 2, target_kn: 15.0 }
    }
}

#[derive(Default)]
pub struct Power {
    pub doc: PowerDoc,
    drag_key: Option<HullParams>,
    drag: Option<DragModel>,
    /// mass of the hull's blocks (used while the hull is not in the vehicle yet)
    hull_mass: f64,
    result_key: String,
    result: Option<(Operating, Boat)>,
    found: Vec<Candidate>,
    pub status: String,
}

impl Power {
    fn line(&self) -> Line {
        let drive = if self.doc.electric {
            Drive::Electric { motor: MOTORS[self.doc.motor.min(2)], motors: self.doc.motors.max(1) }
        } else {
            Drive::Engine(EngineSpec { cylinder: Cylinder::ALL[self.doc.cylinder.min(2)], count: self.doc.cylinders.max(1), flywheel: self.doc.flywheel })
        };
        Line { drive, ratio: GEARS[self.doc.gear.clamp(1, 7)].0, prop: PROPS[self.doc.prop.min(2)] }
    }

    fn drive_mass(&self) -> f64 {
        let l = self.line();
        let per = match &l.drive {
            Drive::Engine(e) => e.mass(),
            Drive::Electric { motor, motors } => motor.mass * *motors as f64,
        } + l.prop.mass;
        per * self.doc.lines.max(1) as f64 + if self.doc.electric { BATTERIES[self.doc.battery.min(2)].mass * self.doc.batteries as f64 } else { 0.0 }
    }

    /// Recompute the drag model when the hull changed and the operating point when anything changed.
    pub fn refresh(&mut self, hull: Option<&Hull>, params: &HullParams, vehicle_mass: f64) {
        if self.drag_key.as_ref() != Some(params) {
            self.drag = hull.map(|h| {
                let hb = keel_geom::hullblocks::build(h, &Default::default());
                // blocks weigh 1 (wedges and pyramids less; close enough for an estimate)
                self.hull_mass = hb.placements.len() as f64;
                let m = keel_geom::metrics::measure(&hb.placements, h);
                let faces = m
                    .faces
                    .iter()
                    .map(|(poly, n, _, _)| {
                        let k = poly.len() as f64;
                        let y = poly.iter().map(|p| p[1]).sum::<f64>() / k;
                        let mut cr = [0.0; 3];
                        for i in 0..poly.len() {
                            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                            cr[0] += a[1] * b[2] - a[2] * b[1];
                            cr[1] += a[2] * b[0] - a[0] * b[2];
                            cr[2] += a[0] * b[1] - a[1] * b[0];
                        }
                        Face { y, n: *n, area: 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt() }
                    })
                    .collect();
                DragModel { faces }
            });
            self.drag_key = Some(params.clone());
            self.result_key.clear();
        }
        let key = format!("{:?}{:?}{vehicle_mass}", self.doc, params);
        if key == self.result_key {
            return;
        }
        self.result_key = key;
        let (Some(h), Some(drag)) = (hull, self.drag.as_ref()) else {
            self.result = None;
            return;
        };
        let base = if vehicle_mass > 0.0 { vehicle_mass } else { self.hull_mass };
        let mass = (base + self.doc.extra_mass + self.drive_mass()).max(100.0);
        let depth = h.params.depth + h.params.sheer_bow.max(h.params.sheer_stern);
        let Some(wl) = game_waterline(mass, &|d| if d <= 0.0 { 0.0 } else { hydrostatics(h, d).volume_m3 }, 0.0, depth) else {
            self.status = format!("The hull cannot float {mass:.0} units in game");
            self.result = None;
            return;
        };
        let boat = Boat { mass, waterline: wl, thrust_scale: self.doc.thrust_scale };
        let op = solve::operate(&self.line(), self.doc.lines.max(1), self.doc.throttle, drag, &boat, 120.0);
        self.status.clear();
        self.result = Some((op, boat));
    }

    pub fn panel_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            theme::section(ui, "Drive");
            ui.horizontal(|ui| {
                if ui.selectable_label(!self.doc.electric, "Modular engine").clicked() {
                    self.doc.electric = false;
                }
                if ui.selectable_label(self.doc.electric, "Electric").clicked() {
                    self.doc.electric = true;
                }
            });
            if self.doc.electric {
                egui::ComboBox::from_id_salt("motor").selected_text(MOTORS[self.doc.motor].name).show_ui(ui, |ui| {
                    for (i, m) in MOTORS.iter().enumerate() {
                        ui.selectable_value(&mut self.doc.motor, i, m.name);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Motors per line");
                    ui.add(egui::DragValue::new(&mut self.doc.motors).range(1..=16));
                });
            } else {
                egui::ComboBox::from_id_salt("cyl").selected_text(Cylinder::ALL[self.doc.cylinder].name()).show_ui(ui, |ui| {
                    for (i, c) in Cylinder::ALL.iter().enumerate() {
                        ui.selectable_value(&mut self.doc.cylinder, i, c.name());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Cylinders");
                    ui.add(egui::DragValue::new(&mut self.doc.cylinders).range(1..=16));
                    ui.checkbox(&mut self.doc.flywheel, "Flywheel");
                });
            }
            egui::ComboBox::from_id_salt("gear").selected_text(format!("Gearbox {}", GEARS[self.doc.gear].1)).show_ui(ui, |ui| {
                for (i, g) in GEARS.iter().enumerate().skip(1) {
                    ui.selectable_value(&mut self.doc.gear, i, g.1);
                }
            });
            egui::ComboBox::from_id_salt("prop").selected_text(PROPS[self.doc.prop].name).show_ui(ui, |ui| {
                for (i, p) in PROPS.iter().enumerate() {
                    ui.selectable_value(&mut self.doc.prop, i, p.name);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Lines (shafts)");
                ui.add(egui::DragValue::new(&mut self.doc.lines).range(1..=4));
            });
            ui.add(egui::Slider::new(&mut self.doc.throttle, 0.05..=1.0).text("throttle"));

            theme::section(ui, "Supply");
            if self.doc.electric {
                egui::ComboBox::from_id_salt("bat").selected_text(BATTERIES[self.doc.battery].name).show_ui(ui, |ui| {
                    for (i, b) in BATTERIES.iter().enumerate() {
                        ui.selectable_value(&mut self.doc.battery, i, b.name);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Batteries");
                    ui.add(egui::DragValue::new(&mut self.doc.batteries).range(1..=64));
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label("Fuel");
                    ui.add(egui::DragValue::new(&mut self.doc.fuel).range(1.0..=1e7).suffix(" units"));
                });
            }
            ui.horizontal(|ui| {
                ui.label("Extra mass");
                ui.add(egui::DragValue::new(&mut self.doc.extra_mass).range(0.0..=1e7).speed(10.0)).on_hover_text("cargo, fuel, interior not built yet (definition mass units)");
            });

            theme::section(ui, "Calibration");
            ui.add(egui::Slider::new(&mut self.doc.thrust_scale, 0.001..=2.0).logarithmic(true).text("thrust factor"));
            ui.label(RichText::new("Uncalibrated: the recovered propeller and coupling laws give too much thrust (OPEN-ITEMS A6b). Run calibration boat C1 in game and enter its top speed to set this factor.").small().color(theme::ORANGE));

            theme::section(ui, "Design search");
            ui.horizontal(|ui| {
                ui.label("Target");
                ui.add(egui::DragValue::new(&mut self.doc.target_kn).range(1.0..=80.0).suffix(" kn"));
                if ui.button("Find drivetrains").clicked() {
                    self.search();
                }
            });
            let mut use_idx = None;
            for (i, c) in self.found.iter().take(8).enumerate() {
                ui.horizontal(|ui| {
                    let rate = if self.doc.electric { format!("{:.2} charge/s", c.op.power_per_s) } else { format!("{:.4} fuel/s", c.op.fuel_per_s) };
                    ui.label(RichText::new(format!("{:.1} kn · {} · +{:.0} mass", c.op.knots(), rate, c.mass)).small());
                    if ui.small_button("Use").on_hover_text(&c.describe).clicked() {
                        use_idx = Some(i);
                    }
                });
                ui.label(RichText::new(&c.describe).small().color(theme::TEXT_DIM));
            }
            if let Some(i) = use_idx {
                self.adopt(i);
            }
        });
    }

    fn search(&mut self) {
        let (Some(drag), Some((_, boat))) = (self.drag.as_ref(), self.result.as_ref()) else { return };
        let target = self.doc.target_kn / 1.943_844;
        let base = Boat { mass: boat.mass - self.drive_mass(), ..*boat };
        self.found = if self.doc.electric { solve::search_electric(target, drag, &base, 6) } else { solve::search_engine(target, drag, &base, 8) };
        self.status = format!("{} drivetrains reach {:.1} kn", self.found.len(), self.doc.target_kn);
    }

    fn adopt(&mut self, i: usize) {
        let c = self.found[i].clone();
        self.doc.lines = c.lines;
        self.doc.gear = GEARS.iter().position(|g| (g.0 - c.line.ratio).abs() < 1e-9).unwrap_or(1);
        self.doc.prop = PROPS.iter().position(|p| p.id == c.line.prop.id).unwrap_or(1);
        match c.line.drive {
            Drive::Engine(e) => {
                self.doc.electric = false;
                self.doc.cylinder = Cylinder::ALL.iter().position(|x| *x == e.cylinder).unwrap_or(1);
                self.doc.cylinders = e.count;
                self.doc.flywheel = e.flywheel;
            }
            Drive::Electric { motor, motors } => {
                self.doc.electric = true;
                self.doc.motor = MOTORS.iter().position(|m| m.id == motor.id).unwrap_or(2);
                self.doc.motors = motors;
            }
        }
    }

    pub fn results_ui(&self, ui: &mut egui::Ui) {
        theme::section(ui, "Operating point");
        if !self.status.is_empty() {
            ui.label(RichText::new(&self.status).color(theme::ORANGE));
        }
        let Some((op, boat)) = &self.result else {
            ui.label(RichText::new("Design a hull first (Hull mode).").color(theme::TEXT_DIM));
            return;
        };
        let row = |ui: &mut egui::Ui, k: &str, v: String| {
            ui.horizontal(|ui| {
                ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(k).color(theme::TEXT_DIM)));
                ui.label(v);
            });
        };
        row(ui, "Top speed", format!("{:.2} m/s · {:.1} kn{}", op.speed, op.knots(), if op.converged { "" } else { " (still accelerating)" }));
        if !self.doc.electric {
            row(ui, "Engine", format!("{:.1} rps · {:.0} rpm · efficiency {:.0} %", op.engine_rps, op.engine_rps * 60.0, op.efficiency * 100.0));
        }
        row(ui, "Propeller shaft", format!("{:.1} rps", op.shaft_rps));
        row(ui, "Thrust / drag", format!("{:.0} N / {:.0} N", op.thrust, op.drag));
        row(ui, "Mass", format!("{:.0} units (drive {:.0})", boat.mass, self.drive_mass()));
        row(ui, "Game waterline", format!("{:.2} m above the baseline", boat.waterline));
        let fuel = self.doc.fuel;
        let charge = bank_capacity(&BATTERIES[self.doc.battery.min(2)], self.doc.batteries);
        if self.doc.electric {
            row(ui, "Electric demand", format!("{:.2} charge units/s", op.power_per_s));
            row(ui, "Battery bank", format!("{:.0} units", charge));
        } else {
            row(ui, "Fuel use", format!("{:.4} units/s · {:.1} units/h", op.fuel_per_s, op.fuel_per_s * 3600.0));
        }
        if let Some(t) = op.endurance(fuel, charge) {
            row(ui, "Endurance", format!("{:.1} h", t / 3600.0));
            row(ui, "Range", format!("{:.1} km · {:.1} nmi", op.speed * t / 1000.0, op.speed * t / 1852.0));
        }
        if let Some(d) = &self.drag {
            theme::section(ui, "Drag curve (in game, steady)");
            for v in [2.0, 5.0, 8.0, 12.0, 16.0, 20.0] {
                row(ui, &format!("{:.0} m/s ({:.0} kn)", v, v * 1.943_844), format!("{:.0} N", d.drag(v, boat.waterline, boat.mass)));
            }
            row(ui, "Wetted area", format!("{:.1} m²", d.wetted(boat.waterline)));
        }
    }
}
