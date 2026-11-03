# RE-0006 — Modular engine (cylinder combustion and rotating couplings)

Status: **core cylinder equations recovered (E1)**. Several inputs are named only by role (marked ?) · Game v1.15.26

Code: `0x14044bef0` is the per-module tick, a `switch` on the definition's `engine_module_type`.
Tooltip `0x14044e5f0` confirms the meaning of the stored outputs.

## Module types (definition `engine_module_type`, E1)

| Type | Parts | Role in the tick |
|---|---|---|
| 1, 22, 27 | cylinder 1×1, piston 3×3, piston 5×5 | combustion (below) |
| 3, 21, 26 | clutch 1×1 / 3×3 / 5×5 | rate-limited engagement (±0.1 per tick) of a coupling |
| 4 | drive belt | coupling |
| 5 | alternator | inertia coupling + electrical output |
| 6 | fluid pump | inertia coupling + pump |
| 7 | starter | adds spin while crank < 5 RPS |
| 8, 9, 10, 11, 28 | coolant / intake / exhaust / air manifolds | network bookkeeping |
| 0, 18, 19, 20, 25 | crankshafts, flywheels, gearboxes, converters | rotating inertia (no tick logic here) |

`engine_max_force` acts as **rotational inertia** in the couplings (flywheels: 150 / 600 / 2400; crankshafts 0.7 / 15 / 25).

## Cylinder tick (types 1/22/27)

Inputs per tick: crank speed `ω` (RPS, crank node `+0x254`), throttle `θ` (engine object `+0x88`), fuel valve `φ` (?) (`+0x90`),
manifold temperature `T_m` °C (`+0xec`), cylinder size `V_c` (definition `+0xb8`, exact attribute TBD), crank inertia `I` (crank node `+0x258`/`+0x600`).

```
t       = clamp(T_m / 100, 0, 1)                      # engine warmth
θ_eff   = 0 if ω > 60 (rev limit, 3600 RPM) or while the start-up/damage counter blocks it, else θ
air     = min(ω · 4e-6 · V_c · θ_eff,  air available in intake network)
p       = clamp(1 + 0.01 · fuel_pressure, 1, 2)       # fuel_pressure from connected fuel network (averaged)
fuel    = min(ω · 2.8e-6 · V_c / 60 · φ · p,  fuel available)
AFR     = fuel · 600 / max(air, 1e-8)                  → stored +0x644, shown as "Air:Fuel Ratio"
rich    = 16 + 4t ;  lean = 12 − 2t                    # window narrows/shifts as the engine warms
stoich  = clamp((rich − AFR) / (rich − lean), 0, 1)    → +0x648, shown as 2·stoich − 1
fires   = (0 ≤ (rich − AFR)/(rich − lean) ≤ 1) and ω > 2
mix_eff = 1 − 2 · (stoich − 0.6)²                      # best at stoich = 0.6
rps_eff = clamp(4 · (0.02ω + 0.5) / (0.02ω + 1)² − 1, 0, 1)   # 1.0 at 0 RPS, 0.5 at 50 RPS, 0 at ≈ 120.7 RPS
eff     = rps_eff · mix_eff                            → +0x64c, "Efficiency"
burn    = clamp(burn + fuel_flow / (air·20/60), 0, 1)  # combustion state, builds up while fuel flows
E       = fires · burn · air
T_m    += E · 6000 / C_heat                            # C_heat = engine object +0xf0
ω      += 80 000 · E · eff / I     (40 000 if damaged; only while ω > 2 and damage < 20)
```
Above 115 °C the engine starts an overheat damage timer (period 300 ticks).

The "Air:Fuel Ratio" the game shows is `600 · fuel / air` in its internal units, so it is not a real-world AFR.

## Couplings (types 4, 5, 6, 7, inertia exchange, E1)

Two rotating nodes with inertias `I_a` (crank side) and `I_b` (`engine_max_force` of the part) and speeds `ω_a`, `ω_b` relax toward
each other by `k = clamp(input, 0, 1)` (e.g. a clutch's engagement):
```
ω_a' = ω_a + I_b/(I_a + I_b) · k · (ω_b − ω_a)
ω_b' = ω_b + I_a/(I_a + I_b) · k · (ω_a − ω_b)          (momentum-conserving)
```
- Alternator (5): output `∝ ω · min(0.25, |ω|/560)`, and `ω_b *= (1 − min(0.25, |ω|/560))`.
- Starter (7): while powered and ω < 5, `ω_b += (5 − ω) · electric_charge_capacity · engine_max_force · 0.1`. Also ω_b decays 1% per tick.
- Clutch (3/21/26): engagement follows its input with a slew limit of ±0.1 per tick.

## Gaps
- Exact definition attribute behind `V_c` (`def+0xb8`, a double) and the crank inertia aggregation (`+0x600`).
- The fuel-network pressure term, the exhaust fraction (`+0x650`), and coolant scaling (`+0xe4`) dynamics. They are written in other module types and network code.
- Torque/RPS transfer through gearboxes and to propellers (the torque network solver).
