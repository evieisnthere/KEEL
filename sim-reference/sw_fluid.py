"""Reference fluid model (RE-0010): volumes, pressure, two-volume transfer, pumps."""
from __future__ import annotations
from dataclasses import dataclass, field

LIQUIDS = {0, 1, 2, 5, 6, 8, 9}
GASES = {3, 4, 7, 10, 11, 12}
NAMES = ['fresh water', 'diesel', 'jet fuel', 'air', 'exhaust', 'oil', 'sea water', 'steam', 'slurry',
         'saturated slurry', 'oxygen', 'nitrogen', 'hydrogen']
PUMP_PRESSURE = {'water_pump': 10.0, 'water_pump_large': 100.0, 'water_pump_manual': 3.0, 'modular_engine_fluid_pump': 0.01}


@dataclass
class Volume:
    capacity: float
    amounts: list = field(default_factory=lambda: [0.0] * 13)
    temp: float = 20.0

    @property
    def liquid(self): return sum(self.amounts[i] for i in LIQUIDS)
    @property
    def gas(self): return sum(self.amounts[i] for i in GASES)
    @property
    def total(self): return sum(self.amounts)

    def pressure(self) -> float:
        C, L, G = self.capacity, self.liquid, self.gas
        if C <= 8.0:
            return 60.0 * (L + G) / C if C > 0 else 0.0
        free = C - L
        g = G * min(1.0, 8.0 / free) if free > 0 else G
        return 60.0 / 8.0 * (g + max(4.0 * L / (60.0 * C), L - (C - 8.0)))


def transfer(a: Volume, b: Volume, head=0.0, valve=1.0, one_way=False, mask=0x1FFF, demand=None) -> float:
    """One tick of flow a→b (positive) following 0x1402efde0. `demand` (agreed amount from the network solver) defaults to
    the computed q, i.e. an unconstrained two-volume system. Returns litres moved a→b."""
    pa, pb = a.pressure(), b.pressure()
    ca, cb = min(a.capacity, 8.0), min(b.capacity, 8.0)
    d = pa - pb + head
    if one_way:
        d = max(d, 0.0)
    q = 0.0
    if d > 0 and pa > 0:
        ps = (pb * cb + pa * ca) / (ca + cb) - head
        ps = min(max(ps, pa - 20.0), pa + 20.0); ps = min(max(ps, 0.0), 60.0)
        q = min((1.0 - ps / pa) * a.total, b.capacity - b.total)
    elif d < 0 and pb > 0:
        ps = (pb * cb + pa * ca) / (ca + cb) + head
        ps = min(max(ps, pb - 20.0), pb + 20.0); ps = min(max(ps, 0.0), 60.0)
        q = -min((1.0 - ps / pb) * b.total, a.capacity - a.total)
    moved = q if demand is None else demand
    cap = abs(q * valve * 0.5)
    moved = max(-cap, min(cap, moved))
    src, dst, m = (a, b, moved) if moved > 0 else (b, a, -moved)
    tot = src.total
    if m <= 0 or tot <= 0:
        return 0.0
    frac = min(1.0, m / tot); moved_total = 0.0
    for i in range(13):
        if mask & (1 << i):
            x = src.amounts[i] * frac
            src.amounts[i] = max(0.0, src.amounts[i] - x); dst.amounts[i] += x; moved_total += x
    if dst.total > 1e-3:
        dst.temp = (dst.temp * (dst.total - moved_total) + src.temp * moved_total) / dst.total
    return moved if moved > 0 else -moved_total


def pump_head(definition: str, scale: float = 1.0, power: float = 1.0) -> float:
    return PUMP_PRESSURE.get(definition, 0.0) * scale * power
