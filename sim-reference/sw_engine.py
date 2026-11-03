"""Reference model of the Stormworks v1.15.26 modular-engine cylinder tick and inertia couplings (RE-0006).
Executable specification; inputs whose game source is not fully identified are explicit parameters."""
from __future__ import annotations
from dataclasses import dataclass

REV_LIMIT_RPS = 60.0


def clamp(x, lo, hi):
    return max(lo, min(hi, x))


@dataclass
class CylinderStep:
    air: float
    fuel: float
    afr: float
    stoich: float
    fires: bool
    efficiency: float
    burn: float
    energy: float
    d_rps: float
    d_manifold_temp: float


def afr_window(manifold_temp_c: float) -> tuple:
    t = clamp(manifold_temp_c / 100.0, 0.0, 1.0)
    return 16.0 + 4.0 * t, 12.0 - 2.0 * t   # (rich bound, lean bound) in game AFR units


def rps_efficiency(rps: float) -> float:
    x = 0.02 * rps
    return clamp(4.0 * (x + 0.5) / (x + 1.0) ** 2 - 1.0, 0.0, 1.0)


def cylinder_tick(rps, throttle, fuel_valve, manifold_temp_c, cyl_size, crank_inertia, heat_capacity, *,
                  burn=1.0, air_available=float('inf'), fuel_available=float('inf'), fuel_pressure=0.0,
                  damaged=False) -> CylinderStep:
    th = 0.0 if rps > REV_LIMIT_RPS else throttle
    air = min(rps * 4e-6 * cyl_size * th, air_available)
    p = clamp(1.0 + 0.01 * fuel_pressure, 1.0, 2.0)
    fuel = min(rps * 2.8e-6 * cyl_size / 60.0 * fuel_valve * p, fuel_available)
    afr = fuel * 600.0 / max(air, 1e-8)
    rich, lean = afr_window(manifold_temp_c)
    raw = (rich - afr) / (rich - lean)
    fires = 0.0 <= raw <= 1.0 and rps > 2.0
    stoich = clamp(raw, 0.0, 1.0)
    eff = rps_efficiency(rps) * (1.0 - 2.0 * (stoich - 0.6) ** 2)
    if fires and air > 0:
        burn = clamp(burn + fuel / (air * 20.0 / 60.0), 0.0, 1.0)
    energy = (1.0 if fires else 0.0) * burn * air
    d_rps = ((40000.0 if damaged else 80000.0) * energy * eff / crank_inertia) if rps > 2.0 else 0.0
    return CylinderStep(air, fuel, afr, stoich, fires, eff, burn, energy, d_rps, energy * 6000.0 / heat_capacity)


def couple(w_a, I_a, w_b, I_b, k=1.0):
    """Momentum-conserving relaxation of two rotating nodes (RE-0006 couplings)."""
    k = clamp(k, 0.0, 1.0)
    s = I_a + I_b
    return w_a + I_b / s * k * (w_b - w_a), w_b + I_a / s * k * (w_a - w_b)
