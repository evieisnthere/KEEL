"""Electrical network reference model (RE-0011, E1)."""
from __future__ import annotations
from dataclasses import dataclass, field

POOL_WINDOW = 10.0
RAMP_TICKS = 300


@dataclass
class Battery:
    capacity: float
    frac: float = 1.0

    @property
    def displayed(self) -> float:
        c = self.capacity
        return max(0.0, min(1.0, self.frac / ((c - POOL_WINDOW) / c))) if c > POOL_WINDOW else self.frac


@dataclass
class Network:
    batteries: list = field(default_factory=list)
    capacity: float = 0.0
    charge: float = 0.0
    infinite: bool = False

    def begin_tick(self):
        self.capacity = self.charge = 0.0
        for b in self.batteries:
            c = min(b.capacity, POOL_WINDOW)
            self.capacity += c
            self.charge += c * b.frac
            b.frac -= c * b.frac / b.capacity

    @property
    def factor(self) -> float:
        return self.charge / self.capacity if self.capacity > 0 else 0.0

    def add(self, a: float):
        if a > 0:
            self.charge += a

    def remove(self, demand: float, ramp: float = 1.0) -> float:
        """Returns the delivered fraction of `demand`. `ramp` = max(0, (300 - slot.ticks)/300) in the game (slot +0x28);
        which event that counter measures is E1-inferred, so KEEL uses the steady state 1.0."""
        if self.infinite:
            return 1.0
        if demand <= 0:
            return 0.0
        r = min(self.charge, ramp * self.factor * demand)
        self.charge -= r
        return r / demand

    def end_tick(self):
        lvl = self.factor
        for b in self.batteries:
            b.frac = max(0.0, min(1.0, b.frac + lvl * min(b.capacity, POOL_WINDOW) / b.capacity))


def motor_tick(omega: float, throttle: float, magnitude: float, max_force: float, net: Network, enabled=True) -> float:
    t = max(-1.0, min(1.0, throttle))
    target = (20.0 if t >= 0 else -20.0) if enabled else 0.0
    a = abs(t)
    d = max(-2 * a, min(2 * a, (target - omega) * a))
    if (t < 0 and d >= 0) or (t >= 0 and d <= 0):
        d = 0.0
    demand = abs(d) * magnitude * max_force * 0.1
    if demand > 0:
        omega = omega + net.remove(demand) * d
    return omega * 0.995


def generator_tick(omega: float, magnitude: float, max_force: float, net: Network, enabled=True):
    w = abs(omega)
    k = min(0.25, w / 560.0)
    produced = magnitude * max_force * 0.1 * w * k if enabled else 0.0
    net.add(produced)
    return (1 - k) * omega, produced
