//! File actions for the GUI (delete, open in file manager, custom commands).
//!
//! `execute_delete` sends a file/directory to the OS trash and tombstones the
//! arena node. `cleanup_duplicate_groups` prunes stale entries from duplicate
//! groups after deletions. `open_in_file_manager` reveals a file or directory
//! in the platform's native file manager. `execute_custom_command` runs a
//! user-defined shell command with `{path}` placeholder substitution.

use rds_core::CustomCommand;
use rds_core::tree::DirTree;

use crate::DuplicateGroup;

/// Sends the filesystem entry at `index` to the OS trash and tombstones the
/// corresponding arena subtree.
///
/// Returns `Ok(freed_bytes)` on success (subtree size before tombstoning) or
/// `Err` with the trash error message if the OS trash operation fails.
/// No tombstoning occurs on failure.
pub(crate) fn execute_delete(tree: &mut DirTree, index: usize) -> Result<u64, String> {
    let path = tree.path(index);
    let freed_bytes = tree.subtree_size(index);
    trash::delete(&path).map_err(|e| e.to_string())?;
    tree.tombstone(index);
    Ok(freed_bytes)
}

/// Removes deleted nodes from duplicate groups and recalculates wasted bytes.
///
/// For each group: retains only indices where the node exists and is not
/// deleted, recalculates `wasted_bytes` from the remaining members, and
/// removes groups with fewer than 2 members. Re-sorts by `wasted_bytes`
/// descending.
pub(crate) fn cleanup_duplicate_groups(groups: &mut Vec<DuplicateGroup>, tree: &DirTree) {
    for group in groups.iter_mut() {
        group
            .node_indices
            .retain(|&idx| tree.get(idx).is_some_and(|n| !n.deleted));

        let file_size = group
            .node_indices
            .first()
            .and_then(|&idx| tree.get(idx))
            .map(|n| n.size)
            .unwrap_or(0);
        group.wasted_bytes =
            file_size.saturating_mul(group.node_indices.len().saturating_sub(1) as u64);
    }

    groups.retain(|g| g.node_indices.len() >= 2);
    groups.sort_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));
}

/// Opens the file manager for the filesystem entry at `index`.
///
/// For directories, opens the directory directly. For files, uses
/// platform-specific commands to reveal (select) the file in its parent
/// directory.
pub(crate) fn open_in_file_manager(tree: &DirTree, index: usize) -> Result<(), String> {
    let node = tree
        .get(index)
        .ok_or_else(|| format!("node at index {index} not found"))?;
    let path = tree.path(index);

    let result = if node.is_dir {
        open::that_detached(&path).map_err(|e| e.to_string())
    } else {
        open_file_revealing(&path)
    };

    match &result {
        Ok(()) => tracing::debug!("opened in file manager: {}", path.display()),
        Err(e) => tracing::warn!("failed to open in file manager: {}: {e}", path.display()),
    }

    result
}

/// Platform-specific file reveal: selects the file in the native file manager.
fn open_file_revealing(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // `raw_arg` bypasses Rust's automatic quoting so explorer sees the
        // `/select,"<path>"` token exactly as intended, even when the path
        // contains spaces, commas, or cmd metacharacters like `&` or `%`.
        use std::os::windows::process::CommandExt;
        let mut child = std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map_err(|e| e.to_string())?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        let mut child = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // Try D-Bus FileManager1.ShowItems first (selects file in file manager).
        // Falls back to opening parent directory if D-Bus is unavailable.
        match dbus_show_items(path) {
            Ok(()) => Ok(()),
            Err(dbus_err) => {
                tracing::debug!(
                    "D-Bus file reveal failed, falling back to open parent: {dbus_err}"
                );
                let target = path.parent().unwrap_or(path);
                open::that_detached(target).map_err(|e| e.to_string())
            }
        }
    }
}

/// Percent-encodes a filesystem path into a `file://` URI suitable for D-Bus.
/// Encodes all bytes except unreserved characters (RFC 3986 Section 2.3) and `/`.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn path_to_file_uri(path: &std::path::Path) -> String {
    let path_str = path.to_string_lossy();
    let mut uri = String::with_capacity(7 + path_str.len() * 3);
    uri.push_str("file://");
    for byte in path_str.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(*byte as char);
            }
            _ => {
                uri.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    uri
}

/// Attempts to reveal a file in the Linux file manager via D-Bus
/// FileManager1.ShowItems. Returns Ok(()) if the D-Bus call succeeds,
/// Err if dbus-send is not available or the call fails.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn dbus_show_items(path: &std::path::Path) -> Result<(), String> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let uri = path_to_file_uri(&canonical);
    let status = std::process::Command::new("dbus-send")
        .arg("--session")
        .arg("--dest=org.freedesktop.FileManager1")
        .arg("--type=method_call")
        .arg("/org/freedesktop/FileManager1")
        .arg("org.freedesktop.FileManager1.ShowItems")
        .arg(format!("array:string:{uri}"))
        .arg("string:")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("D-Bus FileManager1 exited with {status}"))
    }
}

/// Shell-escapes a path string for safe interpolation into a shell command.
///
/// On Unix, wraps in single quotes and escapes embedded single quotes.
/// On Windows cmd, wraps in double quotes and escapes embedded double quotes.
fn shell_escape(s: &str) -> String {
    #[cfg(not(target_os = "windows"))]
    {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
    #[cfg(target_os = "windows")]
    {
        format!("\"{}\"", s.replace('"', "\"\""))
    }
}

/// Resolves a command template by replacing `{path}` with a shell-escaped path.
fn resolve_template(template: &str, path: &std::path::Path) -> String {
    let escaped = shell_escape(&path.to_string_lossy());
    template.replace("{path}", &escaped)
}

/// Executes a user-defined custom command for the filesystem entry at `index`.
///
/// Replaces `{path}` in the command template with the shell-escaped full path
/// of the node, then spawns the resolved command in a platform-specific shell
/// (fire-and-forget).
pub(crate) fn execute_custom_command(
    tree: &DirTree,
    index: usize,
    command: &CustomCommand,
) -> Result<(), String> {
    tree.get(index)
        .ok_or_else(|| format!("node at index {index} not found"))?;

    let path = tree.path(index);
    let resolved_command = resolve_template(&command.template, &path);

    #[cfg(not(target_os = "windows"))]
    let spawn_result = std::process::Command::new("sh")
        .arg("-c")
        .arg(&resolved_command)
        .spawn()
        .map_err(|e| e.to_string());

    #[cfg(target_os = "windows")]
    let spawn_result = std::process::Command::new("cmd")
        .arg("/c")
        .arg(&resolved_command)
        .spawn()
        .map_err(|e| e.to_string());

    match spawn_result {
        Ok(mut child) => {
            tracing::debug!("{}: {}", command.name, resolved_command);
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(())
        }
        Err(e) => {
            tracing::warn!("failed to execute '{}': {e}", command.name);
            Err(e)
        }
    }
}

/// Renders custom command buttons in a context menu. Adds a separator before
/// the commands if the list is non-empty.
pub(crate) fn show_custom_commands_menu(
    ui: &mut egui::Ui,
    tree: &DirTree,
    index: usize,
    commands: &[CustomCommand],
    notifications: &mut crate::notifications::Notifications,
) {
    if commands.is_empty() {
        return;
    }
    ui.separator();
    for command in commands {
        if ui.button(&command.name).clicked() {
            if let Err(e) = execute_custom_command(tree, index, command) {
                notifications.error(format!("{}: {e}", command.name));
            }
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rds_core::tree::FileNode;

    fn make_file_node(name: &str, size: u64) -> FileNode {
        FileNode {
            name: name.to_string(),
            size,
            is_dir: false,
            children: Vec::new(),
            parent: None,
            extension: None,
            modified: None,
            deleted: false,
        }
    }

    fn make_dir_node(name: &str) -> FileNode {
        FileNode {
            name: name.to_string(),
            size: 0,
            is_dir: true,
            children: Vec::new(),
            parent: None,
            extension: None,
            modified: None,
            deleted: false,
        }
    }

    #[test]
    #[ignore]
    fn open_file_in_file_manager() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root_path = tmp.path();
        let subdir_path = root_path.join("subdir");
        std::fs::create_dir(&subdir_path).unwrap();
        let file_path = subdir_path.join("test.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let mut tree = DirTree::new(root_path.to_str().unwrap());
        let subdir_idx = tree.insert(0, make_dir_node("subdir"));
        let file_idx = tree.insert(subdir_idx, make_file_node("test.txt", 5));

        let result = open_in_file_manager(&tree, file_idx);
        assert!(result.is_ok(), "open file failed: {:?}", result.err());
    }

    #[test]
    #[ignore]
    fn open_directory_in_file_manager() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root_path = tmp.path();
        let subdir_path = root_path.join("subdir");
        std::fs::create_dir(&subdir_path).unwrap();
        let file_path = subdir_path.join("test.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let mut tree = DirTree::new(root_path.to_str().unwrap());
        let subdir_idx = tree.insert(0, make_dir_node("subdir"));
        tree.insert(subdir_idx, make_file_node("test.txt", 5));

        let result = open_in_file_manager(&tree, subdir_idx);
        assert!(result.is_ok(), "open dir failed: {:?}", result.err());
    }

    #[test]
    #[ignore]
    fn open_root_in_file_manager() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root_path = tmp.path();
        let subdir_path = root_path.join("subdir");
        std::fs::create_dir(&subdir_path).unwrap();
        let file_path = subdir_path.join("test.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let mut tree = DirTree::new(root_path.to_str().unwrap());
        let subdir_idx = tree.insert(0, make_dir_node("subdir"));
        tree.insert(subdir_idx, make_file_node("test.txt", 5));

        let result = open_in_file_manager(&tree, 0);
        assert!(result.is_ok(), "open root failed: {:?}", result.err());
    }

    #[test]
    fn open_nonexistent_path_returns_result() {
        let mut tree = DirTree::new("/nonexistent/path/that/does/not/exist");
        let file_idx = tree.insert(0, make_file_node("ghost.txt", 42));

        let result = open_in_file_manager(&tree, file_idx);
        // The function should not panic. On Linux, xdg-open may not fail
        // immediately for nonexistent paths, so we just verify the function
        // compiles, doesn't panic, and returns a Result.
        let _ = result;
    }

    #[test]
    fn open_invalid_index_returns_error() {
        let tree = DirTree::new("/some/root");
        let result = open_in_file_manager(&tree, 999);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_template_substitutes_path() {
        let path = std::path::Path::new("/tmp/test.txt");
        let result = resolve_template("echo {path}", path);
        assert!(
            result.contains("/tmp/test.txt"),
            "expected path in result, got: {result}"
        );
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn resolve_template_escapes_single_quotes() {
        let path = std::path::Path::new("/tmp/it's a file.txt");
        let result = resolve_template("echo {path}", path);
        assert_eq!(result, "echo '/tmp/it'\\''s a file.txt'");
    }

    #[test]
    fn resolve_template_no_placeholder() {
        let path = std::path::Path::new("/tmp/test.txt");
        let result = resolve_template("echo hello", path);
        assert_eq!(result, "echo hello");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn shell_escape_simple_path() {
        let result = shell_escape("/tmp/test.txt");
        assert_eq!(result, "'/tmp/test.txt'");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn shell_escape_path_with_spaces() {
        let result = shell_escape("/tmp/my files/test.txt");
        assert_eq!(result, "'/tmp/my files/test.txt'");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn shell_escape_path_with_single_quote() {
        let result = shell_escape("/tmp/it's here.txt");
        assert_eq!(result, "'/tmp/it'\\''s here.txt'");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn shell_escape_path_with_shell_metacharacters() {
        let result = shell_escape("/tmp/$(whoami).txt");
        assert_eq!(result, "'/tmp/$(whoami).txt'");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn shell_escape_simple_path_windows() {
        let result = shell_escape("C:\\Users\\test.txt");
        assert_eq!(result, "\"C:\\Users\\test.txt\"");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn shell_escape_path_with_spaces_windows() {
        let result = shell_escape("C:\\My Files\\test.txt");
        assert_eq!(result, "\"C:\\My Files\\test.txt\"");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn shell_escape_path_with_double_quote_windows() {
        let result = shell_escape("C:\\test\"file.txt");
        assert_eq!(result, "\"C:\\test\"\"file.txt\"");
    }

    #[test]
    #[ignore]
    fn execute_custom_command_substitutes_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root_path = tmp.path();

        let mut tree = DirTree::new(root_path.to_str().unwrap());
        let file_idx = tree.insert(0, make_file_node("test.txt", 5));

        let cmd = CustomCommand {
            name: "Echo Path".to_string(),
            template: "echo {path}".to_string(),
        };

        let result = execute_custom_command(&tree, file_idx, &cmd);
        assert!(
            result.is_ok(),
            "execute_custom_command failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn execute_custom_command_invalid_index_returns_error() {
        let tree = DirTree::new("/some/root");
        let cmd = CustomCommand {
            name: "Test".to_string(),
            template: "echo {path}".to_string(),
        };

        let result = execute_custom_command(&tree, 999, &cmd);
        assert!(result.is_err());
        assert!(
            result.as_ref().unwrap_err().contains("not found"),
            "expected 'not found' in error, got: {:?}",
            result.err()
        );
    }
}
