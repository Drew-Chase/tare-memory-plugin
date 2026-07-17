use std::path::PathBuf;
use tare_schema::AllocationReport;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: tare-static <crate-root> [--output <path>]");
        eprintln!("  Walks all .rs files under <crate-root>/src and outputs schema JSON.");
        std::process::exit(1);
    }

    let crate_root = PathBuf::from(&args[1]);
    let output_path = args
        .iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    if !crate_root.is_dir() {
        eprintln!("Error: {} is not a directory", crate_root.display());
        std::process::exit(1);
    }

    let src_dir = crate_root.join("src");
    if !src_dir.is_dir() {
        eprintln!("Error: no src/ directory found in {}", crate_root.display());
        std::process::exit(1);
    }

    let workspace_root = crate_root
        .canonicalize()
        .unwrap_or_else(|_| crate_root.clone())
        .to_string_lossy()
        .to_string();

    let now = rfc3339_now();

    let mut report = AllocationReport::new(&workspace_root, &now);

    let mut file_count = 0u32;
    let mut site_count = 0u32;

    for entry in walkdir::WalkDir::new(&crate_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "rs")
        })
    {
        let path = entry.path();
        let Ok(source) = std::fs::read_to_string(path) else {
            eprintln!("Warning: could not read {}", path.display());
            continue;
        };

        let file_data = tare_static::analyze_to_file_data(&source, path);
        let entry_count: usize = file_data.lines.values().map(|l| l.entries.len()).sum();
        site_count += entry_count as u32;

        // Compute relative path from crate root.
        let rel_path = path
            .strip_prefix(&crate_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if !file_data.lines.is_empty() {
            file_count += 1;
            report.files.insert(rel_path, file_data);
        }
    }

    let json = serde_json::to_string_pretty(&report).expect("failed to serialize report");

    if let Some(out) = output_path {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&out, &json).unwrap_or_else(|e| {
            eprintln!("Error writing {}: {e}", out.display());
            std::process::exit(1);
        });
        eprintln!(
            "tare-static: {site_count} sites in {file_count} files → {}",
            out.display()
        );
    } else {
        println!("{json}");
        eprintln!("tare-static: {site_count} sites in {file_count} files");
    }
}

/// Minimal RFC 3339 UTC timestamp without pulling in a datetime crate.
fn rfc3339_now() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert epoch seconds to date/time components.
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let h = time_secs / 3600;
    let m = (time_secs % 3600) / 60;
    let s = time_secs % 60;

    // Civil date from day count (algorithm from Howard Hinnant).
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };

    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}
