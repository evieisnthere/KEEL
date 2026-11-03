"""Rudder / keel reference model (RE-0012)."""
from __future__ import annotations
import math


def actuator(s: float, command: float, delivered_fraction: float = 1.0, inverted=False) -> float:
    u = max(-1.0, min(1.0, command * (-1 if inverted else 1)))
    d = u - s
    demand = 0.1 * abs(d)
    if demand <= 0:
        return s
    return s + max(0.0, min(1.0, delivered_fraction / demand)) * d


def angle(s: float, lo=-0.785398, hi=0.785398) -> float:
    return max(lo, min(hi, lo + (hi - lo) * (s + 1) / 2))


def submersion(p_y: float, water_y: float) -> float:
    return max(0.0, min(1.0, 0.5 - 2.0 * (p_y - water_y)))


def water_force(n, v, body_mass: float, area: float, sub: float, keel=False):
    vn = max(-50.0, min(50.0, sum(n[i] * v[i] for i in range(3))))
    A = max(0.5, min(10.0, 60.0 * area))
    m = min(400.0, 0.5 * body_mass)
    k = -vn * m * sub * A * (40.0 if keel else 1.0)
    return tuple(k * n[i] for i in range(3))


def rotate(v, axis, a):
    """Rodrigues rotation of v about unit axis by angle a."""
    c, s = math.cos(a), math.sin(a)
    d = sum(v[i] * axis[i] for i in range(3))
    cr = (axis[1] * v[2] - axis[2] * v[1], axis[2] * v[0] - axis[0] * v[2], axis[0] * v[1] - axis[1] * v[0])
    return tuple(v[i] * c + cr[i] * s + axis[i] * d * (1 - c) for i in range(3))
