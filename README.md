# KEEL — Stormworks Engineering Suite

KEEL is a desktop CAD and systems-engineering suite for designing vehicles for *Stormworks: Build and Rescue*: hulls,
superstructures, interiors, engines, drivetrains, electrical, fluid and logic systems. Its central promise is that
**anything you design can be exported as a native vehicle file that loads and behaves correctly in the game**, and that
KEEL can **predict how it will behave** (draft, stability, speed, fuel use, engine operating point) without the game running.

Product direction lives in [`PROJECT_PLAN.md`](PROJECT_PLAN.md). Live progress is in [`docs/STATUS.md`](docs/STATUS.md).
Everything still missing (research gaps, in-game checks, app work) is in [`docs/OPEN-ITEMS.md`](docs/OPEN-ITEMS.md).

---

## Where the project is now

**Phase 0: research and feasibility.** No end-user application yet, by design. The product owner's rule is that every
game system KEEL simulates is decoded and documented first, so that the software is built on facts rather than guesses.

What exists today:

| Area | State |
|---|---|
| Native file I/O | Byte-identical round-trip of all 850 sample vehicle, microcontroller and definition files, in Python (`src/keel_io`) and **Rust** (`crates/keel-io`). Reads all 1 497 component meshes |
| Game physics constants | 41 tuning values recovered with their defaults (gravity 10, water density 1000, drag, buoyancy, damping…) |
| Water and air forces | Exact per-surface drag, suction, skin friction, slamming, lift, and buoyancy formulas (RE-0003, RE-0005) |
| Buoyancy geometry | The 58 internal surface types, the definition→surface conversion, the shell walk and z-column volume scan (RE-0004) |
| Engines | Modular-engine combustion: air/fuel ratio window, efficiency, rev limiter, heating, starter and clutch couplings (RE-0006) |
| Propellers | Load/thrust law (RE-0007) |
| Drivetrains | Port/pipe connection rules, the torque-network solver and gear ratios (RE-0008). Drivetrains are extracted from vehicle files and simulated at 60 Hz |
| Fluids & electrical | Pressure/transfer/pump laws (RE-0010); battery pool, motors, generators (RE-0011) |
| Waterline | **Estimate** for now (envelope z-scan). The exact game flooding model is parked (RE-0004 §8) |
| Logic & steering | Microcontroller format, catalogue and tick semantics (RE-0013); rudders, fins, control surfaces and keels (RE-0012) |
| Application | **Alpha 0.0.1:** `keel` desktop app (Rust, egui) with Build, Hull, Decks, Power and Logic modes (see below) |

A first end-to-end result: the workshop boat 3556526754 ("Your Boat", two 12-cylinder modular engines, 3:1 gearboxes, two
propellers) simulated purely from its file reaches **28.9 kn, limited by the 3600 RPM rev limiter**. Its waterline is
still assumed until flooding is decoded.

---

## How the research works

1. **Static analysis** of the user's own game executable (`stormworks64.exe`, v1.15.26): strings, MSVC RTTI class/vtable
   recovery, cross-references, and decompilation with angr.
2. **Emulation** of self-contained game routines with unicorn, e.g. to dump the 58 surface types and the surface-conversion
   table exactly as the game builds them.
3. **File-format analysis** of the user's component definitions, meshes and workshop vehicles.
4. **In-game calibration**: small test vehicles with written predictions (`tools/fixtures/make_test_pack.py`).

Every finding carries an evidence level: **E1** read from the game, **E1-inferred** a meaning derived from E1 data, **E2**
measured in game, **E3** community knowledge, **E4** generic engineering approximation. Python reference models in
`sim-reference/` are executable specifications. The Rust crates must match them.

**Never committed:** game executables, game data files (definitions, meshes) and workshop vehicles. Tools take your own
copies as input paths. Only small factual tables derived from them are stored (`research/extracted-metadata/`).

---

## Repository map

```
PROJECT_PLAN.md                     product direction
build.sh                            build / test entry point (Linux, macOS, Windows Git Bash)
Cargo.toml, crates/keel-io/         Rust: lossless XML, vehicle model, mesh reader
src/keel_io/                        Python: lossless XML, vehicle model, mesh reader
sim-reference/                      Python reference models of game systems
  sw_hydro.py                         water + air forces, buoyancy scaling
  sw_buoyancy.py                      surface placement, shell/volume scans, sealed-volume experiments
  sw_engine.py                        modular-engine cylinder tick, couplings, propellers
  sw_drivetrain.py                    drivetrain extraction from vehicle files + 60 Hz tick sim
  sw_offline.py                       mass/COM, draft, drag curve, top-speed helpers
docs/
  STATUS.md                           phase checklist and decisions
  decisions/ADR-0001-*.md             stack decision (Rust)
  engineering/simulation-plan.md      simulation architecture and hull-optimiser policy
  reverse-engineering/RE-0001…        findings, one document per subsystem (see index below)
research/
  binaries/SHA256SUMS                 which game build was analysed
  extracted-metadata/                 physics defaults, field tables, surface shapes, conversion tables
tools/
  re/                                 analysis tooling (xrefs, RTTI, decompile, emulate, extractors)
  fixtures/make_test_pack.py          in-game calibration vehicles + predictions
  offline_report.py                   draft / drag / thrust / top-speed report for a vehicle
  viz/silhouettes.py                  orthographic renders used to fix axis conventions
tests/unit/                         Python tests (reference models, lossless XML)
```

### Reverse-engineering index

| Doc | Subject |
|---|---|
| RE-0001 | Binary inventory, physics settings table, component classes, slot types |
| RE-0002 | Vehicle XML, definitions (incl. non-standard XML), meshes, axes (Y up, Z fore–aft) |
| RE-0003 | Water forces: per-surface pressure/suction drag, skin friction, slamming, impulse cap, buoyancy |
| RE-0004 | 58 surface types, buoyancy shell walk, z-column volume scan, compartments, hole pipeline (in progress) |
| RE-0005 | Air forces: 15 m/s threshold, drag, lift, altitude falloff |
| RE-0006 | Modular engine: combustion tick, module types, couplings, starter, alternator |
| RE-0007 | Propellers / rotors: load and thrust law |
| RE-0008 | Connections (wired vs physical), pipes, torque network solver, gearboxes, clutches, tick dispatch |
| RE-0010 | Fluid volumes: pressure law, transfer law, pumps |
| RE-0011 | Electrical: battery pool, charge factor, motors, generators, relays, diodes, solar |
| RE-0012 | Rudders, control fins, control surfaces, keels, wings: actuator, water and air forces |
| RE-0013 | Microcontrollers: XML format, 60-component catalogue, execution order, component behaviours |

---

## Using it

Requirements: Python 3.11+ (`pytest`, `numpy`, `scipy` for some tools). Rust is installed automatically by `build.sh`.

```bash
./build.sh                 # build Rust crates (release)
./build.sh test            # Rust + Python unit tests
KEEL_CORPUS=/path/to/your/files ./build.sh corpus   # round-trip your own vehicle/definition XML and meshes
```

### The KEEL application (alpha 0.0.1)

Download `keel` (Linux) or `keel.exe` (Windows) from the GitHub release, or build it:

```bash
./build.sh run /path/to/vehicle.xml --defs /path/to/Stormworks/rom/data/definitions
```
KEEL reads your own game files and never copies or bundles them. Point it at `rom/data/definitions` (File → Settings,
`--defs`, or the `KEEL_DEFINITIONS` environment variable). Real part meshes are used when `--rom /path/to/rom` is given.

| Mode | What it does |
|---|---|
| **Build** | 3D editor for native vehicles. Thumbnail part palette by game category with search, favourites (right-click) and recent parts. Tools: Select, Place (R / Shift+R / Alt+R rotate), Paint (Alt+click picks a colour), box Fill, Measure, Wire (ports coloured by type, click output then input). Mirror across the centre line (M), Ctrl+C / Ctrl+V stamps, undo/redo, the Scale tool with its sealing warning, diagnostics including dangling wires |
| **Hull** | Parametric hull with real-world hydrostatics. Bakes to smooth blocks built the way a builder would: deck-edge curve from wedges, then pyramids down the sides |
| **Decks** | Choose a boat type (cruise ship, Ro/Pax ferry, workboat, tug, motor yacht, passenger ferry, patrol, rescue) and it lists the rooms it needs. Set deck heights, watch the floor plan update live in its own window, then generate walls, doors, furniture, windows, lights (kelvin colour, dimmer in dim rooms), per-room switches wired through push-to-toggles with a master control, a battery bus, the superstructure, stairs and ladders |
| **Power** | Modular engine + gearbox + propeller and electric motor + battery calculators, with a design search. Speeds use a thrust calibration factor until in-game test C1 sets it |
| **Logic** | Microcontroller editor: node canvas with all 60 component types, wires, IO nodes and footprint, settings with expression checking, and a tick simulator that follows the game's one-tick-per-component order. Edits are lossless: everything KEEL does not edit is written back byte for byte |

Untouched XML is saved byte-for-byte, including other people's scale and merge edits. Linux needs the usual desktop
libraries (X11 or Wayland, OpenGL, `libxkbcommon-x11`); Windows needs nothing extra.

This is an alpha: export has not yet been confirmed in game for every generator, and predictions marked as estimates in
[`docs/OPEN-ITEMS.md`](docs/OPEN-ITEMS.md) are estimates.

Offline report for one of your vehicles (needs your `rom/data/definitions` folder):

```bash
python3 tools/offline_report.py /path/to/rom/data/definitions research/extracted-metadata /path/to/vehicle.xml
```

---

## Roadmap after Phase 0

Rust application: egui/wgpu shell with a block-first 3D viewport → native block editor → CAD surfaces and hull tools
(realistic hull optimiser by default, opt-in "Stormworks-optimised" mode) → systems editor and microcontroller IDE →
engine builder with full simulation → validation and export. Full plan and phase exit criteria are in `PROJECT_PLAN.md`.

## Legal

KEEL is an independent tool and is not affiliated with Geometa or the Stormworks developers. Analysis is performed on
files the user owns, for interoperability. No game assets or code are distributed.
