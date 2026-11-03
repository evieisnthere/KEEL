# RE-0001 — Game binary inventory and first findings

Status: **in progress** · Phase 0 · Evidence gathered 2026-11-03

## Provenance

| File | SHA-256 | Notes |
|---|---|---|
| `stormworks64.exe` | `c38c03ff…a338d` | PE32+ GUI, x86-64, MSVC, stripped. PE timestamp 2026-10-29 10:01:45 UTC |
| `server64.exe` | `55acb6c9…3a7c` | PE32+ console. Dedicated server. Contains the same physics tables. |

Full hashes: `research/binaries/SHA256SUMS`. **The binaries are not committed** (proprietary; see `.gitignore`).
The game version string has not been found yet. It needs confirming with the user (see Open questions).

Method: static analysis only. Nothing was executed. Tools are in `tools/re/`:
a PE section parser, a RIP-relative cross-reference index, a call graph, a per-function
capstone sweep, a named-field table extractor (`allparams.py`), and angr's decompiler for single functions.

### Evidence levels used in this repo

- **E1:** read directly from game binaries or data (exact strings, constants, call structure)
- **E1-inferred:** a semantic interpretation of E1 data (e.g. what a field *means*)
- **E2:** measured in a controlled in-game test
- **E3:** community knowledge, not yet verified
- **E4:** generic engineering approximation

## Key findings

### F1. Physics engine: Bullet (E1)
The binary contains 184 `bt*` Bullet Physics type and serialisation names (`btRigidBodyDoubleData`, `btCompoundShapeData`, `btConvexHullShapeData`, …).
It wraps Bullet in its own scene layer (`mm_physics_scene`), whose events include `event_create_compound_shape`, `event_create_convex_mesh`,
constraints (hinge, slider, ball-socket, cone-twist, fixed) and `event_create_rope`.
So rigid-body integration, damping and collisions follow Bullet semantics. Buoyancy and drag are game code layered on top.

### F2. Global physics settings table (E1 values, E1-inferred meaning)
Function `0x140295db0` registers 41 named floats with defaults through a generic float serialiser (`0x1408f3a70`).
Full data: `research/extracted-metadata/physics_settings_defaults.json`.

| Group | Field | Default |
|---|---|---|
| World | `gravity` | **10.0** (not 9.81) |
| | `water_density` / `air_density` | 1000 / 1.225 |
| Water drag (per surface) | `drag_factor_linear_pressure` / `_quadratic_pressure` | 15 / 17 |
| | `drag_factor_linear_suction` / `_quadratic_suction` | 12 / 5 |
| | `falloff_power_pressure` / `_suction` | 2.5 / 0.3 |
| | `water_drag_factor` | 1.0 |
| | `water_drag_weight_factor_multiplier` | 4.0 |
| | `viscous_resistance_factor` | 0.5 |
| | `slamming_force_multiplier` | 100 |
| | `wave_velocity_factor` / `_reverse` | 0.5 / 0.3 |
| | `lift_force` | 0.25 |
| Air drag | `air_drag_factor_linear/quadratic_pressure` | 0.05 / 0.1 |
| | `air_drag_factor_linear/quadratic_suction` | 0.025 / 0.05 |
| | `air_falloff_power_pressure/suction` | 0.6 / 0.4 |
| | `air_drag_factor` | 0.2 |
| | `air_speed_threshold`, `air_high_speed_threshold`, `air_high_speed_factor_scale` | 15, 50, 8 |
| | `viscous_air_resistance` | 0.25 |
| Buoyancy | `buoyancy_mass_multiplier`, `buoyancy_mass_modifier` | 10, 10 |
| | `buoyancy_mass_modifier_lower` / `_upper` / `_pow` | 2000 / 23000 / 1.0 |
| | `buoyancy_damping` / `_submerged` | 0.05 / 0.92 |
| | `water_compartment_force_factor`, `gas_compartment_force_factor` | 1.0, 1.0 |
| Body | `vehicle_linear_damping`, `vehicle_angular_damping` | 0.1, 0.1 (Bullet damping) |
| | `vehicle_friction` | 0.8 |
| Sensors | `sonar_noise`, `radar_noise`, `radar_distance_noise` | 0.001, 0.001, 0.01 |

What the names tell us (E1-inferred, still to be confirmed by decompiling the code that uses them):
- Hydrodynamic drag is a **per-surface model**. Each surface has separate *pressure* (leading, facing the flow) and *suction*
  (trailing) coefficients, each with linear and quadratic terms in velocity. A *falloff power* is probably applied to the
  angle between the surface normal and the flow direction. This is not a hull-form resistance method like Holtrop.
- `buoyancy_mass_modifier_lower/upper` (2000–23000) look like a vehicle-mass range over which buoyancy is scaled.
  How the scaling works is **unknown**, and it matters a lot for draft predictions.
- Bullet linear damping of 0.1 bleeds velocity every step whatever the hull shape. It will be a major term in top speed.

These are serialiser *defaults*. A data file or later code could override them. Status: **unverified at runtime**.

### F3. Component definition schema (E1)
Function `0x140318e30` parses component definitions and is the only reader of `buoyancy_surfaces`.
It holds 63 fields, including `mass`, `buoy_radius/factor/force`, `force_emitter_*` (propeller/rotor blades:
`blade_efficiency` 0.01, `blade_physics_length` 4.0, `rotation_speed` 1.0), `engine_max_force`,
`engine_frictionless_force`, `wheel_*`, `rudder_surface_area` (default 1.0), `pump_pressure`, `jet_engine_efficiency`,
`electric_charge_capacity`, `electric_magnitude`, `piston_len` 0.44, `piston_cam` 0.075, `door_*`, `radar_*`, `camera_fov_*`.
The per-component values live in the game's definition files, which we don't have yet.

### F4. Native vehicle file vocabulary (E1, structure not yet mapped)
Recovered field tables include the vehicle root (`data_version`, `bodies_id`), component instance (`definition`, `trans_index`, `d`, `t`),
microcontroller (`name description width length id_counter id_counter_node`), logic node-link fields (`te pe`, `n on off`, `vp`),
the PID controller state, and per-system runtime state:
- Modular engine: `air_in air_out temp_in temp_out rps rps_prev`. UI strings: "Air:Fuel Ratio", "Stoichiometric", "Coolant Scaling".
- Torque network: `rps`. Gearbox: `gear_ratio_1/2` (an enum, `e_gear_ratio`).
- Fluid network: `demand flow`, `mass demand flow factor ratio`, `vol_total temp rad`, `fluid_type fluid_filter fluid_fill`.
  Volumes are stored internally in **centilitres** (game help text).
- Electrical: `emp p_charge p_capacity`.

All 311 tables: `research/extracted-metadata/stormworks64_field_tables.txt`.
Fields marked `?` or `None` are dynamic and not constant defaults.

### F5. Connection model (E1)
Slot classes: `c_vehicle_electric_slot`, `c_vehicle_fluid_slot`, `c_vehicle_torque_slot`, `c_vehicle_rope_slot`, `c_vehicle_logic_slot`.
Logic I/O subtypes: `bool`, `f32`, `composite`, `audio`, `video`.
That is the full candidate list for KEEL's typed connection graph.

### F6. Component runtime classes (E1)
There are 66 `c_scene_vehicle_component_*` classes. They include `buoyancy`, `modular_engine`, `engine`, `jet_engine`, `torque`,
`pump`, `rudder`, `force_emitter`, `stabilizer`, `ski`, `wheel(_advanced)`, `electric`, `nuclear`, `steam`, `heater`,
`microprocessor`, `lua`, `logic_gate`, `composite`, `transmission_line`, `door(_custom|_dock|_manual|_manual_sliding)`, `multibody`.
They define the behaviour families the catalogue has to cover.

## Open questions / next RE steps

1. Decompile the per-surface drag function to confirm F2's formula, then do the same for the buoyancy function.
   Leads: the vehicle physics code clusters around `0x1405e0000`–`0x1405f0000` (e.g. `0x1405e2210` holds the
   "Buoyancy Generation Failed" message), and the settings code sits around `0x14029xxxx`.
2. Find how buoyancy volume is generated from voxels and `buoyancy_surfaces`. This is the basis for flood-integrity checks.
3. Find the physics step rate. 60 Hz is E3 community knowledge.
4. Map the modular engine tick (combustion, AFR, temperature) and the torque-network solver.
5. Cross-check every finding against `server64.exe`, which is smaller and also simulates physics.
