//! `cargo xtask` — orchestration for tare analysis.
//!
//! Commands:
//!   cargo xtask static <crate-root>       — run static analysis only
//!   cargo xtask profile <crate-root> [--bench <name> | --bin <name>]
//!                                         — build with tare-profile, run, attribute
//!   cargo xtask all <crate-root> [--bench <name> | --bin <name>]
//!                                         — both, merged into one JSON

use std::path::{Path, PathBuf};
use std::process::Command;

use tare_aggregate::{from_json, merge, refresh_hashes, to_json};
use tare_collector::dhat_parser::DhatBackend;
use tare_collector::CollectorBackend;
use tare_schema::AllocationReport;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        print_usage();
        std::process::exit(1);
    }

    let command = args[0].as_str();
    let rest = &args[1..];

    match command {
        "static" => cmd_static(rest),
        "profile" => cmd_profile(rest),
        "all" => cmd_all(rest),
        "--help" | "-h" | "help" => {
            print_usage();
        }
        other => {
            eprintln!("Unknown command: {other}");
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("Usage: cargo xtask <command> <crate-root> [options]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  static  <crate-root>                           Static analysis only (no run)");
    eprintln!("  profile <crate-root> [--bench <name>|--bin <name>]  Runtime profiling");
    eprintln!("  all     <crate-root> [--bench <name>|--bin <name>]  Both, merged");
    eprintln!();
    eprintln!("Output: target/tare/allocations.json");
}

enum ProfileTarget {
    DefaultBin,
    Bin(String),
    Bench(String),
}

fn parse_args(args: &[String]) -> (PathBuf, ProfileTarget) {
    if args.is_empty() {
        eprintln!("Error: <crate-root> is required");
        std::process::exit(1);
    }

    let crate_root = PathBuf::from(&args[0]);
    let mut target = ProfileTarget::DefaultBin;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--bench" => {
                i += 1;
                let name = args.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("Error: --bench requires a name");
                    std::process::exit(1);
                });
                target = ProfileTarget::Bench(name);
            }
            "--bin" => {
                i += 1;
                let name = args.get(i).cloned().unwrap_or_else(|| {
                    eprintln!("Error: --bin requires a name");
                    std::process::exit(1);
                });
                target = ProfileTarget::Bin(name);
            }
            other => {
                eprintln!("Unknown option: {other}");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    (crate_root, target)
}

// ── Static ──────────────────────────────────────────────────────────

fn cmd_static(args: &[String]) {
    if args.is_empty() {
        eprintln!("Error: <crate-root> is required");
        std::process::exit(1);
    }

    let crate_root = PathBuf::from(&args[0]);
    let report = run_static_analysis(&crate_root);
    let output_path = output_dir().join("allocations.json");
    write_report(&report, &output_path);
    eprintln!(
        "xtask static: {} files → {}",
        report.files.len(),
        output_path.display()
    );
}

fn run_static_analysis(crate_root: &Path) -> AllocationReport {
    eprintln!("xtask: running static analysis on {} ...", crate_root.display());

    // Run the tare-static binary on the crate root, capture its JSON output.
    let tare_static = find_tare_static_binary();

    let output = Command::new(&tare_static)
        .arg(crate_root)
        .output()
        .unwrap_or_else(|e| {
            eprintln!("Failed to run tare-static at {}: {e}", tare_static.display());
            eprintln!("Hint: run `cargo build -p tare-static` first.");
            std::process::exit(1);
        });

    if !output.status.success() {
        eprintln!(
            "tare-static failed (exit {}):\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        std::process::exit(1);
    }

    let json = String::from_utf8_lossy(&output.stdout);
    from_json(&json).unwrap_or_else(|e| {
        eprintln!("Failed to parse tare-static output: {e}");
        std::process::exit(1);
    })
}

fn find_tare_static_binary() -> PathBuf {
    // Look in target/debug and target/release.
    let candidates = [
        "target/debug/tare-static",
        "target/debug/tare-static.exe",
        "target/release/tare-static",
        "target/release/tare-static.exe",
    ];

    for c in &candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return p;
        }
    }

    // Try building it.
    eprintln!("xtask: building tare-static ...");
    let status = Command::new("cargo")
        .args(["build", "-p", "tare-static"])
        .status()
        .expect("failed to run cargo build");

    if !status.success() {
        eprintln!("Failed to build tare-static");
        std::process::exit(1);
    }

    for c in &candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return p;
        }
    }

    eprintln!("Could not find tare-static binary after building");
    std::process::exit(1);
}

// ── Profile ─────────────────────────────────────────────────────────

fn cmd_profile(args: &[String]) {
    let (crate_root, target) = parse_args(args);
    let report = run_profile(&crate_root, &target);
    let output_path = output_dir().join("allocations.json");
    write_report(&report, &output_path);
    eprintln!(
        "xtask profile: {} files → {}",
        report.files.len(),
        output_path.display()
    );
}

fn run_profile(crate_root: &Path, target: &ProfileTarget) -> AllocationReport {
    let crate_root_abs = crate_root
        .canonicalize()
        .unwrap_or_else(|_| crate_root.to_path_buf());

    // Determine the package name from Cargo.toml.
    let pkg_name = read_package_name(crate_root);

    // Step 1: Build with tare-profile feature.
    eprintln!("xtask: building {} with tare-profile ...", pkg_name);
    let mut build_cmd = Command::new("cargo");
    build_cmd.args(["build", "-p", &pkg_name, "--features", "tare-profile"]);

    match target {
        ProfileTarget::Bench(name) => {
            build_cmd.args(["--bench", name]);
        }
        ProfileTarget::Bin(name) => {
            build_cmd.args(["--bin", name]);
        }
        ProfileTarget::DefaultBin => {}
    }

    let status = build_cmd.status().expect("failed to run cargo build");
    if !status.success() {
        eprintln!("Build failed");
        std::process::exit(1);
    }

    // Step 2: Run the built binary.
    eprintln!("xtask: running profiled binary ...");
    let dhat_json_path = PathBuf::from("dhat-heap.json");

    // Clean up any previous dhat output.
    let _ = std::fs::remove_file(&dhat_json_path);

    let mut run_cmd = Command::new("cargo");
    run_cmd.args(["run", "-p", &pkg_name, "--features", "tare-profile"]);

    match target {
        ProfileTarget::Bench(name) => {
            run_cmd.args(["--bench", name]);
        }
        ProfileTarget::Bin(name) => {
            run_cmd.args(["--bin", name]);
        }
        ProfileTarget::DefaultBin => {}
    }

    let status = run_cmd.status().expect("failed to run profiled binary");
    if !status.success() {
        eprintln!("Profiled run failed");
        std::process::exit(1);
    }

    // Step 3: Parse dhat output and attribute.
    if !dhat_json_path.exists() {
        eprintln!("Error: dhat-heap.json not found after profiled run");
        eprintln!("Make sure the crate uses #[global_allocator] with dhat::Alloc");
        eprintln!("behind the tare-profile feature.");
        std::process::exit(1);
    }

    eprintln!("xtask: attributing allocations ...");
    let backend = DhatBackend::new(&dhat_json_path);
    let allocs = backend.collect().unwrap_or_else(|e| {
        eprintln!("Failed to parse dhat output: {e}");
        std::process::exit(1);
    });

    let now = rfc3339_now();
    let mut report = tare_collector::build_report(&allocs, &crate_root_abs, &now);

    // Refresh hashes from source files on disk.
    refresh_hashes(&mut report, &crate_root_abs);

    report
}

fn read_package_name(crate_root: &Path) -> String {
    let cargo_toml = crate_root.join("Cargo.toml");
    let contents = std::fs::read_to_string(&cargo_toml).unwrap_or_else(|e| {
        eprintln!(
            "Cannot read {}: {e}",
            cargo_toml.display()
        );
        std::process::exit(1);
    });

    // Simple TOML parse — find `name = "..."` under [package].
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name") && trimmed.contains('=') {
            if let Some(name) = trimmed.split('=').nth(1) {
                let name = name.trim().trim_matches('"').trim_matches('\'');
                return name.to_string();
            }
        }
    }

    eprintln!(
        "Could not find package name in {}",
        cargo_toml.display()
    );
    std::process::exit(1);
}

// ── All ─────────────────────────────────────────────────────────────

fn cmd_all(args: &[String]) {
    let (crate_root, target) = parse_args(args);

    let static_report = run_static_analysis(&crate_root);
    let runtime_report = run_profile(&crate_root, &target);

    let mut merged = merge(Some(&runtime_report), Some(&static_report));

    // Refresh all hashes from disk (source may have been recompiled).
    let crate_root_abs = crate_root
        .canonicalize()
        .unwrap_or_else(|_| crate_root.to_path_buf());
    refresh_hashes(&mut merged, &crate_root_abs);

    let output_path = output_dir().join("allocations.json");
    write_report(&merged, &output_path);

    let runtime_lines: usize = runtime_report
        .files
        .values()
        .map(|f| f.lines.len())
        .sum();
    let static_lines: usize = static_report
        .files
        .values()
        .map(|f| f.lines.len())
        .sum();
    let merged_lines: usize = merged.files.values().map(|f| f.lines.len()).sum();

    eprintln!(
        "xtask all: {} runtime lines + {} static lines → {} merged lines in {} files → {}",
        runtime_lines,
        static_lines,
        merged_lines,
        merged.files.len(),
        output_path.display()
    );
}

// ── Helpers ─────────────────────────────────────────────────────────

fn output_dir() -> PathBuf {
    let dir = PathBuf::from("target/tare");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn write_report(report: &AllocationReport, path: &Path) {
    let json = to_json(report);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(path, json).unwrap_or_else(|e| {
        eprintln!("Failed to write {}: {e}", path.display());
        std::process::exit(1);
    });
}

fn rfc3339_now() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days = secs / 86400;
    let time_secs = secs % 86400;
    let h = time_secs / 3600;
    let m = (time_secs % 3600) / 60;
    let s = time_secs % 60;

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
