# tare — inline Rust memory-allocation viewer for JetBrains IDEs
# Run `just` with no args to see all available recipes.

set windows-shell := ["powershell.exe", "-NoProfile", "-NoLogo", "-Command"]


# Default recipe: list available recipes
default:
    @just --list --unsorted

# ─── Build ────────────────────────────────────────────────────────────

# Build the entire workspace (debug)
build:
    @cargo build --workspace

# Build the entire workspace (release)
build-release:
    @cargo build --workspace --release

# Build only the tare-static CLI
build-static:
    @cargo build -p tare-static

# Build only the xtask binary
build-xtask:
    @cargo build -p xtask

# Build the sample crate (without profiling)
build-sample:
    @cargo build -p sample

# Build the sample crate with dhat profiling enabled
build-sample-profile:
   @cargo build -p sample --features tare-profile

# ─── Test ─────────────────────────────────────────────────────────────

# Run all workspace tests
test:
    @cargo test --workspace

# Run tests for a specific crate
test-crate crate:
    @cargo test -p {{crate}}

# Run tare-schema tests (includes fixture validation)
test-schema:
    @cargo test -p tare-schema

# Run tare-static tests (includes sample crate validation)
test-static:
    @cargo test -p tare-static

# Run tare-collector tests (includes dhat output parsing if available)
test-collector:
    @cargo test -p tare-collector

# Run tare-aggregate tests
test-aggregate:
    @cargo test -p tare-aggregate

# Run tests with output shown (for debugging)
test-verbose:
    @cargo test --workspace -- --nocapture

# ─── Lint & Format ───────────────────────────────────────────────────

# Check formatting
fmt-check:
    @cargo fmt --all -- --check

# Format all code
fmt:
    @cargo fmt --all

# Run clippy on the workspace
clippy:
    @cargo clippy --workspace --all-targets -- -D warnings

# Run clippy including the tare-profile feature on sample
clippy-all:
    @cargo clippy --workspace --all-targets -- -D warnings
    @cargo clippy -p sample --features tare-profile -- -D warnings

# Full lint: format check + clippy
lint: fmt-check clippy

# ─── Analysis (xtask) ────────────────────────────────────────────────

# Run static analysis on the sample crate
static: build-static
    @cargo xtask static sample/

# Run static analysis on a given crate
static-crate crate_root:
    @cargo xtask static {{crate_root}}

# Profile the sample crate (runtime, default binary)
profile: build-sample-profile
    @cargo xtask profile sample/

# Profile a specific benchmark in the sample crate
profile-bench bench_name="alloc_bench":
    @cargo xtask profile sample/ --bench {{bench_name}}

# Profile a given crate (runtime, default binary)
profile-crate crate_root:
    @cargo xtask profile {{crate_root}}

# Run both static + runtime analysis on the sample crate, merged
all: build-static build-sample-profile
    @cargo xtask all sample/

# Run both analyses on a given crate, merged
all-crate crate_root:
    @cargo xtask all {{crate_root}}

# ─── Sample Crate ────────────────────────────────────────────────────

# Run the sample crate (no profiling)
run-sample:
    @cargo run -p sample

# Run the sample crate with dhat profiling, producing dhat-heap.json
run-sample-profile:
    @cargo run -p sample --features tare-profile

# Run divan benchmarks on the sample crate (timing only, no dhat)
bench-sample:
   @cargo bench -p sample --bench alloc_bench

# ─── Plugin ──────────────────────────────────────────────────────────

# Build a distributable plugin ZIP (install via Settings > Plugins > Install from Disk)
dist: build-release
    cd plugin && ./gradlew buildPlugin
    @echo ""
    @echo "Plugin ZIP ready at plugin/build/distributions/"
    @echo "Install: Settings > Plugins > gear icon > Install Plugin from Disk"

# Build the IntelliJ plugin (requires Gradle)
plugin-build:
    cd plugin && ./gradlew buildPlugin

# Run the plugin in an IDE sandbox (requires Gradle)
plugin-run:
    cd plugin && ./gradlew runIde

# Verify plugin compatibility
plugin-verify:
    cd plugin && ./gradlew verifyPlugin

# Clean plugin build artifacts
plugin-clean:
    cd plugin && ./gradlew clean

# ─── Full Pipelines ──────────────────────────────────────────────────

# Full CI check: format, clippy, test
ci: lint test

# End-to-end: build everything, test, then run full analysis on sample
e2e: build test all
    @echo ""
    @echo "End-to-end complete. Report at target/tare/allocations.json"

# Generate a fresh dhat-heap.json from the sample crate
generate-dhat: run-sample-profile
    @echo "dhat-heap.json generated in project root"

# ─── Inspect ─────────────────────────────────────────────────────────

# Show the merged allocation report (pretty-printed)
show-report:
    @cat target/tare/allocations.json 2>/dev/null || echo "No report found. Run 'just all' first."

# Show which lines have allocation data in the report
show-lines:
    @cat target/tare/allocations.json 2>/dev/null \
        | grep -oP '"(src/[^"]+)"|"(\d+)"' \
        | paste - - 2>/dev/null \
        || echo "No report found. Run 'just all' first."

# Run tare-static on sample and print to stdout
inspect-static:
    cargo run -p tare-static -- sample/

# ─── Clean ───────────────────────────────────────────────────────────

# Clean Rust build artifacts
clean:
    cargo clean

# Clean all generated files (Rust + plugin + dhat output)
clean-all: clean
    rm -f dhat-heap.json
    rm -rf target/tare/
    -cd plugin && ./gradlew clean 2>/dev/null

# Remove only the tare report (keeps build cache)
clean-report:
    rm -rf target/tare/
    rm -f dhat-heap.json

# ─── Dev Helpers ─────────────────────────────────────────────────────

# Watch and re-run tests on file changes (requires cargo-watch)
watch-test:
    cargo watch -x 'test --workspace'

# Watch and re-run a specific crate's tests
watch-test-crate crate:
    cargo watch -x 'test -p {{crate}}'

# Print workspace dependency tree
deps:
    cargo tree --workspace

# Count lines of Rust code in the workspace
loc:
    @find crates/ sample/src -name '*.rs' -exec cat {} + | wc -l

# Show the schema version
schema-version:
    @cargo run -p tare-static -- sample/ 2>/dev/null | head -5
