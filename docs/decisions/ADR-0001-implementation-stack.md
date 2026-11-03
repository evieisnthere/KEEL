# ADR-0001 — Implementation stack: Rust

Status: **accepted (user direction, 2026-11-03)**. The geometry kernel is to be confirmed by a spike · Supersedes plan §12 "Qt 6 + OCCT"

## Context
- KEEL must run on Windows and Linux and build from a single `build.sh` (user requirement).
- Qt 6 + OCCT is a heavy C++ toolchain (vcpkg, long builds, licensing nuance), and it can't be installed in the cloud dev container.
  Rust 1.97 and crates.io are available here, so the real application can be built and tested in the same place the research happens.
- Phase 0 has produced Python reference models (`sim-reference/`) and a lossless XML layer (`src/keel_io/`). These are specifications to port.

## Decision
Build KEEL as a Rust Cargo workspace:

| Concern | Choice | Notes |
|---|---|---|
| Native format I/O | own crate `keel-io` | port of the lossless XML layer (span-preserving, byte-identical) |
| Vehicle model, catalogue | `keel-model` | no UI or geometry deps (plan §12 rules) |
| Simulation | `keel-sim` | port of RE-0003…0007 reference models, golden tests against the Python reference |
| Geometry kernel | **spike:** `truck` (pure-Rust B-rep/NURBS) vs `opencascade-rs` (OCCT bindings) | decide on measured results: STEP import, lofts, booleans, build time on Windows |
| Meshing / STL | `truck-meshalgo` or own; `stl_io` | |
| UI shell | `egui`/`eframe` | immediate-mode, cross-platform, wgpu backend |
| 3D viewport | `wgpu` | block-first renderer (instanced voxels + surface types), CAD overlays |
| Project files | `.keel` zip (`zip` crate) + `serde` JSON | plan §21 |
| Build | `build.sh` → `cargo build --release` (+ Windows cross-target via `cargo xwin` or native) | |

## Consequences
- The Python modules stay as executable specifications and test oracles. Rust crates must match them on shared fixtures.
- OCCT stays available through `opencascade-rs` if `truck` falls short on STEP or booleans. This is a measured decision, not a preference (plan §12).
- Plan §17's repository layout becomes a Cargo workspace (`crates/keel-io`, `keel-model`, `keel-sim`, `keel-geom`, `keel-app`).
