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
