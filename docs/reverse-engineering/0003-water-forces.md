# RE-0003 — Water forces (hydrodynamic kernel)

Status: **formulas recovered (E1); a few inputs still unidentified** · Game v1.15.26 · `stormworks64.exe` sha256 `c38c03ff…`

## Where the code lives

| Address | Role |
|---|---|
| `0x140296940` | Accessor for the game-constants object. The physics table (RE-0001 F2) sits at `+0x5c` |
| `0x14084ad60` | Loads `data/game_constants.xml` into that table. **The file is not shipped** in `rom/data` (user's listing), so the compiled defaults apply |
| `0x14048d5b0` | Per-body hydro/aero update. Packs constants into a context block, applies the mass scaling, handles compartments |
| `0x1409465a0` | Recursive octree walk over the body's surface cells. Calls the water (`0x14048fa90`) or air (`0x1404908b0`) kernel per cell |
| `0x14048fa90` | Water kernel: loops over **58 fixed face-normal bins** per cell, each with an area and a centroid |
| `0x14048fd50` | Water force for one normal bin: the formulas below |
| `0x1409bdc50` | CRT `powf` (identified by its IEEE-754 special-case structure; angr drops its arguments) |

## Inputs per normal bin

- `n̂`: the bin's unit normal (from a table of 58 normals via `sub_1402f1ce0(i)`, body frame rotated to world).
- `A`: the summed face area in the bin. `r`: the centroid relative to the centre of mass.
- `v_p = v + ω × r`: the point velocity.
- `w`: the water particle velocity at the point. It is scaled by `wave_velocity_factor` (0.5), and its vertical component is multiplied
  again by `wave_velocity_factor_reverse` (0.3) when that component is negative (E1; the axis is taken to be Y, unconfirmed).
- `u = v_p − w'`, `s = |u|`, `û = u/s`, `c = û · n̂`.
- `M`: the body mass (`body+0x948`). `σ`: a cell submersion value. Slamming and debug output only run when `0 < σ < 1`.

## Mass-scaled factors (in `0x14048d5b0`)

```
k_wd   = water_drag_factor + clamp((M − 2000) / 23000, 0, 1) · water_drag_weight_factor_multiplier     → 1 … 5
k_buoy = buoyancy_mass_multiplier + clamp((M − buoyancy_mass_modifier_lower) / buoyancy_mass_modifier_upper, 0, 1)
                                  · buoyancy_mass_modifier                                           → 10 … 20
```
Note that the divisor is `upper` (23000), not `upper − lower`, so the ramp saturates at M = 25000.
How `k_buoy` is used is still being traced (it divides the compartment forces; see the Open items).

## Forces per normal bin (in `0x14048fd50`)

**1. Viscous / skin friction** (always applied when s > 0):
```
F_visc = −û · viscous_resistance_factor · ρ_w · (20 / (s · L · 1e6 + 10) + 1.5) · 0.00075 · A · k_wd · s²
```
`L` is a per-body scalar (context `+0x94`) that is not yet identified. It acts like a Reynolds-number length, and at high speed the bracket tends to 1.5.

**2. Pressure face** (`c > 0`, the face moving into the water):
```
F_p = −n̂ · c^falloff_power_pressure · (drag_factor_linear_pressure · s + drag_factor_quadratic_pressure · s²) · A
    = −n̂ · c^2.5 · (15 s + 17 s²) · A
```

**3. Suction face** (`c < 0`, the face moving away):
```
F_s = −n̂ · (−c)^falloff_power_suction · (drag_factor_linear_suction · s + drag_factor_quadratic_suction · s²) · A · sign
    = … (−c)^0.3 · (12 s + 5 s²) · A
```
The sign convention is E1-inferred (the force opposes motion). It still needs confirming against the raw assembly.

**4. Slamming** (pressure faces only, `0 < σ < 1`, approaching the surface downward with speed `v_d > 0`):
```
F_slam = d̂ · (n̂ · −d̂) · (min(v_d, 5) / 5)² · M · (2A / A_ref) · slamming_force_multiplier(100)
```
`A_ref` is the body field `+0x798`, not yet identified. It may be a total or waterplane area.

**5. Per-tick impulse cap:**
```
|F_visc + F_p + F_s + F_slam| ≤ s · M · 0.5 / 60
```
The `/60` strongly suggests a **60 Hz physics step** (E1-inferred). The cap stops water drag from removing more than half the
relative velocity in one tick. At low speed it often dominates.

**Not used here:** `water_density` only scales the viscous term. Buoyancy is a separate path (see the Open items).

## Buoyancy (E1)

`0x140946220` walks the body's buoyancy octree. Each node has a centre, a half-height `h` and a volume weight `V_n`:
- If the node top is more than 100 m below the lowest wave, it counts fully. If its bottom is above the highest wave, it counts zero.
- At a leaf (or at maximum depth) it reads the local surface height η and adds `f · V_n` with `f = clamp((η − y + h) / (2h), 0, 1)`.
  It also accumulates the volume-weighted centroid.

The parent (`0x14048d5b0`) then applies one vertical force at the submerged centroid:
```
F_b = ρ_w · g · V_sub / k_buoy        k_buoy = 10 … 20 (mass-scaled, above)
```
The octree depth starts at 10 levels and drops one level each time the largest extent is halved until it fits within 8 m.
Sanity check (E1-inferred): with definition masses (`01_block` = 1) and the solid voxel volume alone, 40 workshop vehicles
come out at mass/buoyancy ratios of 0.6–1.8. The units are consistent, and **enclosed air must count toward V_sub**,
otherwise many working boats would sink. The generator that builds this volume from `buoyancy_surfaces`/`buoy_pipes` is the next target.

## Load-time validation (`0x1405e2210`, E1)

- The vehicle extent must fit one of these boxes (m): 37.5 × 11 × 36, 23.5 × 21.5 × 58.25 or 23.5 × 58.25 × 23.5
  (axis order to be confirmed). Otherwise: *"created in a modified version of the game"*.
- Warnings: more than 50 bodies, more than 2000 physics shapes, more than 50 lights, more than 30 monitors, a file over 10 MB (over 14 MB may fail to load).
- *"Buoyancy Generation Failed … intersecting doors and turret rings"*: a per-body flag set by the generator.

## Consequences for KEEL

- Resistance depends on **the faces, their orientation and their speed**, not on hull-form coefficients.
  KEEL uses this model to **predict** in-game performance. Hull optimisers do **not** optimise against it: they target realistic
  hulls (see the simulation-plan hull optimiser policy), and the Stormworks prediction is shown alongside.
- Mass above 2 t raises skin friction up to 5×.
- Speed is capped by the per-tick impulse limit, so the simulator must run at the game's 60 Hz step to match.

## Open items

1. Identify `L` (context `+0x94`), `A_ref` (`+0x798`) and `σ` (cell submersion).
2. Buoyancy: the compartment loop in `0x14048d5b0` uses `water/gas_compartment_force_factor · ρ · g² · 0.01 / k_buoy`.
   Decode it fully along with `0x140946220` (a sibling octree walk, probably the buoyancy volume).
3. Air kernel `0x1404908b0` / `0x140490b50` (air drag with thresholds, `lift_force`, `viscous_air_resistance`).
4. Confirm the 58-normal table (`sub_1402f1ce0`) and how definition `surfaces` map into the bins.
