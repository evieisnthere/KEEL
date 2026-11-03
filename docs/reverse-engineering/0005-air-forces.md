# RE-0005 — Air forces (aerodynamic kernel)

Status: **formulas recovered (E1); exponents of two `powf` calls not visible in the decompile** · Game v1.15.26

Code: `0x1404908b0` loops over the 58 surface types per octree cell (cells above the waterline, RE-0003). It calls
`0x140490b50` per type. The constants come from the same context block as water (RE-0003). Field mapping is E1-inferred
from the packing order.

## Inputs
- `u = v_point − w_wind` (wind velocity, context `+0x78..0x80`), `s = |u|`, `û = u/s`, `c = û · n̂`, `A` = bin area.
- `L`: the same per-body length as in water friction.

## Force per surface type
Nothing at all unless `s² > air_speed_threshold²` (15 m/s). Then with **`s′ = s − 15`**:

```
m_hs   = 1 + max(0, s − air_high_speed_threshold(50)) · air_high_speed_factor_scale(8)
F_visc = −û · viscous_air_resistance(0.25) · ρ_air(1.225) · (20 / (s′ · L / 1.81e-5 + 10) + 1.5) · 0.00075 · A · s′²
             · air_drag_factor(0.2) · m_hs
F_p    = −n̂ · c^p_air,p · (air_quadratic_pressure(0.1) · s′² + air_linear_pressure(0.05) · s′) · A        (c > 0)
F_s    = +n̂ · |c|^p_air,s · (air_quadratic_suction(0.05) · s′² + air_linear_suction(0.025) · s′) · A    (c < 0)
F_lift = +n̂ · lift_force(0.25) · (1 − |c|) · clamp(n̂_y, 0, 1) · min(s′², 50²) · A
F_air  = (F_visc + F_p + F_s + F_lift) · a(y)
```
- `p_air,p`, `p_air,s` are presumably `air_falloff_power_pressure/suction` (0.6 / 0.4), by analogy with water. The `powf`
  arguments are dropped in the decompile, so this is unconfirmed.
- `a(y)`: altitude factor. It is 0 above 40 000 m, otherwise `(1 − y · 2.2414e-5)^k` (1/2.2414e-5 ≈ 44 615 m). The exponent
  `k` is not yet recovered.
- The constant 55248.62 = 1/1.81e-5 (dynamic viscosity of air). Water uses 1e6 = 1/ν_water. So `L` is a characteristic
  length in a Reynolds-number term.

## Consequences
- At typical boat speeds (< 15 m/s, about 29 kn) aerodynamic forces on a vehicle are **exactly zero**. Superstructure windage
  doesn't matter for boats until then.
- Lift comes from upward-facing surfaces nearly parallel to the airflow, and it saturates at s′ = 50 m/s.
