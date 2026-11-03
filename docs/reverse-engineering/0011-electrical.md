# RE-0011 — Electrical networks, batteries, generators, motors

Status: **network pool, battery exchange, charge factor, motor and generator laws recovered (E1)** · Game v1.15.26

Electric parts are `c_scene_vehicle_component_electric` (tick = vtable slot 107, `0x140403070`). The definition attribute
`electric_type` selects the behaviour. Lights, pumps and other consumers belong to their own component classes, but all of
them draw power through the same slot API.

| electric_type | Parts | Behaviour |
|---|---|---|
| 0 | motor_small/medium/large (and the default for every other definition) | electric motor |
| 1 | generator_small/medium/large, modular-engine alternator | generator |
| 2 | battery_small/medium/large | battery |
| 3 | electric_curcuit_breaker | latching switch |
| 4 | electric_relay | relay |
| 5 | solar, solar_large | solar panel |
| 6 | electric_diode | one-way transfer |

Definition fields used: `electric_charge_capacity` (def+0x1424), `electric_magnitude` (+0x1428), `force_emitter_max_force` (+0x848).

## 1. Electric slot (72-byte connector slot, E1)

| Offset | Meaning |
|---|---|
| +0x20 | link to the network node; the node's +0x20 points to the network **pool** `{capacity, charge}` |
| +0x28 | tick counter used for the 300-tick power-up ramp (E1-inferred: ticks since the slot was (re)connected) |
| +0x38 | slot kind; 1 = storage (battery) |
| +0x3c | **charge factor**: the network pool's charge / capacity, written to every slot each tick |
| +0x40 | battery charge fraction 0–1 |
| +0x44 | battery capacity (`electric_charge_capacity`) |

API (also exposed to Lua as `slotElectric*`, vtable slots 163–165 of the lua component):

```
charge_factor(slot)   = 1                                    if infinite electrics
                      = max(0, (300 - slot.ticks)/300) · slot.factor   otherwise
add_charge(slot, a)   : pool.charge += a              (only if a > 0 and the slot is connected)
remove_charge(slot, d): r = min(pool.charge, charge_factor(slot)·d); pool.charge -= r; return r / d   (0x1402f11b0)
```
`remove_charge` returns the **fraction** of the demand that was delivered. With infinite electrics it returns 1 and takes nothing.

## 2. Network tick (inside the connection solver `0x1404cc680` / `0x1404cd510`, E1)

Networks are flood-filled from the electric nodes each tick. For each network:

```
pool = {capacity: 0, charge: 0}
for each battery slot b:                     # slot kind 1
    c = min(b.capacity, 10)
    pool.capacity += c
    pool.charge   += c · b.frac
    b.frac        -= c · b.frac / b.capacity  # the battery lends up to 10 units to the shared pool
every slot.factor = pool.charge / pool.capacity   (0 when there is no battery)      # 0x1404cc5f8
... components tick: consumers remove_charge, producers add_charge ...
for each battery slot b:                                                             # 0x1404cd510
    b.frac += (pool.charge / pool.capacity) · min(b.capacity, 10) / b.capacity
    b.frac  = clamp(b.frac, 0, 1)
```
Consequences:
* **No battery means no power.** The factor is 0, so consumers receive nothing even while a generator runs.
* Each battery contributes a 10-unit window to the pool. Batteries on one network therefore equalise toward a common
  charge level, at a rate set by 10/capacity.
* The battery's displayed charge (and its logic output) is `frac / ((capacity − 10)/capacity)`, clamped to 0–1.

## 3. Component laws (`0x140403070`, E1)

**Motor (type 0):**
```
t      = clamp(logic input, -1, 1)
target = sign(t) · 20 rps            (0 if the component is disabled/damaged)
Δ      = clamp((target − ω)·|t|, −2|t|, 2|t|);  Δ = 0 if it opposes sign(t)
demand = |Δ| · electric_magnitude · force_emitter_max_force · 0.1
f      = remove_charge(slot, demand)
ω      = (ω + f·Δ) · 0.995
```
The motor is a torque-network slot with mass `force_emitter_max_force` (same coupling as RE-0008). It writes ω and then reads
the network's speed back. The largest step is 2·|t| rps per tick, so torque output is bounded by available charge.

**Generator (type 1):**
```
k        = min(0.25, |ω| / 560)
produced = electric_magnitude · force_emitter_max_force · 0.1 · |ω| · k
ω        = (1 − k) · ω          # load on the shaft
add_charge(slot, produced); displayed output = produced · 60 per second
```
For example, the large generator (125 × 0.85) at 10 rps makes 0.19 units/tick. The cap k = 0.25 is reached at 140 rps.

**Battery (type 2):** charge exchange happens in the network tick (§2). The tick only computes the displayed level. Flag
`spawn_charge` sets frac = 1.

**Circuit breaker (type 3):** the state toggles on the rising edge of its input. While on, the two electric slots share one
network: each slot's link points at the other.

**Relay (type 4):** the same joining, controlled by the logic input.

**Solar (type 5):** the output depends on how much the panel faces the sun (dot product with the sun direction, time of
day × 3600), reduced by (1 − cloud)·(1 − rain) and darkness, scaled by 1e-4. It is added with `add_charge`. The exact
sun-model coefficients are E1 structure only.

**Diode (type 6):** each tick it moves `r = min(poolA, ramp·factorA·min(0.01, 1 − factorB, factorA))` from side A to side B.

**Consumers:** for example the pump (RE-0010) draws `max_force_scale · 0.001` per tick and scales its head by the returned
fraction.

## 4. KEEL model

`sim-reference/sw_electric.py` implements §1–§3 for the 60 Hz simulator. Still open: per-component draw constants for
lights, displays, radars, seats and so on (each class's own tick), and the solar coefficients.
