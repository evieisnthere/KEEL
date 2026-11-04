# Hull → blocks: design

Status: **approved direction (2026-11-04): smoothness over fidelity**. Phase P1 in progress. It replaces the ad-hoc voxel-fitting attempts
(`voxel.rs`, `blockhull.rs`, `facets.rs`). Those stay only as references until this design lands.

## 1. What "good" means (measurable)

A generated hull must be:

| # | Criterion | Metric (checked automatically) |
|---|---|---|
| Q1 | **Watertight**: no holes in the skin or the deck | every exposed face of the block shell is backed by a solid. Hole count = 0 |
| Q2 | **No shingles on smooth surfaces**: where the NURBS curves gently, adjacent sections produce one continuous planar face, not 1-voxel jumps | area of exposed *step faces*, i.e. faces whose normal is > 25° off the local NURBS normal, as a share of hull area. Target < 3 % outside the bow and stern zones |
| Q3 | **Close enough**: follows the NURBS, but smoothness wins (product-owner decision) | max and RMS distance in voxels, reported. Soft target: max ≤ 1.5, RMS ≤ 0.6 |
| Q4 | **Planar facets meet edge to edge**: no T-gaps or slivers | every facet edge is shared by exactly two facets or lies on the hull boundary |
| Q5 | **Standard blocks only**, placeable in the editor | definitions 01–16, proper or mirrored rotations. Mirrored rotations are what the editor's symmetry tool produces, and the game's own default rotation is a mirror |
| Q6 | **Real deck** following the sheer | deck plating present in every hull column; the sheer is built from 1:4 / 1:2 ramps |

Every change is judged by these numbers plus a fixed set of flat-colour renders (six views, same cameras), not by eye
on one view.

## 2. The block surface is a lattice polyhedron

Every vertex of every generated block lies on the voxel-corner lattice. A block hull is therefore a closed polyhedral
surface on the lattice, and each planar facet of it must be **tileable** by blocks.

### 2.1 Tileable facets (exact)

Write the facet as a height field: height h over two in-plane axes (u, v), in voxel units, with gradient
(g_u, g_v) = (∂h/∂u, ∂h/∂v).

* **One-direction facets** (g, 0): tiled by wedge boxes of size (rise 1) × (run L).
  * |g| = 1/4 → 1×4 wedge; 1/2 → 1×2 wedge; 1 → 1×1 wedge.
  * |g| = 2 or 4 → the 1×2 / 1×4 wedge stood on end.
* **Two-direction facets**: a plane h/a + u/b + v/c = m on a grid of a×b×c boxes cuts every box either at its low corner
  (a **pyramid** remains) or at its high corner (an **inverse pyramid** remains). The faces alternate and form one flat
  facet: the "pyramid + inverse pyramid combo".
  * Box sizes {a, b, c}: {1,1,1}, {1,1,2}, {1,1,4}, {1,2,2}, {1,2,4}, {1,4,4}.
  * Block pairs: 03/04, 06/07, 09/10, 11/14, 12/15, 13/16.
  * Gradients (g_u, g_v) = (a/b, a/c).

Allowed gentle gradients (|g| ≤ 1), all signs:

| g_u \ g_v | 0 | 1/4 | 1/2 | 1 |
|---|---|---|---|---|
| **0** | cube | wedge 1×4 | wedge 1×2 | wedge 1×1 |
| **1/4** | wedge 1×4 | pyr 4×4 (box 1,4,4) | pyr 2×4 (box 1,4,2) | pyr 1×4 (box 1,4,1) |
| **1/2** | wedge 1×2 | pyr 2×4 | pyr 2×2 (box 1,2,2) | pyr 1×2 (box 1,2,1) |
| **1** | wedge 1×1 | pyr 1×4 | pyr 1×2 | pyr 1×1 (box 1,1,1) |

Every combination of {0, ¼, ½, 1} in both directions is available, plus steep ones (2, 4) through other leg
orders. So any gently curving surface can be quantised to these 16 gradients per sign quadrant.

### 2.2 Tiling condition

A facet with gradient from box (a, b, c) tiles exactly only if:
* its plane passes through the corners of an a×b×c box grid, and
* the facet's footprint is a union of whole boxes.

Neighbouring facets with different gradients have different box grids. Where they meet, the boundary must run along
lattice lines shared by both grids. This is the real difficulty, and the reason per-voxel or per-box greedy choices
produced spikes, gaps and shingles.

## 2.3 The additive-tile construction (chosen)

If a region's height field is **additive**, H(u, v) = A(u) + B(v), every tile is planar and shares its corners with its
neighbours exactly, so the surface is continuous everywhere. Quantise A and B each as a 1D lattice path of steps
(run L ∈ {1, 2, 4}, rise ∈ {−1, 0, +1}; flat steps have run 1). Then every tile, step of A × step of B, is exactly
one box column:

* **Both steps rise:** gradients (1/L_u, 1/L_v). The tile is a box (1, L_u, L_v), which is always one of the six
  pyramid families. Its plane spans two height layers: an inverse pyramid in the lower layer and a pyramid in the upper.
* **One step rises:** a wedge 1×L.
* **Both flat:** a flat face.

All nine combinations of runs (1, 2, 4) × (1, 2, 4) are in the block set. Tiles need no fill heuristics: the solid part
of each box is the box corners on the solid side of the plane, matched exactly to a block. Terracing can only appear
where two regions meet (chine, sheer, stem), never inside a region.

Fidelity cost: within a region the section shape is shared along the length and the plan shape across the height. That
is the accepted trade (smoothness over fidelity). Regions are chosen so the model fits: midbody side, forebody side,
bottom, deck.

## 3. Algorithm

The hull is symmetric. Build the starboard half and mirror it.

1. **Regions by dominant normal.** Classify the NURBS surface by the dominant axis of its normal:
   * SIDE: x = f(y, z)
   * BOTTOM: y = f(x, z)
   * STEM / BOW FACE: z = f(x, y)
   * TRANSOM
   * DECK: y = f(x, z) from above

   Use hysteresis so regions are large and contiguous. Each region becomes a height field over a 2D lattice.
2. **Gradient field.** Sample f and ∇f at lattice-cell centres (voxel units) from the analytic NURBS sections.
3. **Additive fit and path quantisation** (§2.3). Fit A(u) and B(v) by least squares over the region's samples.
   Quantise each with a dynamic programme over lattice heights. The cost is squared error plus a penalty per change of
   step type, so long constant runs are preferred. (The general ICM approach below is kept for later, non-additive
   regions.)

   *General approach:* **Coherent gradient quantisation.** Assign each cell a gradient from the §2.1 table. The objective has three parts:
   * a data term: distance from the true gradient;
   * a smoothness term: a penalty for neighbouring cells with different gradients;
   * an alignment term: a patch's extent along u and v must be a multiple of its box size, so it tiles.

   Solve by iterated conditional modes (ICM) from the nearest-gradient start, then merge patches smaller than one box.
   This turns the continuous curvature into a few large constant-gradient patches. The gradient changes only where it
   has to, and in steps of one table entry.
4. **Height integration with lattice snapping.** Integrate the quantised gradients into corner heights H(u, v), from an
   anchor on the keel or sheer line. Then:
   * per patch, snap the plane offset so it passes through its box-grid corners;
   * keep offsets continuous across patch borders wherever the two grids share the border line;
   * where they don't, allow a one-voxel step. Steps can only occur at patch borders, so on smooth regions there are
     none, which is the shingle criterion Q2.
5. **Exact box filling.** Inside each patch, the box grid and plane determine every box exactly: below the plane →
   full, cut at the low corner → pyramid, cut at the high corner → inverse pyramid, above → empty. Wedge patches are the
   2D case. No fill-fraction heuristics.
6. **Seams between regions** (chine side/bottom, sheer side/deck, stem):
   * place region borders on lattice lines;
   * the seam row uses the corner pieces that both neighbours imply (wedge, pyramid, inverse pyramid);
   * these are chosen by exact matching against the game-verified block solids.
7. **Interior and deck.**
   * Cubes behind the skin up to the shell thickness.
   * Deck plating closes every column at the deck height field, so the sheer becomes 1:4 / 1:2 ramps.
   * Optionally, deck camber.
8. **Mirror to port.** Mirroring swaps chirality, so pyramids use mirrored rotations. These are allowed (Q5).
9. **Validate** Q1–Q6 and report the numbers in the UI's bake panel.

## 4. Test plan (written before the code)

* **Planes:** for every gradient in the §2.1 table and every sign, a half-space must produce only its block family,
  with zero holes and zero step faces.
* **Prism hull** (constant section, length 10 m):
  * every section must be identical (Q2 = 0 step faces outside the ends);
  * the parallel midbody must use the same blocks in every row.
* **Tapered prism** (beam shrinks linearly): the taper must appear as pyramid/inverse-pyramid facets, not steps.
* **Cylinder and sphere:** deviation and watertightness (Q1, Q3) on doubly curved surfaces.
* **Default hull and presets:** every metric reported, with thresholds from §1 asserted in CI.
* **Render set:** six fixed flat-colour views, saved per run for review.

## 5. Phases

| Phase | Scope | Exit check |
|---|---|---|
| P1 | Height-field quantiser and exact box filling for one region (SIDE) on test surfaces | §4 planes, prism, tapered prism pass |
| P2 | BOTTOM and DECK regions, chine and sheer seams | default hull Q1/Q2/Q6 pass midships |
| P3 | STEM and TRANSOM regions, bow and stern seams | default hull and all presets pass Q1–Q4 |
| P4 | UI: bake options, metric readout, colour zones (boot-top), performance (< 2 s for a 30 m hull) | product-owner review of the render set |

### P1 status (2026-11-04)

Implemented in `crates/keel-geom/src/{lattice,hullblocks,metrics}.rs`:

* SIDE (A(z) + B(y)) and BOTTOM (C(z) + D(x)) regions, mirrored to port. The centre column uses cubes and fore-aft
  wedges only.
* The chine is joined by constraining D to end at the side's chine point at midships (`PathOptions::end_h`), so the
  midship section is one continuous lattice polyline.
* **Closure.** The quantised model solid (bottom, side, deck) defines the hull. Every model-boundary voxel that no
  tile covers becomes a cube. This closes the deck, the transom, the stem and the remaining seams.
* **Metrics** (`metrics::measure`). The exact block solids are rasterised at 4 samples per voxel and the outside is
  flood-filled. Q1 leaks are interior samples reached from outside. Q2 steps are exposed faces more than 25° off the
  NURBS normal. Debugging aids: `KEEL_METRICS_SLICE=<z voxel>` prints an ASCII section and `KEEL_METRICS_TRACE=1`
  prints a leak path.

Default 20 m hull: **0 leaks; mid-zone step faces 5.2 %** (target < 3 %). The remaining steps are on the forebody chine
(z 5–7 m), where the bottom rises and the plan narrows at the same time. The bow and stern zones are 51 % steps, because
the closure cubes terrace the forefoot. These are the P2/P3 seam and stem work.

### Swept-curve builder (current, 2026-11-04)

This replaces the P1 regions, after product-owner guidance on how builders shape a bow: "from the front, a few 1×1
blocks wide, then 1×2 slopes, 1×1 slopes, 1×2 and 1×4 slopes to define the curve, then pyramids down from there".

* **Model** (`hullblocks.rs`, `sweep.rs`):
  * Side: the deck-edge plan curve, a monotone lattice path (steps flat, 1×4, 1×2, 1×1, 2×1 and the flat front),
    fitted by DP so its slope only steepens toward the stem. It is moved inward per row by the midship flare offset
    B(ℓy), and curls under at 45° below the chine.
  * Bottom: the keel profile, a lattice path (flat keel, forefoot slopes, vertical stem), moved up by the deadrise
    D(ℓx). Deadrise slopes are limited to 0, ½ and 1, so the crease with the side's 45° curl lands on lattice points.
  * Deck: sheer ramps. Transom: flat.
  * Sweeping a plan segment by a row offset gives a plane. For slopes up to 1×1, and for the flat front, every offset
    rate (0, ¼, ½, 1 per row) gives a tileable plane (wedge or pyramid pair, including 2×4 and 4×4). 2×1 segments need a
    rate of ½ or more.
* **Realisation** (`tiling.rs`): label-consistent tiling. Every block, in every type and rotation, whose lattice-corner
  labels match the model is a valid piece. The largest go first, then 1-voxel blocks, then shell cubes. The starboard
  half is tiled and mirrored; the centre column takes only mirror-symmetric blocks. Watertight by construction (0 leaks
  on every preset).
* **Review tools**:
  * `scripts/hullshot.py` (depth-buffered flat-colour renders; `--bytype` colours cubes, 1-voxel blocks and long blocks);
  * the `curves`, `modelcheck`, `steps`, `maskat` and `blocksat` examples.

Refinements:
* **Forefoot from the side sweep.** The forward keel line is flat; the forefoot and stem come from the side's 45° curl
  reaching the centre line higher as the deck curve narrows. A separate rising keel line cut diagonally through the side's
  4-row tiles, and a ¼-gradient plane can only be tiled by 4-long blocks. `KEEL_FOREFOOT=1` restores the fitted profile.
* **Plan steps.** Up to 1×1 plus the flat front. A 2×1 segment swept by a 1-in-4 flare is not tileable.
* **Flare above the midship deck** is held constant, so the stem does not widen into a T where the sheer rises.
* **Tile-grid alignment.** A plane with equal gradients fits many diagonally shifted boxes, and mixing them leaves gaps.
  Candidates whose footprint sits between the model's own break lines win ties: plan points and flare breaks for side
  tiles, keel points and deadrise breaks for bottom tiles.

Status, default hull: 0 leaks. Bow topsides are 4×4 / 2×4 / 1×4 pyramid pairs, 45° chamfers near the stem and a flat
stem bar. Remaining: a few 1-voxel pieces where the transom lift raises the aft keel, and a few small marks on the bottom
near the bow where the curl's crease crosses a ¼ deadrise.

## 6. Open questions

* Should the bow and stern prefer pyramid facets everywhere (smoothest), or allow 1-voxel steps where the gradient
  exceeds 1 (closer to how many builders finish a stem)? Default: facets up to gradient 1, steps beyond.
* Hollow shell thickness default: one voxel (lightest) or two (stronger in game)? Default: one.
