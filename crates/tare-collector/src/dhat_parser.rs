//! Parser for dhat's JSON output format.
//!
//! dhat writes `dhat-heap.json` with this structure:
//! - `ftbl`: array of frame strings, format:
//!   `"0xADDR: function_name (file.rs:line:col)"`
//! - `pps`: array of program points, each with:
//!   - `tb`/`tbk`: total bytes/blocks
//!   - `mb`/`mbk`: max live bytes/blocks
//!   - `gb`/`gbk`: bytes/blocks at global peak
//!   - `eb`/`ebk`: bytes/blocks at end
//!   - `fs`: array of indices into `ftbl`

use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;

use crate::{CollectorBackend, CollectorError, RawAllocation, ResolvedFrame};

/// dhat JSON backend — parses a `dhat-heap.json` file.
pub struct DhatBackend {
    json_path: std::path::PathBuf,
}

impl DhatBackend {
    pub fn new(json_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            json_path: json_path.into(),
        }
    }
}

impl CollectorBackend for DhatBackend {
    fn collect(&self) -> Result<Vec<RawAllocation>, CollectorError> {
        let contents = std::fs::read_to_string(&self.json_path)?;
        let dhat: DhatJson = serde_json::from_str(&contents)
            .map_err(|e| CollectorError::Parse(format!("invalid dhat JSON: {e}")))?;

        let mut allocations = Vec::with_capacity(dhat.pps.len());

        for pp in &dhat.pps {
            let frames: Vec<ResolvedFrame> = pp
                .fs
                .iter()
                .filter_map(|&idx| {
                    let idx = idx as usize;
                    dhat.ftbl.get(idx).and_then(|s| parse_frame(s))
                })
                .collect();

            // Skip entries with no resolvable frames (e.g., only "[root]").
            if frames.is_empty() {
                continue;
            }

            allocations.push(RawAllocation {
                total_bytes: pp.tb,
                total_count: pp.tbk,
                peak_bytes: pp.gb, // global peak bytes
                frames,
            });
        }

        Ok(allocations)
    }
}

/// Minimal representation of dhat's JSON output.
#[derive(Deserialize)]
struct DhatJson {
    #[serde(default)]
    ftbl: Vec<String>,
    #[serde(default)]
    pps: Vec<ProgramPoint>,
}

#[derive(Deserialize)]
struct ProgramPoint {
    /// Total bytes.
    tb: u64,
    /// Total blocks (allocation count).
    tbk: u64,
    /// Global peak bytes.
    gb: u64,
    /// Frame table indices.
    fs: Vec<u64>,
}

/// Parse a dhat frame string into a `ResolvedFrame`.
///
/// dhat frame format (Windows):
///   `"0x7ff7ce9aec81: sample::build_rows (sample\\src\\lib.rs:12:0)"`
/// dhat frame format (Unix):
///   `"0x55a1234: sample::build_rows (sample/src/lib.rs:12:0)"`
/// Special: `"[root]"` — skip.
fn parse_frame(frame_str: &str) -> Option<ResolvedFrame> {
    if frame_str == "[root]" {
        return None;
    }

    // Step 1: strip the "0xADDR: " prefix.
    static ADDR_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^0x[0-9a-fA-F]+: ").unwrap()
    });

    let rest = ADDR_PREFIX.find(frame_str).map(|m| &frame_str[m.end()..])?;

    // Step 2: try to extract the trailing " (file:line:col)" suffix.
    // dhat format: `function_name (file_path:LINE:COL)`
    //
    // Tricky: function names in generic contexts can contain `(` and `)`,
    // e.g. `String (*)(ref$<str$>)>`. We search backwards for the last
    // ` (` and verify the remainder matches `file:line:col)`.
    static FILE_LINE_COL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(.+):(\d+):\d+\)$").unwrap()
    });

    // Search from the end for " (" — try each occurrence right-to-left.
    let mut search_end = rest.len();
    while let Some(pos) = rest[..search_end].rfind(" (") {
        let inside = &rest[pos + 2..]; // after " ("
        if let Some(caps) = FILE_LINE_COL.captures(inside) {
            let function = rest[..pos].to_string();
            let file = caps.get(1).map(|m| m.as_str().replace('\\', "/"));
            let line = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
            return Some(ResolvedFrame {
                function,
                file,
                line,
            });
        }
        // This " (" wasn't the right one; keep searching leftward.
        search_end = pos;
    }

    // No file info — just a function name.
    Some(ResolvedFrame {
        function: rest.to_string(),
        file: None,
        line: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frame_with_file() {
        let f = parse_frame(
            "0x7ff7ce9aec81: sample::build_rows (sample\\src\\lib.rs:12:0)",
        )
        .unwrap();
        assert_eq!(f.function, "sample::build_rows");
        assert_eq!(f.file.as_deref(), Some("sample/src/lib.rs"));
        assert_eq!(f.line, Some(12));
    }

    #[test]
    fn parse_frame_unix_path() {
        let f = parse_frame(
            "0x55a1234abcde: mymod::func (src/main.rs:42:0)",
        )
        .unwrap();
        assert_eq!(f.function, "mymod::func");
        assert_eq!(f.file.as_deref(), Some("src/main.rs"));
        assert_eq!(f.line, Some(42));
    }

    #[test]
    fn parse_frame_root() {
        assert!(parse_frame("[root]").is_none());
    }

    #[test]
    fn parse_frame_alloc_internal() {
        let f = parse_frame(
            "0x7ff7ce9b11cc: alloc::alloc::impl$1::allocate (alloc\\src\\alloc.rs:429:0)",
        )
        .unwrap();
        assert_eq!(f.function, "alloc::alloc::impl$1::allocate");
        assert_eq!(f.file.as_deref(), Some("alloc/src/alloc.rs"));
    }

    #[test]
    fn parse_frame_complex_generic() {
        let f = parse_frame(
            "0x7ff7ce9b059b: core::iter::traits::iterator::Iterator::collect<core::iter::adapters::map::Map<core::slice::iter::Iter<alloc::string::String>,sample::duplicate_data::closure_env$0>,alloc::vec::Vec<alloc::string::String,alloc::alloc::Global> > (iter\\traits\\iterator.rs:2035:0)"
        ).unwrap();
        assert!(f.function.starts_with("core::iter::traits::iterator::Iterator::collect"));
        assert_eq!(f.file.as_deref(), Some("iter/traits/iterator.rs"));
        assert_eq!(f.line, Some(2035));
    }

    #[test]
    fn parse_frame_fn_pointer_in_generic() {
        // This frame has `(*)` and `(ref$<str$>)` inside the function name,
        // which can confuse naive regex matching.
        let input = r#"0x7ff7ce9b242d: enum2$<core::option::Option<ref$<str$> > >::map_or_else<ref$<str$>,alloc::string::String,alloc::fmt::format::closure_env$0,alloc::string::String (*)(ref$<str$>)> (core\src\option.rs:1278:0)"#;
        let f = parse_frame(input).unwrap();
        assert!(
            f.function.contains("map_or_else"),
            "function should contain map_or_else, got: {}",
            f.function
        );
        assert_eq!(f.file.as_deref(), Some("core/src/option.rs"));
        assert_eq!(f.line, Some(1278));
    }

    #[test]
    fn parse_real_dhat_json() {
        let dhat_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("dhat-heap.json");

        if !dhat_path.exists() {
            eprintln!("Skipping: no dhat-heap.json (run sample with tare-profile first)");
            return;
        }

        let backend = DhatBackend::new(&dhat_path);
        let allocs = backend.collect().unwrap();

        assert!(!allocs.is_empty(), "should have parsed some allocations");

        // Verify at least one allocation has frames referencing sample code.
        let has_sample_frame = allocs.iter().any(|a| {
            a.frames
                .iter()
                .any(|f| f.file.as_deref().map_or(false, |p| p.contains("sample")))
        });
        assert!(
            has_sample_frame,
            "should find frames from sample/ code"
        );
    }

    #[test]
    fn end_to_end_attribution() {
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();

        let dhat_path = project_root.join("dhat-heap.json");
        if !dhat_path.exists() {
            eprintln!("Skipping: no dhat-heap.json");
            return;
        }

        let backend = DhatBackend::new(&dhat_path);
        let allocs = backend.collect().unwrap();

        let report = crate::build_report(
            &allocs,
            &project_root.join("sample"),
            "2026-07-17T12:00:00Z",
        );

        // The report should have entries for sample source files.
        assert!(
            !report.files.is_empty(),
            "report should have file entries"
        );

        // Check that src/lib.rs has attributed lines.
        let lib_data = report.files.get("src/lib.rs")
            .expect("should have entries for src/lib.rs");

        let lib_lines: Vec<&String> = lib_data.lines.keys().collect();

        // We expect allocations on line 12 (build_rows/Vec::with_capacity).
        assert!(
            lib_data.lines.contains_key("12"),
            "expected attribution at line 12 (build_rows), got lines: {lib_lines:?}"
        );

        // Line 25: Box::new (box_a_row)
        assert!(
            lib_data.lines.contains_key("25"),
            "expected attribution at line 25 (box_a_row), got lines: {lib_lines:?}"
        );

        // Line 45: .clone() inside duplicate_data
        assert!(
            lib_data.lines.contains_key("45"),
            "expected attribution at line 45 (duplicate_data), got lines: {lib_lines:?}"
        );

        // Line 50: format! inside label
        assert!(
            lib_data.lines.contains_key("50"),
            "expected attribution at line 50 (label), got lines: {lib_lines:?}"
        );

        // Verify the entry on line 12 is a runtime heap_cumulative with real bytes.
        let line12 = &lib_data.lines["12"];
        assert!(!line12.entries.is_empty());
        assert_eq!(line12.entries[0].source, tare_schema::Source::Runtime);
        assert_eq!(line12.entries[0].kind, tare_schema::Kind::HeapCumulative);
        assert!(line12.entries[0].bytes.unwrap() > 0);

        // No files from std/alloc/core should appear in the report.
        for key in report.files.keys() {
            assert!(
                !key.starts_with("alloc/") && !key.starts_with("core/") && !key.starts_with("std/"),
                "spurious std-lib file in report: {key}"
            );
        }
    }
}
