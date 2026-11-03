"""Reference implementation of Stormworks v1.15.26 water forces, as recovered in RE-0003.

This is an executable specification for the future C++ keel-sim library, not product code.
Every constant comes from research/extracted-metadata/physics_settings_defaults.json (E1).
Inputs still unidentified in RE-0003 (L, A_ref, sigma) are explicit parameters.
"""
from __future__ import annotations
import math
from dataclasses import dataclass

Vec = tuple[float, float, float]


def _dot(a: Vec, b: Vec) -> float: return a[0]*b[0] + a[1]*b[1] + a[2]*b[2]
def _scale(a: Vec, k: float) -> Vec: return (a[0]*k, a[1]*k, a[2]*k)
def _add(*vs: Vec) -> Vec: return tuple(sum(c) for c in zip(*vs))  # type: ignore[return-value]
def _norm(a: Vec) -> float: return math.sqrt(_dot(a, a))
def _clamp(x: float, lo: float, hi: float) -> float: return max(lo, min(hi, x))


@dataclass(frozen=True)
class GameConstants:
    gravity: float = 10.0
    water_density: float = 1000.0
    drag_factor_linear_pressure: float = 15.0
    drag_factor_quadratic_pressure: float = 17.0
    drag_factor_linear_suction: float = 12.0
    drag_factor_quadratic_suction: float = 5.0
    falloff_power_pressure: float = 2.5
    falloff_power_suction: float = 0.3
    water_drag_factor: float = 1.0
    water_drag_weight_factor_multiplier: float = 4.0
    viscous_resistance_factor: float = 0.5
    slamming_force_multiplier: float = 100.0
    wave_velocity_factor: float = 0.5
    wave_velocity_factor_reverse: float = 0.3
    buoyancy_mass_multiplier: float = 10.0
    buoyancy_mass_modifier: float = 10.0
    buoyancy_mass_modifier_lower: float = 2000.0
    buoyancy_mass_modifier_upper: float = 23000.0
    tick_rate: float = 60.0  # E1-inferred from the /60 impulse cap


K = GameConstants()


def mass_ramp(mass: float, k: GameConstants = K) -> float:
    return _clamp((mass - k.buoyancy_mass_modifier_lower) / k.buoyancy_mass_modifier_upper, 0.0, 1.0)


def water_drag_scale(mass: float, k: GameConstants = K) -> float:
    """k_wd. NB: the game hard-codes 2000/23000 here rather than reading the table."""
    return k.water_drag_factor + _clamp((mass - 2000.0) / 23000.0, 0.0, 1.0) * k.water_drag_weight_factor_multiplier


def buoyancy_divisor(mass: float, k: GameConstants = K) -> float:
    """k_buoy: buoyancy is divided by this (10 for light vehicles, 20 at >= 25000)."""
    return k.buoyancy_mass_multiplier + mass_ramp(mass, k) * k.buoyancy_mass_modifier


def buoyancy_force(submerged_volume_m3: float, mass: float, k: GameConstants = K) -> float:
    """Magnitude of the vertical buoyancy force, applied at the submerged-volume centroid."""
    return k.water_density * k.gravity * submerged_volume_m3 / buoyancy_divisor(mass, k)


def cell_fill_fraction(surface_height: float, cell_centre_y: float, half_height: float) -> float:
    return _clamp((surface_height - cell_centre_y + half_height) / (2.0 * half_height), 0.0, 1.0)


def face_bin_force(normal: Vec, area: float, point_velocity: Vec, mass: float, *,
                   water_velocity: Vec = (0.0, 0.0, 0.0), length_L: float = 1.0,
                   k: GameConstants = K) -> Vec:
    """Water force on one normal bin (no slamming; sigma outside (0,1)). Y is up (unconfirmed)."""
    w = _scale(water_velocity, k.wave_velocity_factor)
    if w[1] < 0.0:
        w = (w[0], w[1] * k.wave_velocity_factor_reverse, w[2])
    u = _add(point_velocity, _scale(w, -1.0))
    s = _norm(u)
    if s <= 0.0:
        return (0.0, 0.0, 0.0)
    uh = _scale(u, 1.0 / s)
    c = _dot(uh, normal)
    visc = (k.viscous_resistance_factor * k.water_density
            * (20.0 / (s * length_L * 1e6 + 10.0) + 1.5) * 0.00075
            * area * water_drag_scale(mass, k) * s * s)
    f = _scale(uh, -visc)
    if c > 0.0:
        mag = c ** k.falloff_power_pressure * (k.drag_factor_linear_pressure * s + k.drag_factor_quadratic_pressure * s * s) * area
        f = _add(f, _scale(normal, -mag))
    elif c < 0.0:
        # Sign is E1-inferred (opposes motion); confirm against raw assembly.
        mag = (-c) ** k.falloff_power_suction * (k.drag_factor_linear_suction * s + k.drag_factor_quadratic_suction * s * s) * area
        f = _add(f, _scale(normal, mag))
    cap = s * mass * 0.5 / k.tick_rate
    fm = _norm(f)
    if fm > cap:
        f = _scale(f, cap / fm)
    return f
