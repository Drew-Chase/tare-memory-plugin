//! `tare-collector` — runtime allocation tracking and attribution.
//!
//! For v1, this crate **post-processes dhat's JSON output** rather than
//! implementing a custom `GlobalAlloc`. dhat captures every heap allocation
//! with backtraces; we parse its output, attribute each allocation to the
//! deepest frame under the workspace root, and emit tare-schema JSON.
//!
//! The design is behind a trait seam ([`CollectorBackend`]) so that a
//! hand-rolled `GlobalAlloc` can replace dhat in v2 without changing
//! downstream code.

pub mod dhat_parser;
pub mod attribution;

use std::collections::BTreeMap;
use std::path::Path;
use tare_schema::{AllocationReport, Entry, FileData, LineData, StackTrace};

/// Trait seam for allocation collector backends.
///
/// A backend provides raw allocation records from a profiling run.
/// The v1 implementation parses dhat's JSON; a v2 implementation could
/// capture allocations directly via `GlobalAlloc`.
pub trait CollectorBackend {
    /// Parse profiling output and return raw allocation records.
    fn collect(&self) -> Result<Vec<RawAllocation>, CollectorError>;
}

/// A single allocation record extracted from the backend.
#[derive(Debug, Clone)]
pub struct RawAllocation {
    /// Total bytes allocated at this program point.
    pub total_bytes: u64,
    /// Total number of allocation events at this program point.
    pub total_count: u64,
    /// Peak (max live) bytes at this program point.
    pub peak_bytes: u64,
    /// Resolved stack frames, from deepest (user code) to shallowest.
    /// Each frame is a string like `"sample::build_rows (sample/src/lib.rs:12)"`.
    pub frames: Vec<ResolvedFrame>,
}

/// A resolved stack frame with file/line info.
#[derive(Debug, Clone)]
pub struct ResolvedFrame {
    /// The function name (demangled).
    pub function: String,
    /// Source file path (as it appears in the debug info).
    pub file: Option<String>,
    /// 1-based line number.
    pub line: Option<u32>,
}

#[derive(Debug)]
pub enum CollectorError {
    Io(std::io::Error),
    Parse(String),
}

impl From<std::io::Error> for CollectorError {
    fn from(e: std::io::Error) -> Self {
        CollectorError::Io(e)
    }
}

impl std::fmt::Display for CollectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectorError::Io(e) => write!(f, "I/O error: {e}"),
            CollectorError::Parse(msg) => write!(f, "Parse error: {msg}"),
        }
    }
}

impl std::error::Error for CollectorError {}

/// Build a tare-schema `AllocationReport` from raw allocations.
///
/// `workspace_root` is the absolute path to the workspace; only frames
/// whose file paths are under this root are considered for attribution.
/// The deepest such frame is the attribution target.
pub fn build_report(
    allocations: &[RawAllocation],
    workspace_root: &Path,
    generated_at: &str,
) -> AllocationReport {
    let ws_root_str = workspace_root
        .to_string_lossy()
        .replace('\\', "/");

    let mut report = AllocationReport::new(&ws_root_str, generated_at);
    let attributed = attribution::attribute(allocations, &ws_root_str);

    for ((file, line), records) in &attributed {
        let file_data = report
            .files
            .entry(file.clone())
            .or_insert_with(|| {
                // Compute content hash if the file exists on disk.
                let abs_path = workspace_root.join(file);
                let content_hash = std::fs::read(&abs_path)
                    .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                FileData {
                    content_hash,
                    lines: BTreeMap::new(),
                }
            });

        let mut total_bytes: u64 = 0;
        let mut total_count: u64 = 0;
        let mut peak_bytes: u64 = 0;
        let mut stacks: Vec<StackTrace> = Vec::new();

        for alloc in records {
            total_bytes += alloc.total_bytes;
            total_count += alloc.total_count;
            // Peak is the max across contributing program points.
            if alloc.peak_bytes > peak_bytes {
                peak_bytes = alloc.peak_bytes;
            }

            let frame_strings: Vec<String> = alloc
                .frames
                .iter()
                .map(|f| {
                    if let (Some(file), Some(line)) = (&f.file, f.line) {
                        format!("{} ({}:{})", f.function, file, line)
                    } else {
                        f.function.clone()
                    }
                })
                .collect();

            stacks.push(StackTrace {
                frames: frame_strings,
                bytes: alloc.total_bytes,
            });
        }

        let line_data = file_data
            .lines
            .entry(line.to_string())
            .or_insert_with(|| LineData {
                entries: Vec::new(),
            });

        line_data
            .entries
            .push(Entry::runtime_heap(total_bytes, total_count, peak_bytes, stacks));
    }

    report
}
