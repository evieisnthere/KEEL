# Hull → blocks: design

Status: **plan, awaiting product-owner review** (2026-11-04). It replaces the ad-hoc voxel-fitting attempts
(`voxel.rs`, `blockhull.rs`, `facets.rs`). Those stay only as references until this design lands.

## 1. What "good" means (measurable)

A generated hull must be:

| # | Criterion | Metric (checked automatically) |
|---|---|---|
| Q1 | **Watertight**: no holes in the skin or the deck | every exposed face of the block shell is backed by a solid. Hole count = 0 |
| Q2 | **No shingles on smooth surfaces**: where the NURBS curves gently, adjacent sections produce one continuous planar face, not 1-voxel jumps | area of exposed *step faces*, i.e. faces whose normal is > 25° off the local NURBS normal, as a share of hull area. Target < 3 % outside the bow and stern zones |
| Q3 | **Faithful**: follows the NURBS | max and RMS distance from the block surface to the NURBS, in voxels. Target max ≤ 0.6, RMS ≤ 0.25 |
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
3. **Coherent gradient quantisation.** Assign each cell a gradient from the §2.1 table. The objective has three parts:
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

## 6. Open questions

* Should the bow and stern prefer pyramid facets everywhere (smoothest), or allow 1-voxel steps where the gradient
  exceeds 1 (closer to how many builders finish a stem)? Default: facets up to gradient 1, steps beyond.
* Hollow shell thickness default: one voxel (lightest) or two (stronger in game)? Default: one.
