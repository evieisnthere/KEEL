# KEEL — status and Phase 0 plan

## Decisions from the user (2026-11-03)

- Game definitions (`rom/data/definitions`) will be supplied. Game version: **v1.15.26**. `rom/meshes` is too large to send; the preview is built from voxels.
- Simulation priority: a **game-accurate replica** (keep decompiling the drag, buoyancy and engine code).
- Platforms: **Windows and Linux**, driven by one `build.sh`. **Stack changed to Rust** (ADR-0001), so `build.sh` bootstraps rustup.
- `rom/meshes` received (component meshes, 1497 files). Never committed.
- Hull optimisers target **realistic real-world hulls**. The Stormworks model is a prediction or optional constraint by default. An opt-in "Stormworks-optimised" mode is allowed, behind an explicit unrealistic-results warning.
- Work continuously; report at milestones, don't pause to ask.
- Goal (user): exact 60 Hz simulation of a boat driven by its own throttle lever/helm (logic clock 60 Hz, user-confirmed). Flooding to be derived from game code, not guessed.
- In-game calibration: occasional, not before a few days from now. Prefer static evidence and batch fixtures.

Product direction: `PROJECT_PLAN.md`. This file tracks what is actually done.

## Phase 0 — Research and feasibility

| # | Task (plan §19) | Status |
|---|---|---|
| 1 | Inspect dev environment | Done for the cloud container: gcc, cmake, Java 21, Python 3.13, **no Qt 6 / OCCT**, GitHub downloads blocked. The real build target (user's machine) is still unknown |
| 2 | ADR: stack | **Done:** ADR-0001 Rust (egui/wgpu; geometry kernel spike truck vs opencascade-rs pending) |
| 3 | Request and inventory reference vehicles | **Done:** about 70 vehicles and 25 MCs received (RE-0002) |
| 3a | Inventory game binaries | **Done:** RE-0001 |
| 3b | Game data files (definitions, meshes) | Definitions received (759). Meshes too large; preview from voxels instead |
| 4 | XML structure report | First pass done (RE-0002) |
| 5 | Smallest round-trip fixture | Candidate: `3607751095` (174 comps, 1 body, includes 3 MCs) |
| 6 | Lossless parser | **Done** in Python (`src/keel_io`) and **Rust** (`crates/keel-io`). 850/850 XML byte-identical, 1497/1497 meshes parse |
| 7 | Vehicle model + coordinate tests | Model done (`keel_io.vehicle`). Axes: Y up, Z fore–aft (E1-inferred) |
| 8 | Viewport prototype | Not started |
| 9 | No-op round-trip export | Implemented. In-game load test pending (user) |
| 10 | In-game load test | Needs the user |
| — | Simulation plan | **Drafted:** `docs/engineering/simulation-plan.md` |
| — | Water forces + buoyancy formula | **Recovered (E1):** RE-0003. Reference implementation in `sim-reference/sw_hydro.py`, 6 unit tests passing |
| — | Surface types, conversion, buoyancy builder | **Recovered (E1):** RE-0004. 58 surface types and the definition→type conversion extracted by emulation. Builder = edge-loop shell walk + z-column scan |
| — | Buoyancy volume calibration | Envelope z-scan model: 6/8 sample boats float plausibly. Test pack with predictions sent to the user (barges A, C, D, E, F + round-trip R1/R2) |
| — | Component transform convention | `r` is column-major (world = rᵀ·v), E1-inferred from voxel-overlap minimisation |
| — | Air forces | **Recovered (E1):** RE-0005, no air force below 15 m/s. Reference + tests |
| — | Modular engine cylinder + couplings | **Recovered (E1):** RE-0006. Reference `sw_engine.py` + tests |
| — | Propellers (force emitters) | **Recovered (E1):** RE-0007, load/thrust law. Absolute magnitudes unverified (blade count/variant flags) |
| — | Torque networks + connections | **Recovered (E1):** RE-0008. Network solver, slot layout, gear ratios, port/pipe connection rules (pipes carry torque and fluid) |
| — | Drivetrain sim | `sim-reference/sw_drivetrain.py`: topology extraction + 60 Hz ticks. 3556526754: rev-limited 60 RPS, 28.9 kn (waterline assumed). Other ships: topology gaps (connector_torque, pivots, electric motors) |
| — | Fluids + pumps | **Recovered (E1):** RE-0010 pressure law, transfer law, pump head. Reference + tests |
| — | Flooding / waterline | **Pinned as an estimate** (product owner, 2026-11-03): envelope z-scan draft. Shell walk ported (exact on unit cases); the "volume capped" stage and per-face sealing are parked. `buoy_*` definition fields are a debug draw only (E1). Sub-bodies inside hulls add duplicate buoyancy (RE-0004 §8) |
| — | Electrical | **Recovered (E1):** RE-0011. Battery pool (10-unit window per battery), network charge factor, motor/generator/diode/relay laws. `sim-reference/sw_electric.py` + 5 tests |
| — | Next RE | Torque network + gearbox ratios, `buoy_pipes`/flooding, microcontroller node catalogue (`0x1403698d0`), fluid networks |

## Recommended order once files arrive

1. Map the vehicle XML from the workshop samples and write a lossless parse → write round-trip (Python first, which is quick
   to iterate; then port to C++ in `vehicle-io` once the format is understood).
2. Parse the game's definition files into the versioned component catalogue: mass, voxels, connector slots, buoyancy surfaces.
3. Build the voxel mass model and S0 hydrostatics, then check them against in-game float tests.
4. Carry on decompiling in parallel: the drag function, buoyancy generation, the step rate.
5. Run the stack spike (Qt 6 + OCCT viewport showing a parsed vehicle as blocks) on the user's target platform.
