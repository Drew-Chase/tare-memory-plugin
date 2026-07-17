//! `tare-schema` — the JSON contract between the Rust analysis tools and the
//! JetBrains plugin.
//!
//! One JSON file, keyed `file → line → entries`. Both the static analyzer and
//! the runtime collector append entries; the plugin renders by `source`/`kind`.
//!
//! **This crate is the only coupling between the Rust side and the plugin.**
//! Changing the schema is a deliberate, versioned act.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Current schema version. Bump when the shape changes.
pub const SCHEMA_VERSION: u32 = 1;

/// Root of the allocation report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllocationReport {
    /// Schema version — always [`SCHEMA_VERSION`].
    pub version: u32,

    /// RFC 3339 timestamp of when this report was generated.
    pub generated_at: String,

    /// Absolute path to the workspace root the report was generated from.
    pub workspace_root: String,

    /// Per-file allocation data, keyed by path relative to `workspace_root`.
    pub files: BTreeMap<String, FileData>,
}

/// Allocation data for a single source file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileData {
    /// Hex-encoded blake3 hash of the file's contents at generation time.
    /// The plugin compares this against the open document to detect staleness.
    pub content_hash: String,

    /// Per-line allocation entries, keyed by 1-based line number (as a string
    /// for JSON compatibility).
    pub lines: BTreeMap<String, LineData>,
}

/// All allocation entries for a single source line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineData {
    pub entries: Vec<Entry>,
}

/// A single allocation entry. The `source` discriminates runtime vs. static;
/// `kind` discriminates the specific type of information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    /// Where this entry came from.
    pub source: Source,

    /// What kind of entry this is.
    pub kind: Kind,

    /// Runtime: cumulative bytes allocated at this line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,

    /// Runtime: number of allocations at this line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,

    /// Runtime: peak (live) bytes at this line at the point of maximum heap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_bytes: Option<u64>,

    /// Runtime: call stacks that led to allocations at this line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stacks: Option<Vec<StackTrace>>,

    /// Static: the allocation-site constructs found (e.g. `["Vec::with_capacity"]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constructs: Option<Vec<String>>,

    /// Static: a human-readable hint about the amount, e.g.
    /// `"n * size_of::<Row>()"`. **Never a concrete number presented as fact.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_hint: Option<String>,

    /// Static (type_size): the type name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,

    /// Static (type_size): stack size in bytes (upper bound, not post-codegen truth).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_bytes: Option<u64>,
}

/// Discriminates the data source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Runtime,
    Static,
}

/// Discriminates the kind of allocation information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Runtime: cumulative heap allocation stats for this line.
    HeapCumulative,
    /// Static: an allocation-site construct was found on this line.
    AllocSite,
    /// Static: a type's stack size (upper bound).
    TypeSize,
}

/// A call stack captured at allocation time (runtime only).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StackTrace {
    /// Frames from deepest (user code) to shallowest (allocator internals).
    pub frames: Vec<String>,

    /// Bytes attributed to this particular call path.
    pub bytes: u64,
}

impl AllocationReport {
    /// Create a new empty report with the current schema version.
    pub fn new(workspace_root: impl Into<String>, generated_at: impl Into<String>) -> Self {
        Self {
            version: SCHEMA_VERSION,
            generated_at: generated_at.into(),
            workspace_root: workspace_root.into(),
            files: BTreeMap::new(),
        }
    }
}

impl Entry {
    /// Create a runtime heap-cumulative entry.
    pub fn runtime_heap(bytes: u64, count: u64, peak_bytes: u64, stacks: Vec<StackTrace>) -> Self {
        Self {
            source: Source::Runtime,
            kind: Kind::HeapCumulative,
            bytes: Some(bytes),
            count: Some(count),
            peak_bytes: Some(peak_bytes),
            stacks: Some(stacks),
            constructs: None,
            amount_hint: None,
            ty: None,
            stack_bytes: None,
        }
    }

    /// Create a static allocation-site entry.
    pub fn static_alloc_site(constructs: Vec<String>, amount_hint: Option<String>) -> Self {
        Self {
            source: Source::Static,
            kind: Kind::AllocSite,
            bytes: None,
            count: None,
            peak_bytes: None,
            stacks: None,
            constructs: Some(constructs),
            amount_hint,
            ty: None,
            stack_bytes: None,
        }
    }

    /// Create a static type-size entry.
    pub fn static_type_size(ty: impl Into<String>, stack_bytes: u64) -> Self {
        Self {
            source: Source::Static,
            kind: Kind::TypeSize,
            bytes: None,
            count: None,
            peak_bytes: None,
            stacks: None,
            constructs: None,
            amount_hint: None,
            ty: Some(ty.into()),
            stack_bytes: Some(stack_bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_empty_report() {
        let report = AllocationReport::new("/home/user/project", "2026-07-17T12:00:00Z");
        let json = serde_json::to_string_pretty(&report).unwrap();
        let parsed: AllocationReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, parsed);
    }

    #[test]
    fn round_trip_full_report() {
        let mut report = AllocationReport::new("/home/user/project", "2026-07-17T12:00:00Z");

        let mut lines = BTreeMap::new();
        lines.insert(
            "42".to_string(),
            LineData {
                entries: vec![
                    Entry::runtime_heap(
                        393216,
                        3,
                        131072,
                        vec![StackTrace {
                            frames: vec![
                                "sample::build_rows".to_string(),
                                "alloc::raw_vec::RawVec<T,A>::grow".to_string(),
                            ],
                            bytes: 393216,
                        }],
                    ),
                    Entry::static_alloc_site(
                        vec!["Vec::with_capacity".to_string()],
                        Some("n * size_of::<Row>()".to_string()),
                    ),
                    Entry::static_type_size("Row", 24),
                ],
            },
        );

        report.files.insert(
            "src/foo.rs".to_string(),
            FileData {
                content_hash: "abc123def456".to_string(),
                lines,
            },
        );

        let json = serde_json::to_string_pretty(&report).unwrap();
        let parsed: AllocationReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, parsed);
    }

    #[test]
    fn runtime_entry_omits_static_fields() {
        let entry = Entry::runtime_heap(1024, 1, 1024, vec![]);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("constructs"));
        assert!(!json.contains("amount_hint"));
        assert!(!json.contains("ty"));
        assert!(!json.contains("stack_bytes"));
    }

    #[test]
    fn static_entry_omits_runtime_fields() {
        let entry = Entry::static_alloc_site(vec!["Box::new".to_string()], None);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("\"bytes\""));
        assert!(!json.contains("count"));
        assert!(!json.contains("peak_bytes"));
        assert!(!json.contains("stacks"));
    }

    #[test]
    fn source_and_kind_serialize_snake_case() {
        let json = serde_json::to_string(&Source::Runtime).unwrap();
        assert_eq!(json, "\"runtime\"");

        let json = serde_json::to_string(&Source::Static).unwrap();
        assert_eq!(json, "\"static\"");

        let json = serde_json::to_string(&Kind::HeapCumulative).unwrap();
        assert_eq!(json, "\"heap_cumulative\"");

        let json = serde_json::to_string(&Kind::AllocSite).unwrap();
        assert_eq!(json, "\"alloc_site\"");

        let json = serde_json::to_string(&Kind::TypeSize).unwrap();
        assert_eq!(json, "\"type_size\"");
    }

    /// Load every fixture file in `fixtures/` and verify it round-trips.
    #[test]
    fn fixtures_parse_and_round_trip() {
        let fixtures_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures");

        if !fixtures_dir.exists() {
            panic!("fixtures/ directory not found at {}", fixtures_dir.display());
        }

        for entry in std::fs::read_dir(&fixtures_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                let contents = std::fs::read_to_string(&path).unwrap();
                let report: AllocationReport = serde_json::from_str(&contents)
                    .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()));

                // Round-trip
                let json = serde_json::to_string_pretty(&report).unwrap();
                let reparsed: AllocationReport = serde_json::from_str(&json).unwrap();
                assert_eq!(report, reparsed, "Round-trip failed for {}", path.display());

                // Version check
                assert_eq!(
                    report.version, SCHEMA_VERSION,
                    "Fixture {} has wrong version",
                    path.display()
                );
            }
        }
    }
}
