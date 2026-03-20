# crates/

Cargo workspace members. Each crate has its own CLAUDE.md with file index.

## Subdirectories

| Directory | What | When to read |
| --------- | ---- | ------------ |
| `rds-core/` | Shared data types; zero deps beyond `serde` (CI-enforced) | Modifying core types, understanding arena tree layout, adding scan events |
| `rds-scanner/` | Single-threaded walkdir scanner, crossbeam event streaming | Implementing scan logic, modifying scanner-GUI event protocol |
| `rds-gui/` | egui/eframe GUI shell, treemap rendering (stub) | Implementing UI panels, modifying the eframe app struct |
