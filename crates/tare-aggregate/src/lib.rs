//! `tare-aggregate` — merge runtime and static allocation reports into one.
//!
//! Takes two `AllocationReport`s (one from tare-static, one from tare-collector)
//! and merges them into a single report. Entries from both sources are combined
//! per file+line, with content hashes refreshed from disk.

use std::collections::BTreeMap;
use std::path::Path;
use tare_schema::{AllocationReport, FileData, LineData, SCHEMA_VERSION};

/// Merge two reports into one.
///
/// - Entries from both reports are combined per file+line.
/// - Content hashes are taken from `runtime` first (since it was generated
///   from a build, which requires the source to compile), falling back to
///   `static_report`, falling back to recomputing from disk.
/// - The `workspace_root` and `generated_at` come from whichever report
///   is available (runtime preferred).
pub fn merge(
    runtime: Option<&AllocationReport>,
    static_report: Option<&AllocationReport>,
) -> AllocationReport {
    let (workspace_root, generated_at) = match (runtime, static_report) {
        (Some(r), _) => (r.workspace_root.clone(), r.generated_at.clone()),
        (_, Some(s)) => (s.workspace_root.clone(), s.generated_at.clone()),
        (None, None) => return AllocationReport::new("", ""),
    };

    let mut merged = AllocationReport::new(&workspace_root, &generated_at);

    // Collect all file keys from both reports.
    let mut all_files: BTreeMap<String, (Option<&FileData>, Option<&FileData>)> = BTreeMap::new();

    if let Some(r) = runtime {
        for (path, data) in &r.files {
            all_files.entry(path.clone()).or_default().0 = Some(data);
        }
    }
    if let Some(s) = static_report {
        for (path, data) in &s.files {
            all_files.entry(path.clone()).or_default().1 = Some(data);
        }
    }

    for (file_path, (rt_data, st_data)) in &all_files {
        let content_hash = rt_data
            .map(|d| d.content_hash.clone())
            .or_else(|| st_data.map(|d| d.content_hash.clone()))
            .unwrap_or_else(|| "unknown".to_string());

        let mut merged_lines: BTreeMap<String, LineData> = BTreeMap::new();

        // Merge runtime entries.
        if let Some(rd) = rt_data {
            for (line, line_data) in &rd.lines {
                merged_lines
                    .entry(line.clone())
                    .or_insert_with(|| LineData {
                        entries: Vec::new(),
                    })
                    .entries
                    .extend(line_data.entries.iter().cloned());
            }
        }

        // Merge static entries.
        if let Some(sd) = st_data {
            for (line, line_data) in &sd.lines {
                merged_lines
                    .entry(line.clone())
                    .or_insert_with(|| LineData {
                        entries: Vec::new(),
                    })
                    .entries
                    .extend(line_data.entries.iter().cloned());
            }
        }

        merged.files.insert(
            file_path.clone(),
            FileData {
                content_hash,
                lines: merged_lines,
            },
        );
    }

    merged
}

/// Refresh content hashes in a report by reading files from disk.
///
/// For each file in the report, compute a fresh blake3 hash from the
/// source file on disk (relative to `workspace_root`). If the file
/// can't be read, keep the existing hash.
pub fn refresh_hashes(report: &mut AllocationReport, workspace_root: &Path) {
    for (file_path, file_data) in &mut report.files {
        let abs_path = workspace_root.join(file_path);
        if let Ok(contents) = std::fs::read(&abs_path) {
            file_data.content_hash = blake3::hash(&contents).to_hex().to_string();
        }
    }
}

/// Serialize a report to JSON.
pub fn to_json(report: &AllocationReport) -> String {
    serde_json::to_string_pretty(report).expect("failed to serialize report")
}

/// Deserialize a report from JSON.
pub fn from_json(json: &str) -> Result<AllocationReport, String> {
    let report: AllocationReport =
        serde_json::from_str(json).map_err(|e| format!("invalid report JSON: {e}"))?;
    if report.version != SCHEMA_VERSION {
        return Err(format!(
            "schema version mismatch: expected {}, got {}",
            SCHEMA_VERSION, report.version
        ));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tare_schema::{Entry, StackTrace};

    fn runtime_report() -> AllocationReport {
        let mut report = AllocationReport::new("/project", "2026-07-17T12:00:00Z");
        let mut lines = BTreeMap::new();
        lines.insert(
            "10".to_string(),
            LineData {
                entries: vec![Entry::runtime_heap(
                    1024,
                    5,
                    512,
                    vec![StackTrace {
                        frames: vec!["app::run".to_string()],
                        bytes: 1024,
                    }],
                )],
            },
        );
        lines.insert(
            "20".to_string(),
            LineData {
                entries: vec![Entry::runtime_heap(48, 1, 48, vec![])],
            },
        );
        report.files.insert(
            "src/main.rs".to_string(),
            FileData {
                content_hash: "rt_hash_abc".to_string(),
                lines,
            },
        );
        report
    }

    fn static_report() -> AllocationReport {
        let mut report = AllocationReport::new("/project", "2026-07-17T12:00:00Z");
        let mut lines = BTreeMap::new();
        lines.insert(
            "10".to_string(),
            LineData {
                entries: vec![Entry::static_alloc_site(
                    vec!["Vec::with_capacity".to_string()],
                    Some("n * size_of::<T>()".to_string()),
                )],
            },
        );
        lines.insert(
            "30".to_string(),
            LineData {
                entries: vec![Entry::static_alloc_site(
                    vec!["format!".to_string()],
                    None,
                )],
            },
        );
        report.files.insert(
            "src/main.rs".to_string(),
            FileData {
                content_hash: "st_hash_def".to_string(),
                lines,
            },
        );
        // A file only in static.
        let mut lib_lines = BTreeMap::new();
        lib_lines.insert(
            "5".to_string(),
            LineData {
                entries: vec![Entry::static_alloc_site(
                    vec!["Box::new".to_string()],
                    None,
                )],
            },
        );
        report.files.insert(
            "src/lib.rs".to_string(),
            FileData {
                content_hash: "lib_hash_ghi".to_string(),
                lines: lib_lines,
            },
        );
        report
    }

    #[test]
    fn merge_combines_entries_on_same_line() {
        let rt = runtime_report();
        let st = static_report();
        let merged = merge(Some(&rt), Some(&st));

        let main_rs = merged.files.get("src/main.rs").unwrap();

        // Line 10 should have both runtime and static entries.
        let line10 = &main_rs.lines["10"];
        assert_eq!(line10.entries.len(), 2);
        assert!(line10
            .entries
            .iter()
            .any(|e| e.source == tare_schema::Source::Runtime));
        assert!(line10
            .entries
            .iter()
            .any(|e| e.source == tare_schema::Source::Static));
    }

    #[test]
    fn merge_preserves_unique_lines() {
        let rt = runtime_report();
        let st = static_report();
        let merged = merge(Some(&rt), Some(&st));

        let main_rs = merged.files.get("src/main.rs").unwrap();

        // Line 20: runtime only.
        assert!(main_rs.lines.contains_key("20"));
        assert_eq!(main_rs.lines["20"].entries.len(), 1);
        assert_eq!(
            main_rs.lines["20"].entries[0].source,
            tare_schema::Source::Runtime
        );

        // Line 30: static only.
        assert!(main_rs.lines.contains_key("30"));
        assert_eq!(main_rs.lines["30"].entries.len(), 1);
        assert_eq!(
            main_rs.lines["30"].entries[0].source,
            tare_schema::Source::Static
        );
    }

    #[test]
    fn merge_preserves_files_from_single_source() {
        let rt = runtime_report();
        let st = static_report();
        let merged = merge(Some(&rt), Some(&st));

        // src/lib.rs only in static.
        assert!(merged.files.contains_key("src/lib.rs"));
        let lib_rs = merged.files.get("src/lib.rs").unwrap();
        assert_eq!(lib_rs.content_hash, "lib_hash_ghi");
    }

    #[test]
    fn merge_prefers_runtime_hash() {
        let rt = runtime_report();
        let st = static_report();
        let merged = merge(Some(&rt), Some(&st));

        // Runtime hash takes priority for shared files.
        let main_rs = merged.files.get("src/main.rs").unwrap();
        assert_eq!(main_rs.content_hash, "rt_hash_abc");
    }

    #[test]
    fn merge_runtime_only() {
        let rt = runtime_report();
        let merged = merge(Some(&rt), None);
        assert_eq!(merged.files.len(), 1);
        assert!(merged.files.contains_key("src/main.rs"));
    }

    #[test]
    fn merge_static_only() {
        let st = static_report();
        let merged = merge(None, Some(&st));
        assert_eq!(merged.files.len(), 2);
    }

    #[test]
    fn merge_nothing() {
        let merged = merge(None, None);
        assert!(merged.files.is_empty());
    }

    #[test]
    fn json_round_trip() {
        let rt = runtime_report();
        let st = static_report();
        let merged = merge(Some(&rt), Some(&st));

        let json = to_json(&merged);
        let parsed = from_json(&json).unwrap();
        assert_eq!(merged, parsed);
    }

    #[test]
    fn from_json_rejects_wrong_version() {
        let json = r#"{"version":99,"generated_at":"","workspace_root":"","files":{}}"#;
        let result = from_json(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("version mismatch"));
    }
}
