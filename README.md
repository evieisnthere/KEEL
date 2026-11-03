# KEEL — Stormworks Engineering Suite

A desktop CAD and systems-engineering suite for designing *Stormworks: Build and Rescue* vehicles.
Product direction: [`PROJECT_PLAN.md`](PROJECT_PLAN.md). Current progress: [`docs/STATUS.md`](docs/STATUS.md).

**Phase 0 (research).** This repository currently holds reverse-engineering findings for game v1.15.26, extracted metadata,
analysis tools, and an executable reference model of the game's water physics (`sim-reference/`).

- `docs/reverse-engineering/`: findings (RE-0001 to RE-0004), each tagged with an evidence level
- `docs/engineering/simulation-plan.md`: simulation architecture and hull-optimiser policy
- `research/extracted-metadata/`: small factual tables derived from the game (constants, surface types)
- `sim-reference/`: Python reference implementations (spec for the future C++ `keel-sim`)
- `tools/re/`: static-analysis and emulation scripts; they need your own copy of the game

Game binaries and game data files are **not** included and must not be committed (see `.gitignore`).

Run tests: `python3 -m pytest tests/unit`
