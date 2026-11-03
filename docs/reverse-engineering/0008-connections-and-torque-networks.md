# RE-0008 — Connections and torque networks

Status: **network solver and connection rules recovered (E1)**. Gearbox accumulator consumption is E1-inferred · Game v1.15.26

## 1. Ports (definition `logic_nodes/logic_node`, E1)

Attributes: `label`, `description`, `position` (voxel within the component), `orientation` (face 0–5 = +X −X +Y −Y +Z −Z, same
order as the surface types in RE-0004), `mode` (0 = output, 1 = input), `type`:

| type | Kind | How it connects |
|---|---|---|
| 0 | on/off (bool) | wired: `logic_node_links` type 0 |
| 1 | number (f32) | wired: link type 1 |
| 2 | **torque / RPS** | **physical**: never in `logic_node_links` (0 of ~27 000 links in 40 vehicles) |
| 3 | **fluid** | **physical** |
| 4 | electric | wired: link type 4 |
| 5 | composite | wired: link type 5 |
| 6 | video | wired: link type 6 |
| 7 | audio | wired: link type 7 |
| 8 | ammo belt | wired: link type 8 |

Wired links (`vehicle/logic_node_links/logic_node_link[type]`) name their two endpoints by **voxel position** (`voxel_pos_0`,
`voxel_pos_1`). The ports at those voxels with matching type are connected.

## 2. Physical connections: torque and fluid (builder `0x1404a3890`, E1)

- The builder iterates network kinds. For each component it takes ports whose type is physical and whose kind matches, then
  transforms position and orientation by the component's rotation.
- **Two ports connect when each sits in the voxel the other faces:** `p_a + d_a = p_b` and `d_b = −d_a`. This is checked with
  rotated integer vectors (orientation switch: 1 → −X, 2 → +Y, 3 → −Y, 4 → +Z, 5 → −Z, otherwise +X).
- **Pipes** (`trans_*` definitions, definition type 6: "Pipe Straight / Angle / T / Cross / Omni", enclosed variants) have no
  ports. Their **surfaces with `trans_type=1`** are open pipe ends, and they act as conduits. Component surfaces with `trans_type=2`
  mark fluid connection faces on parts such as filters, valves and manifolds.
- **Pipes carry mechanical power as well as fluid** (E1 from data). On the "ALTA Tug100" (3698852022), propeller RPS ports face
  `trans_block_angle` / `trans_block_straight`, and modular gearbox RPS ports face `trans_straight`, `trans_cross`, `trans_t_corner`
  and `torque_clutch`. Each kind forms its own networks.
- Network objects: torque network = 48 bytes `{f32 rps, f32 weight(=1), f32 1.0, slot list}`. Fluid network = 128 bytes
  (initialised with `+8 = 4.0`, `+0x44 = 20.0` °C, slot list at `+0x68`). A slot points to its network at `slot+0x20`.

## 3. Torque slot (`c_vehicle_torque_slot`, serialiser `0x1402f0c50`, E1)

| Offset | Field | Meaning |
|---|---|---|
| +0x08 | editor_connected | |
| +0x20 | network* | |
| +0x28 | mass | how strongly this part imposes its speed (weight) |
| +0x2c | demand | the RPS this part wants this tick |
| +0x30 | flow | network RPS written back by the solver |
| +0x38 | partner* | slot on the other side of a gearbox/clutch |
| +0x40 | factor | coupling (clutch engagement, gearbox 1.0) |
| +0x44 | ratio | gear ratio for this side (partner holds 1/ratio) |

## 4. Network solver (`0x1404cc680`, once per tick, E1)

For every torque network N:
```
W_N   = 1 + Σ massᵢ
rps_N = (rps_N · 1 + Σ massᵢ · demandᵢ) / W_N            → each slot.flow = rps_N
```
Then propagate through slots with a partner (gearboxes, clutches), breadth-first, starting with `f = slot.factor · 0.95`,
`r = slot.ratio`. For each reached network M:
```
M.acc_a += sign(r) · rps_N · W_N · f
M.acc_b += W_N · f / |r|
```
This pulls M toward `r · rps_N` with strength `W_N · f`. The walk continues through M's partner slots with `f ← f · factor · 0.95` and
`r ← r · ratio`, and stops when `f ≤ 0.001`. If a loop returns to a visited network with a ratio outside ×0.9–1.1 of the earlier visit
(inconsistent gearing), the involved networks' accumulators are reset rather than fighting.
*E1-inferred:* the accumulators act on M on the next solve. Each gear stage passes about 95 % of the coupling strength.

The fluid part of the same solver balances supply (+) and demand (−) slot values against network capacity and current volume,
then hands out flow pro rata (RE-0010).

## 5. Component rules feeding the solver (E1)

| Component | Slot mass | Slot demand | Other |
|---|---|---|---|
| Propeller / rotor (RE-0007) | `F_eff / 1000` | `ω − absorbed` | reads `flow` as next ω |
| Modular clutch (types 3/21/26) | `e · I_crank` | `ω_crank` | crank: `ω_c += e·(ω_net − ω_c)`. Engagement e slews ±0.1/tick toward clamp(input) |
| Torque clutch (`torque_clutch`) | — | — | factor = pressure² × electric supply; written to both slots |
| Gearbox (`torque_gearbox*`, `modular_engine_gearbox_*`) | — | — | factor 1; ratio from `e_gear_ratio` (below); gear changes need electric power |
| Torque meter | — | — | reads RPS / torque |
| Crank handle (`torque_crank`) | — | — | manual input |

`e_gear_ratio` (`0x1403fe040`), output/input ratio by index:

| index | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|---|
| ratio | −1 (reverse) | 1:1 | 6:5 (0.833) | 3:2 (0.667) | 9:5 (0.556) | 2:1 (0.5) | 5:2 (0.4) | 3:1 (0.333) |

Defaults: `gear_ratio_1 = 1` (1:1), `gear_ratio_2 = 5` (2:1). The "Gear Switch" bool selects between them.

## 6. Component tick dispatch (E1)

Each `c_scene_vehicle_component_*` class overrides **vtable slot 107** (per-tick update) and sometimes 108 (second phase).
Examples: modular engine 107 = `0x14044bef0` (RE-0006), 108 = `0x14044db40`. Torque parts 107 = `0x1403fd610`.
Pump 107 = `0x1403ff670`. Electric 107 = `0x140403070`. Legacy engine 107 = `0x14034df80`. Force emitter 108 = `0x140341530` (RE-0007).
