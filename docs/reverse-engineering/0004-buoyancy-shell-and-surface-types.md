# RE-0004 — Buoyancy shell builder, compartments and the 58 surface types

Status: **in progress** · Game v1.15.26 · Evidence E1 unless marked

## 1. The 58 internal surface types (E1, exact)

`sub_1402f2970` lazily builds a static table of **58** surface types (`0x3a`). We ran its initialiser in a unicorn emulator
(`tools/re/emu.py`) and exported the result to `research/extracted-metadata/surface_shapes.json`. This is a list of
directed edges per type, in half-voxel integer units (−1, 0, 1), plus a 3×3 integer orientation record.

| Types | Count | Geometry | Area (m²) |
|---|---|---|---|
| 0–5 | 6 | Full voxel faces ±X ±Y ±Z | 0.0625 |
| 6–29 | 24 | Quarter-face triangles (face edge to face centre), 4 per face | 0.015625 |
| 30–41 | 12 | 45° diagonal planes through the voxel (wedge slopes), stored with midpoint vertices | 0.088388 |
| 42–57 | 16 | Corner-diagonal hexagons (pyramid / inverted-pyramid slopes) | 0.054127 |

These are the "58 normal bins" of the water kernel (RE-0003). Drag area and centroid are accumulated per surface type.
Splitting faces into quarters and storing midpoints on the slopes makes every edge between neighbouring surfaces match
half-edge to half-edge. That's what lets the shell walk below find closed surfaces.
Normals here are computed by Newell's method from the stored edge order. For faces 0–5 they point **inward**, so the game's
convention is probably the opposite. Confirm before relying on sign.

## 2. Buoyancy shell builder (`0x140489520` → `0x140486db0`, loop `0x140488260`, finish `0x140488d20`)

- Voxels live in a sparse map of 8×8×8 chunks (`sub_1408e1230`, chunk size 0x200). Each voxel carries **two 64-bit masks over surface
  types**: "present" (from the definitions' `buoyancy_surfaces`) and "visited".
- `0x140488260` is one step of an **edge-loop walk**. It takes the current open loop of directed edges, looks up the
  neighbouring surface across the next edge (a per-type adjacency list in the static table), and either cancels matching
  edge pairs (the loop closes locally) or splices the new surface's remaining edges into the loop. Loops are split and
  merged as needed.
- Each newly reached surface is marked visited. Its area is added to the body total (`body+0x798`, the slamming `A_ref` in RE-0003),
  and its area and centroid are written into the octree leaf's per-type bins (`leaf+152+4t`, `leaf+384+12t`).
- *E1-inferred:* a body's buoyancy volume is the region enclosed by a **closed shell** of connected buoyancy surfaces.
  A shell that never closes sets the per-body failure flag that leads to *"Buoyancy Generation Failed … intersecting doors and turret rings"*.
- Still to decode: how the enclosed volume and octree `V_n` weights are computed (`0x140488d20`, `0x140482ed0`, `0x1404914c0`),
  and how `buoy_pipes` (6-bit per-voxel mask) opens a shell to let water in.

## 2b. Buoyancy volume: Z-column scan (`0x140488d20`, E1)

The enclosed volume is **not** a closed-shell integral. The finish step loops over the body bounding box: x (outer), then y,
then **z (inner)**, from z_min − 2 to z_max + 1 voxels. It keeps an "inside" state per column and reads each octree leaf's
per-type area bins:
- the previous voxel has a top face (types 4, 10, 16, 22, 28: full or quarter faces on its +z side) → leave;
- the next voxel has a bottom face (types 5, 11, 17, 23, 29) → enter;
- the current voxel has a slope whose solid side is above (types 30, 34, 35, 38, 42, 43, 48, 49, 52–55) → add ½ voxel and enter.
  If the solid side is below (types 32, 36, 40, 41, 44–47, 50, 51, 56, 57), add ½ voxel and leave;
- a full inside voxel adds 1. For empty inside voxels, the 6 neighbours are checked against a second sparse map, and
  (direction, position) records are emitted. These are probably compartment openings and flood paths.

The bounds come from the surface-insert routine (`0x1404829b0`), which tracks min/max of `vp`-space positions. So the scan
axis is the **vehicle file's z axis**. A hull only needs to be closed along z (fore–aft if z is the vehicle's length). An open
top or sides parallel to z don't break buoyancy. This fits open-top boats floating in game.
Only faces reached by the edge-loop walk (seeded from bottom-facing types) are in the bins, so internal partitions should
not toggle the state (E1-inferred).

**Envelope model (current default, E1-inferred):** the scan counts empty voxels as inside, so cavities must count. The walk
therefore only reaches the outer envelope. The reference now uses *first entry → last exit* per z-column. With that,
6 of 8 workshop boats float at displacement ratios of 0.54–0.82. The other two give 1.15 and 1.39 (hull possibly split across
bodies, or unmodelled mass). The older depth-counting variant, which excludes cavities, is kept as `volume_depth_m3` for comparison.
In-game calibration pack: `tools/fixtures/make_test_pack.py` (solid, mass-scaled, hollow and open-top barges with predictions).

**Earlier status:** `sim-reference/sw_buoyancy.py` implements a geometric z-scan (sub-sampled, depth-counting) plus a
closed-shell check. Unit shapes are exact (block, wedge) or within sampling error (pyramids). On ~30 workshop vehicles, mass
from definitions vs. this volume gives float ratios of 0.5–2.5. That's inconclusive: the classifier shows most samples are
trains or cars, and boats are mixed. **Calibration needs the in-game barge test, or emulating the builder on a real body.**
Possible gaps: walk-reachability (internal faces), the effective component mass model, and multi-body hulls.

## 3. Compartments and flooding (`0x14048d5b0`, E1)

Each body has a ring buffer of compartments (`body+0x848`). Each holds an integer voxel bounding box (scaled by 0.25 m) and a
fluid container at `+0x30`:
- 13 fluid-slot amounts at `+0xc`, capacity at `+0x8`, total at `+0x10`.
- `sub_1402edaf0` sums the **liquid** slots and `sub_1402edb70` the **gas** slots (from jump tables):
  - Liquids: 0 fresh water, 1 diesel, 2 jet fuel, 5 oil, 6 sea water, 8 slurry, 9 saturated slurry
  - Gases: 3 air, 4 exhaust, 7 steam, 10 oxygen, 11 nitrogen, 12 hydrogen
- Force per compartment, applied at the compartment centroid when downward:
```
F = −g · ρ_w · 0.01 · [ water_compartment_force_factor · Liquid
                       + gas_compartment_force_factor · max(Gas − Capacity/60, 0) ] / k_buoy
```
*E1-inferred:* the amounts are litres. A fully flooded 0.25 m voxel (15.625 L) then weighs 15.625 force units,
exactly cancelling that voxel's buoyancy (ρ g V / k_buoy with k_buoy = 10). Water inside the shell acts as ballast.
The meaning of the gas term (gas beyond 1/60 of capacity adds weight) is unclear. It may model compressed gas.

## Implications for KEEL

- **Watertightness check, game-exact:** rebuild the shell walk over `buoyancy_surfaces` using `surface_shapes.json`. If it closes, the
  hull is a sealed buoyancy volume as the game sees it. This replaces the heuristic in plan §25.4 with the real criterion.
- Drag bins, total area and buoyancy volume all come out of the same pass, so KEEL can compute them offline from a vehicle file.
- Flood/ballast calculation: litres in compartments × 1 force unit per litre (at k_buoy = 10), scaled by 10/k_buoy.

## Next

1. Map definition `surface(orientation, rotation, shape)` to sets of the 58 types (compare definition surfaces with the static
   edge table, or find the converter used by `0x140486db0`).
2. Decode the volume/octree weighting and the `buoy_pipes` behaviour.


## 4. Watertightness and flooding: status (2026-11-03)

**In-game behaviour (user, E2):** open-topped boats sink; the hull must be watertight. A door-frame block can make a
"semi-sealed" gap. Definitions confirm the mechanism: `door_frame_straight` has **six full buoyancy faces** (a sealed cube in
buoyancy terms), while e.g. `window_1x1` has only its single panel face.

Static models tried in `sim-reference/sw_buoyancy.py` (all correct on unit boxes: closed box sealed, open-top floods, door frames reseal):
1. `sealed_volume` (voxel-face flood): leaks through voxels split by slopes.
2. `sealed_volume_fine` (sub-voxel wall rasterisation, scipy labelling): ±0.2–2 % on box structures.
3. `game_buoyancy` (game structure: z-scan inside, compartments = empty inside voxels, open = touches outside air).

On 8 workshop boats these models disagree, and most predict sinking. Likely causes: true openings that only flood up to the
external water level (dynamic, not static), multi-body hulls, and components without buoyancy surfaces. **Static flooding is
parked until in-game data** (test pack barges E/F). The dynamic flooding model (water level per compartment, inflow through
openings below the waterline) will be built in `keel-sim`.

`buoy_pipes` (per-voxel 6-bit face mask) appears only on fluid/engine parts (valves = 18 = two opposite faces, many engine
parts = 63). It most likely marks pipe-through faces, not hull openings.


## 5. Compartments and the hole pipeline (from game code, in progress)

- **Debug stages (E1, editor buoyancy view `0x1407a2e40`):** 0 off, 1 cleared stencil, 2 buoyancy hole, 3 cleared holes,
  4 volume uncapped, 5 volume capped, 6 final. The buoyancy volume is built in stages: a voxel stencil, then explicit hole
  detection, then hole clearing, then capping.
- **Compartment creation (builder `0x140489520`, E1):** after the z-scan, opening records come from the scan and from
  components that bound compartments (`def field_ecc ≤ 1 or = 7`, with their door/hatch normal). For each record whose face is not
  sealed, the voxel is looked up in a per-voxel compartment index (`body+0x860`). If absent, a 296-byte compartment is allocated
  (ctor `0x140492810`) into `body+0x848` (the ring the hydro code reads). Its **interior shell** is then traced with the same
  edge-loop walker on a second surface map (`body+0x668`, `0x14048c440`, `0x1404914c0`).
- Components with `compartment_sample_pos` are bound to the compartment containing that point (`comp+0x4a8/0x4b0`, `+0x41c/0x420`).
  Pumps etc. read and write that compartment's fluid. One component kind (`field_ecc == 19`) sets compartment flag `+0x7c`.
- **Still to decode:** hole detection and capping (stages 2–5) and the per-tick sea-water exchange into compartment containers.
  Until then, flooding stays an E1 gap (user: open-top hulls sink unless sealed, e.g. with door frames).


## 6. Watertightness model: findings so far (2026-11-03)

- **Sealing is per face, per voxel** (user, confirmed by definitions): only faces covered by `buoyancy_surfaces` stop water. Occupied
  voxels without surfaces (seats, displays, microcontrollers, pipes) do not. `door_frame_straight` has all six faces → seals a gap.
  Voxel `flags` are collision flags (0 seats/rotors, 4 doors…), not sealing.
- **Structure (E1):** the finish step records an empty inside voxel only where the neighbour already carries a surface on the shared
  face. Those are compartment walls, and they seed an interior shell walk. So watertightness is **shell closure at surface level**:
  - the first-walked (outer) shell defines the z-scan buoyancy volume;
  - other shells are compartments: closed → sealed air (buoyant), open → "buoyancy hole" → cleared;
  - an open-top hull's inner faces join its outer shell over the rim, so the cavity is never counted. That is why open-top boats sink.
- `sw_buoyancy.shell_buoyancy` implements this with edge pairing. All unit cases are exact (closed / open-top / door-frame-capped /
  solid), but real hulls show thousands of open edges at **non-manifold junctions** (block faces meeting 2:1 / 4:1 slope pieces,
  windows, signs), where simple pairing cannot pick the continuation.
- The game resolves junctions with **ordered per-edge candidate lists**, dumped by emulation to
  `research/extracted-metadata/surface_adjacency.json`: 45 lists × 12 candidates `(order, dx, dy, dz, neighbour edge id, neighbour
  surface type)`. Edge ids follow `6·m + face`. **Next:** port `0x140488260` (walk step) and `0x140488d20` (z-scan) exactly on top of
  these lists, then re-run the boats.


## 7. Ported shell walk (`sim-reference/sw_shellwalk.py`)

- Full per-edge candidate tables: **264 edge ids**, 8 or 12 ordered candidates each (the builder loops `edge < 0x108`). An earlier
  dump with only 45 lists was an emulator instruction-cap artefact. Edge-type twins (back faces) exist for all 58 types.
- Port: both-sided surface registration, full-face → quarter normalisation, head-edge expansion into the first present candidate,
  cancellation on revisits. On two adjacent blocks the walk visits exactly the 10 outer faces. All unit cases are exact (block,
  wedge, closed box, open-top box, door-frame cap, solid).
- **On real ships the outer walk reaches interior cabins** (e.g. the Lagoon's cabin sign at y = +5 is on the outer shell), so
  interiors connected to the open air are not counted, and every sample ship is predicted to sink. The open question is the game's
  **"volume uncapped → volume capped"** stage. The physical expectation is that an open compartment still displaces water up to the
  height of its lowest opening. Decoding that stage is next.
- User (E2): paintable sign blocks and paintable indicators are fully sealed. This is consistent with definitions (`sign_na`: six
  buoyancy faces, one written without an `orientation` attribute, which defaults to 0). The walk reaching a sign means the
  surrounding cabin is open to the air, not that the sign leaks.

## 8. Status: pinned as an estimate (2026-11-03, product-owner decision)

KEEL currently treats waterline and draft as an **estimate**. They come from the envelope z-scan (`sw_offline.solve_draft`),
which floats 6 of 8 sample boats plausibly. The exact game model is unfinished and parked: the "volume capped" stage,
compartment seeds and per-face sealing of open interiors are not yet decoded.

Findings recorded before parking:

* **`buoy_radius` / `buoy_factor` / `buoy_force` (definition +0x83c/+0x840/+0x844) are not physics (E1).** The only reader,
  `0x14043e0a8`, adds them to a component position and passes both points to `0x1401be220`, the debug line draw
  (colour 0xffffffff). Almost every definition carries the same 1/1/100, so they are legacy data.
* **Sub-bodies.** Each physics body (pivot, hinge or turret parts) gets its own shell and buoyancy, scaled by its own
  body mass (`k_buoy` = 10 below 2000 kg). Sub-bodies that sit inside a sealed hull are counted twice: once as their own
  shell and again as the hull volume they occupy. This is the user-reported "robotic pivots inside the hull add
  unnatural buoyancy". With the current shell model the effect is small on the sample ships (+2 to +11 kN, 1–5 % of
  weight). The amount for a given vehicle depends on the exact sealing model, so KEEL should flag it as a warning rather
  than rely on it.
* Main-body sealed volumes from the ported shell model still under-float 5 of 7 ships, so that model is not used for
  prediction yet.
