# RE-0010 — Fluid volumes, pressure, transfer, pumps

Status: **pressure law, transfer law and pump head recovered (E1)** · Game v1.15.26

## Fluid volume (shared by pipe networks, tanks, compartments, engine manifolds)

| Offset | Field |
|---|---|
| +0x08 | capacity C (litres) |
| +0x0c … +0x3c | amount per fluid type 0–12 (litres): 0 fresh water, 1 diesel, 2 jet fuel, 3 air, 4 exhaust, 5 oil, 6 sea water, 7 steam, 8 slurry, 9 saturated slurry, 10 oxygen, 11 nitrogen, 12 hydrogen |
| +0x40 | total (`vol_total`) |
| +0x44 | temperature °C (`temp`, new networks start at 20) |
| +0x48 | radiation (`rad`) |

Liquids: types 0, 1, 2, 5, 6, 8, 9. Gases: 3, 4, 7, 10, 11, 12 (jump tables `0x1402edb38` / `0x1402edbb8`). Volumes up to 8 L behave as
small pipes; larger ones as tanks.

## Pressure (`0x1402eda10` / `0x1402ed970`, E1), scale 0–60

```
C ≤ 8 :  P = 60 · (L + G) / C
C > 8 :  P = 60/8 · [ G · min(1, 8 / (C − L))  +  max( 4L / (60C),  L − (C − 8) ) ]
```
L = liquid litres, G = gas litres. Gas pressurises as it is squeezed into the free space. Liquid adds only a small head until it is
within 8 L of full, then pressure rises one-for-one per litre. The game's pressure sensors read this 0–60 scale.

## Transfer between two volumes through a pair of slots (`0x1402efde0`, E1)

Arguments: slot A (source side), slot B, head `h` (pump pressure), valve `k` (0–1), `one_way`, fluid-type mask, output record.
```
c_a = min(C_a, 8), c_b = min(C_b, 8)
Δ   = P_a − P_b + h            (one_way: Δ = max(Δ, 0))
if Δ > 0:  P* = clamp((P_b·c_b + P_a·c_a)/(c_a + c_b) − h,  P_a − 20, P_a + 20) clamped to [0, 60]
           q  = (1 − P*/P_a) · V_a          limited by the space left in B
if Δ < 0:  the mirror image toward A
slot_A.flow = q, slot_B.flow = −q
moved = agreement of last tick's demands (slot +0x2c, from the network solver's supply/demand balancing),
        clamped to ±|q · k · 0.5|
```
The moved litres are taken from A's volume per fluid type, in proportion to its contents and restricted to the mask, and added to B.
B's temperature and radiation become amount-weighted averages. Totals are recomputed.

The network solver (`0x1404cc680`, RE-0008 part 1) first sums every volume's positive and negative slot flows and scales them so a
volume can neither give more than it holds nor take more than its free capacity. That produces the demands used above.

## Pumps (`c_scene_vehicle_component_pump`, tick `0x1403ff670`, E1)

```
power  = electric supply factor (1 with infinite electrics), ramping in over 300 ticks after power is restored
head   = pump_pressure · max_force_scale · power      (pump_pressure: Fluid Pump 10, Large Fluid Pump 100, Manual 3,
                                                          modular-engine fluid pump 0.01; max_force_scale = in-game slider)
transfer(inlet → outlet, head, valve 1, one_way = true, all fluid types)
electric draw = max_force_scale · 0.001 per tick
flow output (L/s) = slot flow · 60
```
When switched off the pump still calls the transfer with head 0, so it acts as an open one-way pipe.

## Flooding

Compartments own the same fluid-volume struct (RE-0004 §5), so sea water entering a compartment and pumps removing it use these
same laws. The opening exchange (`0x14049f440`) samples the sea at both ends of each opening and picks the higher-pressure
neighbour. The hole/capping stages of the buoyancy build are still being decoded (RE-0004 §5).
