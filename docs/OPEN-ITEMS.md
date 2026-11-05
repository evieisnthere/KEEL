# KEEL — what is still needed

This is the single register of known gaps at the end of Phase 0 (2026-11-03). Every item names where it is documented and
how it gets closed: **RE** = more decompilation, **GAME** = in-game test by the user, **APP** = software work. When an item
is closed, delete it here and update the RE document.

## A. Simulation accuracy gaps (affect predictions)

| # | Item | Impact | Where | Close by |
|---|---|---|---|---|
| A1 | **Waterline is an estimate.** The exact flooding pipeline is parked: the "volume capped" stage, compartment seeds (`field_830`, `0x14048c440`, `0x1402f81e0`), per-face sealing of open interiors, and the per-tick sea-water exchange | draft, trim, stability, top speed | RE-0004 §4–§8 | RE + GAME (test-pack barges A–F) |
| A2 | Ported shell-walk volumes under-float 5 of 7 ships, so the vehicle-scale mass↔volume relationship is unconfirmed | as A1 | RE-0004 §8 | RE (body mass build in Bullet), GAME |
| A3 | Sub-bodies (pivots, hinges, turrets) inside hulls add duplicate buoyancy. KEEL must warn about it, and the exact amount depends on A1 | as A1 | RE-0004 §8 | APP (warning), A1 |
| A4 | Water drag inputs still unidentified: body scalar `L` (+0x94), `A_ref` (+0x798), sigma; drag sign confirmed only by inference | speed and drag curve | RE-0003 | RE |
| A5 | Air-force altitude falloff exponents are assumed (0.6/0.4); the k value in air lift is not recovered | aircraft only | RE-0005, RE-0012 §5 | RE |
| A6 | Drivetrain assumptions: cylinder size = voxel count; unlimited air/fuel supply and fixed manifold temperature; propeller blade count fixed at 4; gearbox accumulators applied immediately | engine operating point, fuel use | `sim-reference/sw_drivetrain.py` (ASSUME lines), RE-0006–0008 | RE (manifold/fluid coupling now that RE-0010 exists), GAME |
| A6b | **Propulsion is uncalibrated.** With the recovered laws (RE-0006/0007/0008) a 2-cylinder 3×3 engine on a large propeller pushes a 5 t box to ~100 m/s: the clutch slot (mass `e·I_crank`) refills the shaft every tick and the propeller converts the absorbed rps to thrust at `F_eff/1000 · 1500` N per rps. A constant or unit in the thrust law (or the propeller slot mass) is likely off. KEEL's Power calculators carry a thrust calibration factor until an in-game top-speed test sets it | top speed, fuel use, range | `keel-power`, RE-0007 | GAME (calibration boat C1), RE |
| A6c | RE-0011's worked generator example (0.19 units/tick at 10 rps) is 10× smaller than its own formula (1.90) | charging rates | RE-0011 §3 | RE |
| A7 | Drivetrain topology gaps on other ships: connector_torque, pivots carrying torque, electric motors on torque networks | top-speed validation | STATUS | APP (extend extractor; laws are known) |
| A8 | Top-speed solver validated on one ship only (3556526754, 28.9 kn rev-limited, waterline assumed) | confidence | task #3 | APP + GAME |
| A9 | Rudder `body[+8]` read as mass (it could be inverse mass); the keel righting couple's argument is inferred to be vehicle mass | turning rate | RE-0012 | GAME (test-pack R3) |
| A10 | Electrical: what the 300-tick ramp counter measures; per-component draw constants for lights, displays, radars and seats; solar coefficients | battery life | RE-0011 §4 | RE |
| A11 | Logic between physical components (outside microcontrollers) is assumed to take one tick per link | control-loop timing | RE-0013 §4 | GAME (test-pack L1) |
| A12 | Microcontroller details: bridge type numbering is inferred (consistent with node types over 7616 corpus MCs); SR latch edge cases; the full expression grammar (operator precedence; boolean `!&` `!|` `!^` read as NAND/NOR/XNOR; `round(x, step)` read as rounding to a multiple of step, both seen in vehicles but unconfirmed); composite read/write `i` taken as a 0-based channel; Lua Script, video and audio are not simulated (Lua needs an embedded VM) | MC simulation | RE-0013, `crates/keel-mc` | RE + APP |
| A12b | Microcontroller IO ports are placed at footprint cell (x, 0, z) of the part. Corpus evidence: dangling-wire warnings on a sample vehicle fell from 218 to 19 once MC ports were added; the rest are not explained yet | wire diagnostics | `keel-model` diagnostics | RE |
| A13 | Not yet decoded at all: water jets, electric-motor torque slots on non-motor parts, jet engines, steam, wheels/tracks, winches/ropes, gyros, radar/sonar sensing models | anything using them | — | RE (when a feature needs it) |

## B. File format gaps (affect export)

| # | Item | Where | Close by |
|---|---|---|---|
| B1 | Vehicle `t` attribute, `sc` format, multi-body joint fields, and many `<o>` properties are unmapped. Round-trip is byte-exact, but authoring new parts needs them | RE-0002 | RE + corpus survey |
| B2 | A KEEL-exported vehicle has never been loaded in game (round-trip R1/R2 in the test pack) | STATUS row 9 | GAME |
| B4 | Export writes `<name>.xml` and a 512×512 `<name>.png` into the game's vehicles folder (`%APPDATA%\Stormworks\data\vehicles`, or the Proton prefix on Linux). That the game lists the PNG as the vehicle's thumbnail is unconfirmed | `keel-app/src/game.rs` | GAME |
| B5 | Paintable sign / indicator pixels (`gc`, 81 colours): rows follow local +Z and columns local +X. Inferred from seam continuity between neighbouring signs in the corpus (rows 6812 vs 3503, columns 6013 vs 3082 seams favour this order); which way is "up" on screen is not confirmed, so decals offer flips | `keel-app/src/decal.rs` | GAME |
| B3 | Unconfirmed conventions: rotation matrix convention (column-major) and the voxel-face flooding mask meaning | RE-0002 | GAME |

## C. In-game calibration the user has been sent (`keel_test_pack.zip`)
Barges A–F (draft, sealing, door frames), round-trip R1/R2. Still to add to the pack: **R3** (rudder force vs hull mass),
**L1** (logic tick counting), **E1** (battery drain with a motor at fixed throttle), **S1** (straight-line top speed of
3556526754 at full throttle).

**C2 (propulsion installer check):** generate a workboat in Decks mode with "Install the drive from Power mode" ticked,
export it, and in game: turn the key (starter should run until the engine turns), raise the throttle lever (clutch
engages, boat moves), steer with A/D. Report anything that does not connect (fuel, air, exhaust, cooling, shaft). The
engine layout follows working corpus engines but has not been run in game.

**C1 (propulsion calibration, requested 2026-11-04):** the KEEL "Planing" preset hull baked and saved from KEEL, plus
one large electric motor driving one small propeller directly, a small battery and a throttle lever at 100 %. Report
the steady top speed and the mass the editor shows. KEEL then sets the thrust factor (`keel_power::solve::calibrate`).
This also exercises the round trip (B2).

## D. Application (Phase 1 onward, see PROJECT_PLAN.md)
Known limits of alpha 0.0.1:
- The microcontroller part is drawn as one voxel whatever its footprint; its ports are placed correctly.
- Nested microcontroller groups are preserved but not shown (none in the corpus).
- Generated ships longer than ~58 m or wider than ~23.5 m exceed the game's size limit; KEEL warns but still generates.
- Power predictions carry an uncalibrated thrust factor (A6b) until test C1 is reported.
- No macOS build yet (Linux and Windows only).

- Geometry-kernel spike: truck vs opencascade-rs (ADR-0001).
- Rust ports of the reference models, golden-tested against `sim-reference/`.
- The UI: viewer → block editor → systems editor → simulation → export (plan phases 1–6).
