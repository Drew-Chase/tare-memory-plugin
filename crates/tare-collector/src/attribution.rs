//! Attribution: assign allocations to the deepest workspace source line.
//!
//! Given a list of `RawAllocation`s, each with a resolved call stack, find
//! the **deepest frame** whose file path is under the workspace root. This
//! means `let rows: Vec<_> = it.collect()` is blamed on that user line,
//! not on `RawVec::grow` or `alloc::alloc::allocate`.

use std::collections::BTreeMap;

use crate::RawAllocation;

/// Key for attributed allocations: (relative_file_path, line_number).
pub type AttrKey = (String, u32);

/// Attribute allocations to the deepest frame under `workspace_root`.
///
/// `workspace_root` should use forward slashes. Frame file paths are
/// normalized to forward slashes before comparison.
///
/// Returns a map from (file, line) to all allocations attributed there.
pub fn attribute(
    allocations: &[RawAllocation],
    workspace_root: &str,
) -> BTreeMap<AttrKey, Vec<RawAllocation>> {
    let mut result: BTreeMap<AttrKey, Vec<RawAllocation>> = BTreeMap::new();

    for alloc in allocations {
        if let Some((file, line)) = find_attribution_frame(&alloc.frames, workspace_root) {
            result.entry((file, line)).or_default().push(alloc.clone());
        }
    }

    result
}

/// Find the deepest (last in the frames list, since dhat lists from
/// shallowest/allocator to deepest/user-code... actually, dhat lists
/// frames from allocator internals at top to user code at bottom, so
/// the "deepest user frame" is the LAST frame whose path is under the
/// workspace root.
///
/// Wait — let me re-examine. In the dhat JSON `fs` array:
/// - Index 0 is typically `[root]`
/// - Lower indices are allocator internals (alloc::alloc, raw_vec, etc.)
/// - Higher indices are user code (sample::build_rows, sample::main)
///
/// So frames go from top-of-stack (allocator) to bottom-of-stack (main).
/// The "deepest workspace frame" is the FIRST frame (lowest index) that
/// is under the workspace root — that's the direct call site.
///
/// Example: `[alloc::alloc, raw_vec::grow, Vec::push, sample::build_rows, sample::main]`
/// → attribute to `sample::build_rows` (first workspace frame).
fn find_attribution_frame(
    frames: &[crate::ResolvedFrame],
    workspace_root: &str,
) -> Option<(String, u32)> {
    // Normalize workspace root: strip trailing slash, lowercase for comparison.
    let ws_normalized = workspace_root.trim_end_matches('/');

    for frame in frames {
        let Some(ref file) = frame.file else {
            continue;
        };
        let Some(line) = frame.line else { continue };

        // Normalize the file path.
        let file_normalized = file.replace('\\', "/");

        // Check if this file is under the workspace root.
        // The file path from dhat may be relative to the build dir or
        // contain the crate name as a prefix.
        if let Some(rel) = extract_workspace_relative_path(&file_normalized, ws_normalized) {
            return Some((rel, line));
        }
    }

    None
}

/// Try to extract a workspace-relative path from a frame's file path.
///
/// dhat frame file paths come in several forms:
/// - Absolute: `/home/user/project/src/main.rs`
/// - Crate-relative: `sample/src/lib.rs` (when the crate name matches a dir)
/// - Std-lib: `alloc/src/alloc.rs`, `src/raw_vec/mod.rs`
///
/// We match by checking:
/// 1. If the path starts with the workspace root → strip the prefix
/// 2. If the path starts with `src/` and workspace_root ends with a crate name → it's a std path, skip
/// 3. If the path looks like `<crate>/src/...` where `<crate>` matches a known workspace member → use it
///
/// For v1, we use a simpler heuristic: if the path contains `src/` and
/// the segment before `src/` matches a directory that exists under the
/// workspace root, accept it.
fn extract_workspace_relative_path(file_path: &str, workspace_root: &str) -> Option<String> {
    let file_normalized = file_path.replace('\\', "/");
    let ws_normalized = workspace_root.replace('\\', "/");

    // Case 1: absolute path under workspace root.
    if file_normalized.starts_with(&ws_normalized) {
        let rel = file_normalized[ws_normalized.len()..].trim_start_matches('/');
        return Some(rel.to_string());
    }

    // Case 2: relative path that starts with the workspace dir name.
    // e.g., workspace_root="/abs/path/sample", frame="sample/src/lib.rs"
    // → strip "sample/" prefix → "src/lib.rs"
    if let Some(ws_dir_name) = ws_normalized.rsplit('/').next() {
        let prefix = format!("{ws_dir_name}/");
        if file_normalized.starts_with(&prefix) {
            let rel = &file_normalized[prefix.len()..];
            // Sanity: reject if this looks like a std-lib path after stripping.
            if !is_known_non_workspace(rel) {
                return Some(rel.to_string());
            }
        }
    }

    // Case 3: filter out known standard library / dependency paths.
    if is_known_non_workspace(&file_normalized) {
        return None;
    }

    // Case 4: relative workspace path with "src/" in it.
    if file_normalized.contains("/src/") || file_normalized.starts_with("src/") {
        let first_segment = file_normalized.split('/').next().unwrap_or("");
        if matches!(first_segment, "alloc" | "core" | "std" | "iter" | "slice" | "src") {
            return None;
        }
        return Some(file_normalized);
    }

    None
}

fn is_known_non_workspace(path: &str) -> bool {
    let known = [
        "alloc/", "core/", "std/", "src/raw_vec/", "src/vec/",
        "src/sync/", "src/io/", "iter/", "slice/",
    ];
    known.iter().any(|p| path.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResolvedFrame;

    fn frame(func: &str, file: Option<&str>, line: Option<u32>) -> ResolvedFrame {
        ResolvedFrame {
            function: func.to_string(),
            file: file.map(|s| s.to_string()),
            line,
        }
    }

    #[test]
    fn attributes_to_first_workspace_frame() {
        let frames = vec![
            frame("alloc::alloc::allocate", Some("alloc/src/alloc.rs"), Some(429)),
            frame("alloc::raw_vec::grow", Some("src/raw_vec/mod.rs"), Some(449)),
            frame("Vec::with_capacity", Some("src/vec/mod.rs"), Some(526)),
            frame("sample::build_rows", Some("sample/src/lib.rs"), Some(12)),
            frame("sample::main", Some("sample/src/main.rs"), Some(12)),
        ];

        let result = find_attribution_frame(&frames, "/home/user/project");
        assert_eq!(result, Some(("sample/src/lib.rs".to_string(), 12)));
    }

    #[test]
    fn skips_std_lib_frames() {
        let frames = vec![
            frame("alloc::alloc::allocate", Some("alloc/src/alloc.rs"), Some(429)),
            frame("std::io::stdout", Some("std/src/io/stdio.rs"), Some(719)),
        ];

        let result = find_attribution_frame(&frames, "/home/user/project");
        assert_eq!(result, None);
    }

    #[test]
    fn attributes_absolute_path() {
        let frames = vec![
            frame("alloc::alloc::allocate", Some("alloc/src/alloc.rs"), Some(429)),
            frame("myapp::handler", Some("/home/user/project/src/main.rs"), Some(42)),
        ];

        let result = find_attribution_frame(&frames, "/home/user/project");
        assert_eq!(result, Some(("src/main.rs".to_string(), 42)));
    }

    #[test]
    fn strips_workspace_dir_prefix() {
        // When workspace_root="/abs/path/sample" and frame path is "sample/src/lib.rs",
        // the "sample/" prefix should be stripped → "src/lib.rs".
        let frames = vec![
            frame("alloc::alloc::allocate", Some("alloc/src/alloc.rs"), Some(429)),
            frame("sample::build_rows", Some("sample/src/lib.rs"), Some(12)),
        ];

        let result = find_attribution_frame(&frames, "/abs/path/sample");
        assert_eq!(result, Some(("src/lib.rs".to_string(), 12)));
    }

    #[test]
    fn groups_allocations_by_line() {
        let allocs = vec![
            RawAllocation {
                total_bytes: 100,
                total_count: 1,
                peak_bytes: 100,
                frames: vec![
                    frame("alloc::alloc", Some("alloc/src/alloc.rs"), Some(1)),
                    frame("foo::bar", Some("mycrate/src/lib.rs"), Some(10)),
                ],
            },
            RawAllocation {
                total_bytes: 200,
                total_count: 2,
                peak_bytes: 200,
                frames: vec![
                    frame("alloc::alloc", Some("alloc/src/alloc.rs"), Some(1)),
                    frame("foo::bar", Some("mycrate/src/lib.rs"), Some(10)),
                ],
            },
            RawAllocation {
                total_bytes: 50,
                total_count: 1,
                peak_bytes: 50,
                frames: vec![
                    frame("alloc::alloc", Some("alloc/src/alloc.rs"), Some(1)),
                    frame("foo::baz", Some("mycrate/src/lib.rs"), Some(20)),
                ],
            },
        ];

        let result = attribute(&allocs, "/project");

        // Two distinct lines.
        assert_eq!(result.len(), 2);

        let line10 = result.get(&("mycrate/src/lib.rs".to_string(), 10)).unwrap();
        assert_eq!(line10.len(), 2); // two allocations at line 10

        let line20 = result.get(&("mycrate/src/lib.rs".to_string(), 20)).unwrap();
        assert_eq!(line20.len(), 1);
    }
}
