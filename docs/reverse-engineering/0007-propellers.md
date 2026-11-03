# RE-0007 — Propellers and rotors (force emitters)

Status: **thrust/load law recovered (E1)**. Several input roles are E1-inferred · Game v1.15.26

Code: `0x140341530` (with chunk `0x14034158c`) is the per-tick force-emitter update. `0x140340570` gives the effective max force.
Definition fields (offsets in the definition struct): `force_emitter_max_force` 0x848, `_rotation_speed` 0x864,
`_blade_physics_length` 0x868, `_blade_efficiency` 0x86c, `_efficiency` 0x870. Flags at `def+0x2b8` select variants
(bit 2 = bladed/pitched, bit 4 = rotor-style; E1-inferred).

## Per tick

```
ω        = shaft speed (RPS) from the torque network, then ω *= 0.9999 (bearing loss)
n        = number of active blades (bitmask popcount)
F_eff    = force_emitter_max_force · (n / 8 if bladed variant else 1)          # 0x140340570
pitch    = clamp(pitch_input + collective, −1, 1)
B        = (2π · ω · rotation_speed · blade_physics_length · pitch · 0.75 + pitch · v_axial)
           · blade_efficiency · n · 0.32                                      # blade "bite"
λ        = clamp(B · 0.5 / (F_eff / 1000), −10, 10);  λ *= 10 if the emitter is below the water surface
absorbed = (1 − 1 / (1 + λ²)) · ω                                             # shaft speed taken by the fluid this tick
ω       -= absorbed
thrust   = absorbed · (F_eff / 1000) · 1500 · force_emitter_efficiency · sign(pitch)   (along the emitter axis)
```
- Thrust is scaled by the altitude factor from RE-0005 (0 above 40 km).
- Rotors also apply cyclic-offset moments (`·800` term), which matters for helicopters but not boat propulsion.

## Interpretation (E1-inferred)
- The propeller is a **speed sink**: each tick it removes `absorbed` RPS from the shaft and converts it to thrust with gain
  `1.5 · F_eff · efficiency` newtons per RPS absorbed.
- `λ` grows with ω, pitch, blade length and blade efficiency. Larger `F_eff` lowers λ (a "stiffer" prop absorbs less per tick but
  gives more thrust per unit absorbed).
- Submersion multiplies λ by 10. An emerged water propeller barely loads the shaft.
- `v_axial` (`+0x26c`) is presumably the inflow speed along the axis. Sign conventions are still to be confirmed.

## Consequences for the powertrain solver
Steady state: per tick, the engine's ω gain (RE-0006: `80 000 · E · eff / I`) equals the propeller's `absorbed` plus losses.
That gives a closed-form operating point per throttle setting, which KEEL can solve numerically for any engine–gearbox–prop
combination, with thrust then balanced against hull drag (RE-0003).
