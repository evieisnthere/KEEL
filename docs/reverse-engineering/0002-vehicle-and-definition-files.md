# RE-0002 — Vehicle XML and component definitions (first pass)

Status: **in progress** · Evidence: E1 (observed in files) unless marked otherwise

## Inputs

- About 70 workshop `vehicle.xml` files (174 to 119,149 components, 1 to 184 bodies), 25 `microcontroller.xml` files, and 2 playlists (`playlist.xml` + `script.lua`).
  All vehicles are `data_version="3"`.
- 759 component definition files from `rom/data/definitions/`.
- Not supplied: `rom/meshes` (too large). Block preview will be built from definition voxels and surfaces instead.

## Definition files

- **They are not well-formed XML.** `<physics_shape_rotation 00="1" 01="0" …/>` uses attribute names that start with a digit.
  557 of 759 files fail a strict parser. KEEL needs a tolerant reader. For analysis we rename `NN=` to `_NN=`
  (`tools/re/defs_load.py`). The exporter must write these names back out exactly as they were.
- Root `<definition>` attributes match the binary schema in RE-0001 F3 (`mass`, `buoy_*`, `engine_max_force`, …).
  Child elements: `surfaces`, `buoyancy_surfaces`, `logic_nodes`, `voxels`, the voxel/physics bounding boxes, and
  `compartment_sample_pos`, plus seat, door, light and connector frames.
- `voxels/voxel` carries `flags`, `physics_shape` and `buoy_pipes`. `buoy_pipes` values range from 0 to 63, with 63 the most common
  non-zero value. *E1-inferred:* it is a **6-bit per-face mask** (one bit per voxel face) that controls flooding/air connectivity between voxels.
  This is central to the flood-integrity work (plan §25).
- `surfaces/surface` (drag/render faces) and `buoyancy_surfaces/surface` each have `orientation` (0–5 = face),
  `rotation`, `shape` (0, 1, 2, 3 and 6 are common, ~60 values in all) and `trans_type` (0/1/2, probably transparency or window glazing).
  *E1-inferred:* the per-surface drag model in RE-0001 F2 runs over these faces.
- Example masses: `01_block` 1.0, `02_wedge` 0.5. Mass units are not yet known. `buoyancy_mass_multiplier = 10` hints at a scale factor (unverified).
- Float blocks (`buoyancy_float_*`) use `buoy_radius 0.75` and `buoy_factor 0.5`. Most parts use 1 / 1 / 100.
- Catalogue extract: `research/extracted-metadata/definitions_catalogue.json` (generated locally and **gitignored**, because
  it is derived from game assets; regenerate it from the user's own install).

## Vehicle XML (`data_version="3"`)

```
vehicle[data_version, bodies_id]
  authors/author[steam_id, username]
  bodies/body[unique_id]/components/c[d=definition (absent => 01_block), t=?]/o[...]
      o: r="9-int rotation matrix", bc/ac/bc2/bc3 colours (hex), sc="surface colour spec", per-type properties
      o/vp[x,y,z]  voxel position (integers, 0.25 m grid; missing axis = 0)
      o/logic_slots/slot[editor_connected]
      o/microprocessor_definition[...]/nodes, group/components, components_bridge   (embedded MC)
  logic_node_links/logic_node_link[type]/voxel_pos_0, voxel_pos_1   (connections are by voxel position)
```

- Of 561 distinct component IDs used, all but 15 resolve to a definition. The 15 are 32-hex-digit hashes,
  probably modded or custom components. They must be **preserved untouched**.
- Connections are stored as pairs of voxel positions, not component IDs. So moving a component means rewriting its links.
- Still to map: `t` (looks like a shape/transform variant, 0–4 seen), the `sc` format, multi-body joints, and every `o` property.

## Next

1. Coordinate convention: compare `vp` and `r` against definition voxel extents on a few known shapes.
2. Lossless round-trip: parse → write all ~95 files and compare semantically (attribute order and whitespace may differ).
3. Compute mass, COM and buoyancy volume for each vehicle from the definitions. This is the input to S0 hydrostatics.
