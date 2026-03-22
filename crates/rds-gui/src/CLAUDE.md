# crates/rds-gui/src/

egui/eframe GUI implementation: app state machine, panels, interactions, and file actions.

## Files

| File | What | When to read |
| ---- | ---- | ------------ |
| `lib.rs` | `RustDirStatApp` (ScanPhase state machine, dir picker, scanner spawning, event drain loop, 3-panel layout, PendingDelete, confirm_delete, delete confirmation dialog, CommandEditorState, custom_commands, ExportDialogState, freed_bytes tracking, `format_bytes` utility, config persistence via save callback, recent paths tracking/dropdown, settings dialog integration, `on_exit` save, `collect_config`/`save_config` helpers) | Modifying app state, scan lifecycle, layout, adding panel implementations, modifying delete action flow, modifying config persistence |
| `tree_view.rs` | `SubtreeStats` (cached subtree sizes/file counts, filters deleted nodes), `TreeViewState` (expanded nodes, selection sync, scroll-to), `expand_ancestors`, `sorted_children` (filters deleted), tree view rendering with right-click context menu | Modifying tree view display, adding tree interactions, understanding size caching strategy |
| `ext_stats.rs` | `hsl_to_color32` (HSL to Color32 conversion), `show` (extension stats panel with stacked bar chart, scrollable Grid table, click-to-select extension filter) | Modifying extension statistics display, reusing hsl_to_color32 for treemap coloring |
| `treemap.rs` | `CushionCoeffs`, `TreemapRect`, `TreemapLayout` (cached layout with cushion coefficients), `compute_recursive` (recursive squarify), `find_drill_target`, `breadcrumb_chain`, `build_cushion_mesh`, `show` (cushion mesh render + flat fallback + extension dimming + tooltip + click/double-click + right-click context menu) | Modifying treemap rendering, tuning cushion constants, understanding drill-down navigation |
| `duplicates.rs` | `show` (duplicates bottom panel with collapsible groups sorted by wasted space, selectable file paths with right-click context menu: Open in File Manager + Delete) | Modifying duplicate display, understanding panel layout ordering |
| `actions.rs` | `execute_delete` (trash crate + arena tombstone), `cleanup_duplicate_groups` (prune stale entries after deletion), `open_in_file_manager` (reveal file/dir in native file manager), `open_file_revealing` (platform-specific file select), `execute_custom_command` (platform shell dispatch with {path} substitution) | Modifying file action logic |
| `command_editor.rs` | `show` (command editor window: inline editing of custom commands, add/remove controls, close button) | Modifying command editor UI |
| `export.rs` | `ExportFormat`, `ExportScope`, `ExportRecord`, `DuplicateExportRecord`, `ExportResult`, `ExportDialogState`, `export_tree` (DFS traversal + CSV/JSON write), `export_duplicates` (duplicate group flattening + CSV/JSON write), `default_filename`, `show_dialog` (export dialog window with format/scope ComboBox, rfd save dialog, result display) | Modifying export logic, adding export formats, modifying export dialog UI |
| `settings.rs` | `SettingsDialogState`, `show` (settings window with exclude patterns editor, sort order ComboBox, color scheme ComboBox, Apply/Cancel buttons) | Modifying settings dialog UI, adding new configuration options |
