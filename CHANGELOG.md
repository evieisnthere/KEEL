# Changelog

## 0.0.1-alpha (first public alpha)

The first release of the KEEL desktop app for Linux and Windows.

### Build mode
- Native vehicle editor with a GPU viewport, real part meshes (optional) and paint colours.
- Part palette by game category, with rendered thumbnails, search, favourites and recently used parts.
- Tools: select, place (with rotation before placing), paint and eyedropper, box fill, measure, and wire.
- Mirror across the centre line for placing, filling and painting. Copy/paste stamps.
- Undo/redo, save and export. Untouched XML is written back byte for byte.
- Scale tool, which warns about sealing.
- Diagnostics, including wires that do not land on a port.

### Hull mode
- Parametric hull with hydrostatics: draft, displacement, block coefficient, KB, BM, KM.
- Bake to smooth blocks. A swept-lattice model is tiled with wedges and pyramids, with no plain corner blocks and nothing pointy.

### Decks mode
- Boat programmes for 8 boat types. Each lists its rooms, counts and areas.
- Deck height selection. The floor plan is a CAD-style view in its own live window.
- Generates:
  - rooms and corridors;
  - walls and doors;
  - block-built furniture;
  - windows and lights, with kelvin colour per ship, room or light, and dimmer lights in dim rooms;
  - room switches through push-to-toggles plus a master control, fed from a battery bus;
  - the superstructure, stairs and ladders.
- Warns when a design exceeds the game's vehicle size limit.

### Power mode
- Modular engine + gearbox + propeller, and electric motor + battery calculators with a design search.
- Not yet calibrated: speeds use a thrust factor until in-game test C1.

### Logic mode
- Microcontroller editor covering all 60 component types:
  - canvas with wiring, box select, pan and zoom;
  - IO nodes and the footprint;
  - settings, with live checking of expressions.
- Tick simulator following the game's latch-then-compute order. It does not run Lua, video or audio.
- Lossless round trip: all 7,616 microcontrollers in the test corpus come back graph-identical.

### Known limits
See `docs/OPEN-ITEMS.md` section D.
