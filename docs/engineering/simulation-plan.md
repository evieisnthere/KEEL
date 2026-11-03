# KEEL simulation plan

Status: **proposal** · depends on RE-0001

## Goal

Predict how a design will behave **in Stormworks**: draft, trim, stability, top speed, the engine/propeller operating point
and fuel endurance. Every number carries its evidence level. Real-world naval-architecture estimates sit in a separate
channel and are never mixed in (plan §5.4).

Key implication from RE-0001: Stormworks does not use real-world hull resistance. It applies **per-surface
linear and quadratic pressure/suction drag**, voxel-based buoyancy, gravity = 10 and Bullet velocity damping.
So the "Stormworks estimate" channel has to **reimplement that model** rather than adjust Holtrop or ITTC results.

**Hull optimiser policy (user decision, 2026-11-03):** optimisers target **realistic, real-world hull forms**. The objective uses
real naval-architecture measures (resistance estimates, form coefficients, stability, fairness). The Stormworks model is reported
alongside each candidate and may be used as an optional *constraint* (e.g. "must float level and reach X knots in-game"),
never as the default objective. A hull that looks right and behaves plausibly beats one gamed to the per-face drag model.
Users **may opt in** to a "Stormworks-optimised" objective that optimises directly against the in-game model. It must be clearly
marked as such, with a warning that results may be unrealistic or odd-looking ("jank"). Realistic remains the default.

## Architecture: `keel-sim`, a C++ library with no Qt and no OCCT dependency

```
keel-sim/
  params/      versioned game-constant sets (physics_settings_defaults.json keyed by game build)
  mass/        voxel mass model: per-block mass from definitions -> mass, COM, inertia tensor
  hydro/       buoyancy volume generation, submerged-volume integration, compartments/flooding
  drag/        per-surface pressure/suction drag (water + air), slamming, wave terms, lift
  body/        6-DOF rigid-body integrator matching Bullet's damping/step semantics
  networks/    torque, fluid, electric, logic graphs (graph model shared with the Systems editor)
  propulsion/  modular engine, gearbox, propeller/force_emitter, rudder, jet, electric motor
  scenarios/   static float test, speed trial, turning circle, endurance run
  calib/       in-game measurement records, predicted-vs-measured error tracking
```

Each module declares an evidence level per equation (E1 / E1-inferred / E2 / E3 / E4), and results carry the lowest
evidence level used to compute them.

## Staged delivery

| Stage | Output | Needs |
|---|---|---|
| S0 | Hydrostatics: displacement, draft, LCB/VCB, trim at rest, using the voxel model and water density 1000 / g 10 | definition masses; buoyancy decompile (or E2 float tests) |
| S1 | Static stability: GZ curve from the voxel buoyancy volume; compartment flooding what-ifs | S0 + compartment rules |
| S2 | Speed trial: time-domain 6-DOF integration with surface drag and Bullet damping, constant thrust input -> top speed, running trim | drag decompile; step rate |
| S3 | Propulsion: propeller/force-emitter thrust vs rps, gearbox, torque network | force_emitter and torque decompile |
| S4 | Modular engine: torque/rps/AFR/temperature/fuel burn -> solve for the engine–gearbox–prop–hull operating point | engine tick decompile |
| S5 | Endurance/range from usable fuel and an operating profile; electrical budget | fluid + electric networks |
| S6 | Calibration loop: import in-game measurements, report prediction error per stage | user test runs |

The speed trial runs as a **time-domain simulation** at the game step rate rather than an analytic formula.
Per-surface drag, slamming and damping interact non-linearly. Integrating them directly is the honest way to match
the game, and it is cheap at the voxel-face level.

## Validation fixtures (the user runs these in-game)

1. **Float test:** a solid N×M×K block barge of a known block type. Record draft at rest. Repeat for 3 masses spanning
   2 000–23 000 kg, which tests the `buoyancy_mass_modifier` range.
2. **Drop test:** release the same barge from 1 m. Record the oscillation decay (`buoyancy_damping`).
3. **Tow/thrust test:** a fixed-thrust source on a box hull. Record speed vs thrust at 3 thrust levels, which separates the
   linear and quadratic drag terms.
4. **Engine bench:** a modular engine on a fixed gearbox and propeller. Log rps, temperature and fuel use at fixed throttle.

Each fixture gets a small vehicle XML in `tests/fixtures/in-game/` once the vehicle format is mapped.

## Risks

- Serialiser defaults may be overridden at runtime. Fixtures 1–3 detect this.
- Bullet damping is applied per step, so if the step rate is wrong every speed prediction is off by a constant factor.
  Pin the step rate down early.
- Reimplement from documented equations. Do not copy decompiled code into the product. RE notes record the equations and constants.
