# crates/

Cargo workspace members. Each crate has its own CLAUDE.md with file index.

## Subdirectories

| Directory | What | When to read |
| --------- | ---- | ------------ |
| `rds-core/` | Shared data types; zero deps beyond `serde` (CI-enforced) | Modifying core types, understanding arena tree layout, adding scan events |
| `rds-scanner/` | Parallel jwalk scanner, crossbeam event streaming, structured tracing spans | Implementing scan logic, modifying scanner-GUI event protocol |
| `rds-gui/` | egui/eframe GUI: dir picker, tree view, ext stats, cushion-shaded treemap, duplicates, delete/open/command actions, CSV/JSON export, settings, config persistence, toast notifications, scan error log | Implementing UI panels, modifying scan lifecycle, adding interactions or actions |
