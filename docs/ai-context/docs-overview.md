# Documentation Overview

Registry of all project documentation, classified by tier for navigation.

## Tier 1 — Foundational (Project-Wide)

| Path | Purpose |
|------|---------|
| `CLAUDE.md` | Root AI context: file/directory guide, build/test commands, dev requirements |
| `README.md` | Architecture decisions, arena design rationale, scanner-GUI event flow, dependency strategy |
| `docs/milestones.md` | MS1-MS21 roadmap with deliverables and dependency graph |
| `docs/superpowers/specs/2026-03-19-rustdirstat-design.md` | Full design specification: goals, architecture, data flow, dependencies, testing strategy |
| `docs/ai-context/project-structure.md` | Annotated file tree, architectural patterns, milestone status |
| `docs/ai-context/docs-overview.md` | This file — documentation registry |

## Tier 2 — Component-Level

| Path | Purpose |
|------|---------|
| `crates/CLAUDE.md` | Workspace crates directory index |
| `crates/rds-core/CLAUDE.md` | Core crate AI context: file guide, module structure, zero-dep constraint |
| `crates/rds-core/README.md` | Arena invariants, index stability contract, append-only design rationale |
| `crates/rds-core/src/CLAUDE.md` | Core source files index: tree, scan, config, stats modules |
| `crates/rds-scanner/CLAUDE.md` | Scanner crate AI context: walkdir traversal, event streaming |
| `crates/rds-gui/CLAUDE.md` | GUI crate AI context: dependencies, streemap declaration |
| `docs/CLAUDE.md` | Documentation directory index |
| `plans/CLAUDE.md` | Implementation plans directory index |

## Tier 3 — Feature-Specific / Implementation Plans

| Path | Purpose |
|------|---------|
| `plans/ms1-workspace-scaffold-ci.md` | MS1 implementation plan with decision log (completed) |
| `plans/ms2-core-data-types.md` | MS2 implementation plan with decision log (completed) |
| `plans/ms3-single-threaded-scanner.md` | MS3 implementation plan with decision log (completed) |

## Cross-Reference Map

| Topic | Primary Source | Supporting Docs |
|-------|---------------|-----------------|
| Arena tree design | `crates/rds-core/README.md` | `tree.rs`, design spec |
| Scanner-GUI protocol | `README.md` (Architecture) | `scan.rs`, design spec |
| Dependency versions | `Cargo.toml` ([workspace.dependencies]) | `README.md` (Dependency management) |
| CI pipeline | `.github/workflows/ci.yml` | `README.md` (CI section) |
| Milestones & scope | `docs/milestones.md` | Design spec (Goals/Non-Goals) |
| Config schema | `crates/rds-core/src/config.rs` | Design spec (Configuration) |
| Extension colors | `crates/rds-core/src/stats.rs` | Design spec (Treemap Rendering) |
| Scanner implementation | `crates/rds-scanner/src/scanner.rs` | Design spec (Data Flow), `scan.rs` events |
