# DECISIONS.md

Running log of verified facts, version choices, and design decisions.
Consult before making changes that touch these areas.

---

## 2026-07-17 — Phase 0: Version & API verification

### Crate versions (verified against crates.io / docs.rs)

| Crate     | Version  | Notes |
|-----------|----------|-------|
| `dhat`    | 0.3.3    | Last release >2 years ago. Stable API. |
| `divan`   | 0.1.21   | Requires Rust ≥1.80. |
| `syn`     | 2.0.118  | Active development (2026-06-16). |
| `blake3`  | 1.8.5    | Fast hash, public-domain. Using for content hashes. |
| `serde`   | 1.x      | Standard; `serde_json` for serialization. |

### dhat global allocator setup

```rust
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    let _profiler = dhat::Profiler::new_heap();
    // … workload …
    // profiler dropped → writes dhat-heap.json
}
```

- dhat tracks **all** heap allocations (no sampling). This is fine for our
  use case because profiling runs are separate, short-lived workloads.
- Only one `Profiler` may exist at a time (panics otherwise).
- dhat's JSON output is for its own viewer — we will **not** use dhat's JSON
  directly. Instead we capture allocations via dhat's tracking, then post-process
  into our tare-schema JSON.

### divan × global allocator conflict

- `divan::AllocProfiler` is itself a `#[global_allocator]` (`impl GlobalAlloc`).
- Only one `#[global_allocator]` per binary → dhat::Alloc and divan::AllocProfiler
  **cannot coexist in the same binary**.
- **Resolution:** profiling runs use a **dedicated binary** (or the same binary
  behind a `tare-profile` Cargo feature) that installs `dhat::Alloc` as the global
  allocator. Timing benches use `divan` with its own `AllocProfiler` or no custom
  allocator. The two concerns are never mixed in one compilation.
- The `sample/` crate will have:
  - `benches/alloc_bench.rs` — divan timing bench (no dhat).
  - `src/main.rs` or a separate bin — workload entrypoint that opts in to
    `dhat::Alloc` behind `#[cfg(feature = "tare-profile")]`.

### IntelliJ Platform Gradle Plugin

- **Version:** 2.18.1 (2026-07-10). Plugin ID: `org.jetbrains.intellij.platform`.
- **Target IDE build:** 2026.1.x (RustRover / IntelliJ IDEA).

### Inlay Hints API

- Use **declarative** API: `com.intellij.codeInsight.hints.declarative.InlayHintsProvider`.
- Extension point: `com.intellij.codeInsight.declarativeInlayProvider`.
- Available since 2023.1; recommended over older `InlayHintsProvider`.
- "Order of magnitude faster, much simpler and less error-prone" per JetBrains docs.
- Supports clickable items via `InlayActionHandler`.

### Content hashing strategy

Using `blake3` for file content hashes. Fast, no external dependencies beyond the
crate. The hash is hex-encoded in the JSON `content_hash` field. The plugin
recomputes it from the open `Document` text and compares — mismatch → grey/hide.
