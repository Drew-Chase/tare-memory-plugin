# tare

Inline Rust memory-allocation viewer for JetBrains IDEs.

Annotates your Rust source lines with allocation data from two sources,
merged into one view:

- **Runtime** — real heap bytes captured by [dhat](https://docs.rs/dhat)
  during a profiling run, attributed to the source line that triggered
  each allocation (including allocations deep inside library calls).
- **Static** — allocation *sites* flagged by a
  [syn](https://docs.rs/syn)-based pass (`Box::new`, `Vec::with_capacity`,
  `.collect()`, `.clone()`, `format!`, etc.). No run needed. Approximate.

Both feed one JSON contract, rendered by one IntelliJ plugin as inlay
hints and gutter icons.

## Honesty rules

These are the reason the tool exists. Breaking one makes the tool lie.

1. **Static entries are sites, never amounts.** Heap amounts are runtime
   values (`Vec::with_capacity(n)`, growth doubling) and are fundamentally
   unknowable statically. Static entries may carry a hint string
   (e.g., `"capacity x element size"`) — never a number presented as fact.

2. **Runtime entries only for lines that executed.** Never render "0 bytes"
   for an unexecuted line — render nothing.

3. **Never track line numbers live after edits.** When a file's content
   hash no longer matches the hash recorded at generation time, the
   plugin greys out that file's hints with a "re-run to refresh"
   indicator. It does **not** attempt to shift stale numbers.

4. **Type size is an upper bound, not the truth.** Post-codegen stack
   frames coalesce, reuse, and spill. Type-size hints are labeled as
   upper bounds.

5. **The tool is a profiler surfaced inline, not a static analyzer.**

## Quickstart

### 1. Add tare-profile support to your crate

In your `Cargo.toml`:

```toml
[features]
tare-profile = ["dep:dhat"]

[dependencies]
dhat = { version = "0.3", optional = true }
```

In your `main.rs` (or workload binary):

```rust
#[cfg(feature = "tare-profile")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    #[cfg(feature = "tare-profile")]
    let _profiler = dhat::Profiler::new_heap();

    // ... your workload ...
}
```

### 2. Run the analysis

From your workspace root (where `tare/` is checked out alongside your
crate, or add the tare crates as path dependencies):

```sh
# Static analysis only (no run needed)
cargo xtask static path/to/your/crate

# Runtime profiling only
cargo xtask profile path/to/your/crate

# Both, merged into one report
cargo xtask all path/to/your/crate

# Profile a specific benchmark
cargo xtask profile path/to/your/crate --bench my_bench
```

Output lands in `target/tare/allocations.json`.

### 3. Install the plugin

Build the plugin from `plugin/`:

```sh
cd plugin
./gradlew buildPlugin
```

Install the resulting ZIP from **Settings > Plugins > Install Plugin
from Disk**.

### 4. Open your project

Open your Rust project in RustRover / IntelliJ IDEA. The plugin
automatically loads `target/tare/allocations.json` and renders:

- **Inlay hints** at line ends:
  - Runtime: `23.4 KiB, 1000 allocs`
  - Static: `site: Vec::with_capacity (capacity x element size)`
  - Stale: `⟳ re-run to refresh`

- **Gutter icons** with tooltips showing byte breakdown and call stacks.

### 5. One-button re-profiling

Add a **Tare Profile** run configuration (Run > Edit Configurations >
+  > Tare Profile). Set the crate root and command. Running it
executes `cargo xtask`, then automatically refreshes the hints.

## Configuration

**Settings > Tools > Tare Allocation Viewer:**

| Setting | Default | Description |
|---------|---------|-------------|
| Enabled | on | Show/hide all tare hints |
| Min bytes | 0 | Hide runtime hints below this threshold |
| Metric | Cumulative | Which metric to display: cumulative bytes, peak bytes, or allocation count |

## Architecture

```
tare/
  crates/
    tare-schema/       JSON contract types (the only coupling)
    tare-static/       syn-based allocation-site analyzer + CLI
    tare-collector/    dhat output parser + backtrace attribution
    tare-aggregate/    merges runtime + static reports
    xtask/             cargo xtask orchestration
  sample/              target crate with known alloc patterns
  plugin/              Kotlin IntelliJ plugin
  fixtures/            hand-crafted schema JSON for TDD
```

### The contract is the seam

One JSON file (`target/tare/allocations.json`), keyed
`file -> line -> entries`, with `source` (`runtime`|`static`) and `kind`
discriminators. Both analyzers write entries; the plugin renders by
source/kind. Changing the schema is a deliberate, versioned act.

### divan compatibility

divan's `AllocProfiler` and dhat's `Alloc` are both `#[global_allocator]`
implementations — only one can exist per binary. Resolution: profiling
runs use dhat behind the `tare-profile` feature flag; timing benchmarks
use divan separately. The two are never mixed.

## DIY allocator (replacing dhat)

The collector is behind a `CollectorBackend` trait. To replace dhat with
a custom `GlobalAlloc`:

1. Implement `CollectorBackend` in `tare-collector`
2. Capture unresolved backtraces at alloc time (never symbolize hot-path)
3. Guard with a thread-local `Cell<bool>` to prevent reentrancy
4. Sample (size-weighted Poisson) to avoid per-alloc overhead
5. Resolve to file:line offline at serialization
6. Build with `-C force-frame-pointers=yes` for faster unwinds

The rest of the pipeline (attribution, aggregation, plugin) stays the
same.
