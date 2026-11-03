# KEEL — status and Phase 0 plan

## Decisions from the user (2026-11-03)

- Game definitions (`rom/data/definitions`) will be supplied. Game version: **v1.15.26**. `rom/meshes` is too large to send; the preview is built from voxels.
- Simulation priority: a **game-accurate replica** (keep decompiling the drag, buoyancy and engine code).
- Platforms: **Windows and Linux**, driven by one `build.sh` that installs or bootstraps the toolchain (vcpkg for Qt 6 + OCCT).
- Hull optimisers target **realistic real-world hulls**. The Stormworks model is a prediction or optional constraint by default. An opt-in "Stormworks-optimised" mode is allowed, behind an explicit unrealistic-results warning.
- Work continuously; report at milestones, don't pause to ask.
- In-game calibration: occasional, not before a few days from now. Prefer static evidence and batch fixtures.

Product direction: `PROJECT_PLAN.md`. This file tracks what is actually done.

## Phase 0 — Research and feasibility

| # | Task (plan §19) | Status |
|---|---|---|
| 1 | Inspect dev environment | Done for the cloud container: gcc, cmake, Java 21, Python 3.13, **no Qt 6 / OCCT**, GitHub downloads blocked. The real build target (user's machine) is still unknown |
| 2 | ADR: Qt 6 + OCCT vs alternatives | Not started. Blocked on the target OS/toolchain |
| 3 | Request and inventory reference vehicles | **Done:** about 70 vehicles and 25 MCs received (RE-0002) |
| 3a | Inventory game binaries | **Done:** RE-0001 |
| 3b | Game data files (definitions, meshes) | Definitions received (759). Meshes too large; preview from voxels instead |
| 4 | XML structure report | First pass done (RE-0002) |
| 5 | Smallest round-trip fixture | Candidate: `3607751095` (174 comps, 1 body, includes 3 MCs) |
| 6 | Lossless parser | Not started |
| 7 | Vehicle model + coordinate tests | Not started |
| 8 | Viewport prototype | Not started |
| 9 | No-op round-trip export | Not started |
| 10 | In-game load test | Needs the user |
| — | Simulation plan | **Drafted:** `docs/engineering/simulation-plan.md` |
| — | Water forces + buoyancy formula | **Recovered (E1):** RE-0003. Reference implementation in `sim-reference/sw_hydro.py`, 6 unit tests passing |
| — | Buoyancy volume generator (enclosed air) | Next: decompile the generator behind the `field_1078` failure flag |
| — | Air kernel, compartments/flooding, engine | Not started |

## Recommended order once files arrive

1. Map the vehicle XML from the workshop samples and write a lossless parse → write round-trip (Python first, which is quick
   to iterate; then port to C++ in `vehicle-io` once the format is understood).
2. Parse the game's definition files into the versioned component catalogue: mass, voxels, connector slots, buoyancy surfaces.
3. Build the voxel mass model and S0 hydrostatics, then check them against in-game float tests.
4. Carry on decompiling in parallel: the drag function, buoyancy generation, the step rate.
5. Run the stack spike (Qt 6 + OCCT viewport showing a parsed vehicle as blocks) on the user's target platform.
