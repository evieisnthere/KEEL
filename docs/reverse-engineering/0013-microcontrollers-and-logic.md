# RE-0013 — Microcontrollers and logic

Status: **file format, full component catalogue and execution order recovered (E1)**. Per-component behaviours: §5.
Game v1.15.26

## 1. File format (inside a vehicle `<c d="microprocessor">` → `<o>` → `microprocessor_definition`)

```
microprocessor_definition name width length id_counter id_counter_node sym0..sym15 (icon bitmap rows)
  nodes/n id component_id
     node label mode(0 = output, 1 = input) type(0 bool, 1 number, 2 composite, 3 video, 4 audio, …) description
       position x z           (the node's cell on the microcontroller footprint)
  group
    data type                (16448 = flags of the root group)
      inputs / outputs
    components/c type=<catalogue id>
      object id [type-specific attributes]
        pos x y              (editor position, 0.25 grid)
        in1..inN component_id=<source object id> [node_index=<output index of the source, default 0>]
        type-specific children (min/max/n/kp/… each with a value `text` or `v`)
    components_bridge/c type=<bridge kind>   (0 bool in, 1 bool out, 2 number in, 3 number out, 4 composite in, 5 composite out, 6 video in, 7 video out, 8 audio in, 9 audio out: E1-inferred from class order)
      object id; in1 (for outputs)
    groups (nested groups, same layout)
```
Wires are stored on the **consumer** side only (`inK component_id`). An input with no wire reads its default (0 / off).

## 2. Component catalogue (E1)
`research/extracted-metadata/microcontroller_components.json` holds all 60 types (id, name, description, input and output
labels). It was dumped by emulating the game's table builder `0x1403698d0`. Ids 0–59 are the `type` attribute.
Attributes seen in the sample corpus:

| id | name | attributes / children |
|---|---|---|
| 0–5 | NOT AND OR XOR NAND NOR | in1, in2 |
| 6–9 | Add Subtract Multiply Divide | in1, in2 (Divide has a second output "Divide By Zero") |
| 10, 36, 45 | f(x,y,z), f(8 inputs), f(x) | `e` = expression |
| 11 | Clamp | min, max |
| 12 | Threshold | min, max |
| 13 | Memory Register | in1 Set, in2 Reset, in3 value, `r` reset value, `memory` |
| 14 | Abs | |
| 15 | Constant Number | `n` |
| 16 | Constant On Signal | |
| 17, 18 | Greater Than, Less Than | |
| 19, 20, 33, 34, 58 | Property Slider/Dropdown/Toggle/Number/Text | values set in the vehicle editor |
| 21, 22 | Numerical Junction / Switchbox | |
| 23, 39 | PID / Advanced PID | kp ki kd (23) or gains as inputs (39) |
| 24, 25 | SR Latch, JK Flip Flop | |
| 26 | Capacitor | `ct` charge time, `dt` discharge time (seconds) |
| 27 | Blinker | `on`, `off` durations |
| 28 | Push To Toggle | |
| 29–32, 40, 41 | Composite read/write (single, and 32-channel `count` `offset`) | |
| 35 | Delta | |
| 37 | Up/Down Counter | `m` mode, `r` reset, `i` increment, min, max |
| 38 | Modulo (fmod) | |
| 42 | Equal | `e` epsilon |
| 43, 44 | Tooltips | |
| 46, 47 | Boolean f(4) / f(8) | `e` expression |
| 48 | Pulse | `m` mode |
| 49–52 | Timers TON/TOF/RTO/RTF | `u` unit (seconds/ticks) |
| 53, 57, 59 | Composite / video / audio switchbox | |
| 54, 55 | Number ↔ composite binary | |
| 56 | Lua Script | `script` (Lua source; `onTick` is called once per tick) |

## 3. Execution order (E1: `0x140396aa0` → `0x14038f810`)

Each tick, for the microcontroller component:
1. **Input nodes read the outside world.** Each bridge input copies the value of the logic link connected to the
   microcontroller (vtable +0x58).
2. Group evaluation (`0x14038f810`, recursive for nested groups):
   a. every bridge input publishes its value as an output;
   b. **every component latches all its inputs**: input value = the source's *current output* (input node vtable +0x60,
      e.g. `0x1403741e0`: `if (link) value = link->out`);
   c. **every component computes** from its latched inputs (component vtable +0x50, e.g. Add `0x140375b60`:
      `out = in1 + in2`);
   d. nested groups are evaluated the same way;
   e. bridge outputs latch and publish.
3. Output nodes write to the outside logic links (vtable +0x48).

Consequences:
* **Every component adds exactly one tick of delay** between its inputs and the components it feeds. Placement order
  does not matter, because all latches happen before any compute.
* A microcontroller input wired straight through one gate to an output gets that gate's result **in the same tick**:
  input bridges publish before the latch, and output bridges latch after the compute. A chain of N gates adds N − 1
  ticks.
* Feedback loops are well defined: each loop iteration takes one tick.
* On spawn the microcontroller waits a few ticks before running (state 1, `field_450/454`). After that, state > 1 means
  disabled: all outputs are cleared to 0 / off.

## 4. Logic links outside microcontrollers
Physical components read their logic inputs in their own tick (for example the rudder reads its input at +0x220). How one
component's write reaches another in the same tick is ordered by the component tick dispatch (RE-0008 §tick dispatch).
KEEL treats each vehicle logic link as a one-tick delay, consistent with the latch/compute model. To be confirmed with
test-pack item L1 (a button → MC → indicator chain, counting ticks with a capacitor).

## 5. Component behaviours (E1, decompiled update functions; t = per tick, no dt scaling)

| Component | Update |
|---|---|
| Add/Sub/Mul | `out = a ∘ b` |
| Divide | `|b| ≤ 1e-6 → out 0, "Divide By Zero" on`; else `a/b` |
| Modulo | `|b| ≤ 1e-6 → 0`; else C `fmod(a, b)` |
| Clamp | `clamp(x, min(lo,hi), max(lo,hi))` |
| Threshold | on if `min(lo,hi) ≤ x ≤ max(lo,hi)` |
| Equal | on if `|a − b| < e` |
| Memory Register | Reset (priority) → value = r; else Set → value = in3 |
| Delta | `x − x_prev` (0 on the first tick) |
| Numerical Junction | switch on → (x, 0) else (0, x) |
| Numerical Switchbox | switch on → in1 else in2 |
| PID / Advanced PID | active: `e = sp − pv; I += e; D = e − e_prev (0 on first active tick); out = kp·e + ki·I + kd·D`; inactive: I = 0, out = 0. **Per tick**, so gains are per-tick values |
| Capacitor | output goes on after `ct·60` consecutive ticks of input on, and off after `dt·60` consecutive ticks of input off (the counters reset when the input changes) |
| Blinker | while on: output on for `on·60` ticks, then off for `off·60` ticks, repeating; control off → counter 0, output off |
| Push To Toggle | toggles on a rising edge |
| Pulse | mode 0: 1-tick pulse on the rising edge; 1: on the falling edge; 2: on any change |
| SR Latch / JK FF | evaluated only when an input changed: R → off, S → on (JK: both → toggle); second output = NOT |
| Up/Down Counter | first tick and Reset → value = r; Up → += i; else Down → −= i; mode 1 clamps to [min, max] |
| Timers TON/TOF/RTO/RTF | enable: `t += (u == 0 ? 1/60 : 1)`, capped at the duration. TON: on when `t ≥ duration`. TOF: on while `t < duration`. Disable resets t (TON/TOF). RTO/RTF hold t until Reset |

Expression components (`e`): operators `+ − * / % ^ !`, constants `pi`, `pi2`, and the functions seen in the corpus:
clamp, len, abs, round, floor, ceil, lerp, atan2, atan, asin, acos, sin, cos, tan, sqrt, min, max, sgn. Boolean
expressions use the same parser with logical operators. Lua Script components are out of scope for the reference
interpreter; KEEL will embed a Lua 5.3-compatible VM later.
