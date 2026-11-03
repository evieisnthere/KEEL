# RE-0012 — Rudders, control fins, control surfaces, keels and wings

Status: **actuator and water-force law recovered (E1)**. The keel righting couple is recovered in structure, with one
input E1-inferred. Game v1.15.26

Class `c_scene_vehicle_component_rudder`, tick `0x1403e2380`. One code path serves three `rudder_type`s:

All 17 parts with definition type 18 run this code:

| rudder_type | Part | area | angle range |
|---|---|---|---|
| 0 | rudder | 13 | ±45° |
| 0 | rudder_surface | 3.2 | ±45° |
| 0 | control_fin_small / medium / large | 2 / 8 / 12 | **±0.25 rad (±14.3°)** |
| 0 | control_surface_small / medium / large | 10 / 28 / 70 | ±45° |
| 1 | wing_small_front / small / medium / large / xl / xxl | 4 / 8 / 20 / 40 / 70 / 120 | ±45° (adds wing lift in air, §5) |
| 2 | keel_small / medium / large (mass 500 / 1000 / 2000) | 13 | ±45° (water force × 40, plus righting couple §3) |

Definition fields: `dynamic_min_rotation` / `dynamic_max_rotation` (±0.785398 = ±45°), `dynamic_rotation_axes`,
`dynamic_side_axis`, `dynamic_body_position`, `rudder_surface_area` (+0xec4), `rudder_type` (+0x2cdc), `mass`.

## 1. Actuator (E1)
```
u = clamp(logic input · (inverted ? −1 : 1), −1, 1)     (0 if the component is disabled)
d = u − s
demand = 0.1·|d|                                        electric (RE-0011)
f = remove_charge(slot, demand)                          # delivered fraction
s += clamp(f / demand, 0, 1) · d
angle = clamp(min + (max − min)·(s + 1)/2, min, max)
```
When powered, the rudder reaches the commanded angle **in the same tick**: f = 1 gives f/demand ≥ 1 for any |d| ≤ 2.
**Without electric charge it does not move** (RE-0011: no battery means factor 0). With infinite electrics it always moves.

## 2. Water force (E1)
```
n   = side axis rotated by `angle` about the rotation axis, in world space
p   = component position + 0.5·dynamic_body_position (flipped by flag bit 2)
sub = clamp(0.5 − 2·(p.y − water_height(p)), 0, 1)       # fully effective 0.25 m below the surface
v   = body linear velocity + ω × (p − body COM)
vn  = clamp(n·v, −50, 50)
A   = clamp(60 · rudder_surface_area, 0.5, 10)          # every stock rudder saturates at 10
m   = min(400, 0.5 · body[+8])                          # body[+8] = body mass (E1-inferred)
F   = −n · vn · m · sub · A          (× 40 for rudder_type 2 / keels)
```
F is applied at p. This is a flat-plate normal force, linear in the normal velocity. For a body heavier than 800 kg it is
`F ≈ −4000 · vn` newton-equivalent per rudder at full submersion. The force acts in the game's impulse units, the same
as RE-0003.

## 3. Keel righting couple (rudder_type 2, E1 structure)
With `a` = the component's local axis in world space, while `a.y ≤ 0` (keel pointing down):
```
k = min(arg6, 4·keel mass)·200·sub          arg6 = tick argument, E1-inferred as the vehicle mass
F₁ = k·(−a.x, −1 − a.y, −a.z)  at p + 2a
F₂ = −F₁                       at p − 2a
```
This couple pulls the keel's axis toward straight down, so it acts as a stabiliser on top of the hull hydrostatics.

## 4. Saturation: all stock parts are equal in water (E1)
`A = clamp(60·area, 0.5, 10)` (constant verified in the disassembly at `0x1403e2a61`). Every stock part has area ≥ 2, so
**in water a small control fin, a rudder and a large control surface give the same force per unit normal velocity**.
They differ only in angle range (fins ±14.3°), mass and where they sit. This is one of the "Stormworks-optimised" effects
that the hull/appendage optimiser's jank mode may exploit. Realistic mode should not.

## 5. Air force (same tick, applied with weight 1 − sub, E1)
```
v_rel = v − wind;  s = |v_rel|
m_a   = min(0.2, body[+8]·3e-5)
F     = −n·(n·v_rel)·m_a·(1 − sub)·max(0, s − 15)                       # nothing below 15 m/s (as RE-0005)
wings (type 1): + lift term 3·clamp((s − 15)/25, 0, 1)·max(0, −(v̂·chord))·… along −n
altitude: for y ≤ 40000, f = pow(1 − y·2.2414e-5, e)   (e = pow() exponent, not yet read)
F    *= area · 24 · f                                                      # area NOT clamped in air
```
In air, area matters linearly.

## 6. KEEL model
`sim-reference/sw_rudder.py`. Open: whether body[+8] is mass or inverse mass. It is checked in game with test-pack
item R3: a rudder on a light vs a heavy barge at equal speed.
