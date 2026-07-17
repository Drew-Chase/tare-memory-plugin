# CLAUDE.md — `tare`

Standing project memory. Read this every session before touching code. It encodes
the invariants that must not drift; it is **not** the build script (that was the
kickoff prompt). When something here conflicts with a request, surface the conflict
rather than silently breaking an invariant.

---

## What `tare` is

An inline Rust memory-allocation viewer for JetBrains IDEs (RustRover / IDEA+Rust /
CLion). It annotates source lines with allocation info from **two sources merged
into one JSON contract**, rendered by one plugin:

1. **Runtime** — a **dhat-backed** tracking global allocator captures sampled
   backtraces during a bench/workload run; real heap bytes are attributed to the
   source line that triggered each allocation, including allocations inside library
   calls. Accurate, but only for lines that actually executed.
2. **Static** — a `syn`-based pass flags known allocation **sites** (`Box::new`,
   `Vec`/`String`/`HashMap` ctors, `.clone()`, `.collect()`, `vec!`, `format!`,
   `Rc`/`Arc::new`, `.to_owned()`, `.to_string()`, …) and, where cheap, type sizes.
   No run needed. Approximate — **sites, never amounts.**

The runtime backend is **decided: dhat**, structured behind a small trait seam so a
hand-rolled `GlobalAlloc` can replace it later. Do not reopen this choice without a
concrete reason logged in `DECISIONS.md`.

---

## The honesty rules (the point of the whole project — enforce in code AND UI)

These are the reason the tool exists. Breaking one makes the tool lie, which is
worse than not having it.

- **Static entries are SITES, never amounts.** Heap amounts are runtime values
  (`Vec::with_capacity(n)`, growth doubling) and are *fundamentally* unknowable
  statically. Static entries may carry an `amount_hint` **string** (e.g.
  `"n * size_of::<Row>()"`) — never a number presented as fact.
- **Runtime entries only for lines that executed.** Never render `0 bytes` for an
  unexecuted line. Render nothing.
- **Never track line numbers live after edits.** On content-hash mismatch, grey or
  hide that file's hints with a "re-run to refresh" state. Do **not** shift stale
  numbers to follow edits.
- **Type size is an upper bound, not the truth.** Post-codegen stack frames coalesce,
  reuse, and spill. If/when type-size hints ship, label them as upper bounds.
- **The tool is a profiler surfaced inline, not a static analyzer.** Copy, docs, and
  UI states must read that way.

If a change would violate any of these, stop and flag it.

---

## The contract is the seam

One JSON file, `file -> line -> entries`, keyed by `source` (`runtime` | `static`)
and `kind`. It is the **only** coupling between the Rust side and the plugin. Build
and freeze `tare-schema` first; develop every other component against
`fixtures/` before real data exists. Changing the schema is a deliberate,
versioned act — bump `version`, update fixtures, update both sides.

Canonical output path: `target/tare/allocations.json`.

---

## Hard parts — already reasoned out; do not relearn the hard way

- **Reentrancy:** the recording path itself allocates. Guard record calls with a
  thread-local `Cell<bool>`, or the allocator recurses forever. (dhat handles its own
  internals, but any code we add around it obeys this too.)
- **Never symbolize at alloc time.** Capture unresolved backtraces (raw IPs); resolve
  to `file:line` offline at serialization. Build profiled targets with
  `-C force-frame-pointers=yes`.
- **Sample.** Size-weighted Poisson (dhat-style); scale sampled bytes back up.
  Per-allocation capture is brutal on alloc-heavy code.
- **Attribution = deepest frame under `workspace_root`.** Blame `let rows = it.collect()`
  on that line, not on `RawVec::grow`. Keep the full stack for the tooltip.
- **divan × global allocator:** the allocator slot is claimed once. dhat-as-global
  and divan's own alloc profiler conflict. Resolution lives in `DECISIONS.md` —
  profiling runs behind a `tare-profile` feature / dedicated binary, separate from
  timing benches. Re-verify it still holds when touching either.
- **Staleness:** stamp each file's `content_hash`; plugin compares against the open
  document (`Document.getLineStartOffset` for line→offset; no PSI needed for v1).

---

## Phase discipline

Work in phases; each is isolated behind the JSON contract. **Compile, test, commit,
and pause for review at every phase boundary.** Do not run ahead.

0. Scaffold + `tare-schema` + fixtures + `sample/` crate with known patterns.
1. `tare-static` (syn) — sites only, tested against `sample/`.
2. `tare-collector` (dhat, behind trait seam + `tare-profile` feature).
3. `tare-aggregate` + `xtask` (`static` / `profile [--bench|--bin]` / `all`).
4. Plugin — `DeclarativeInlayHintsProvider` + gutter `LineMarkerProvider`,
   hide-on-dirty, settings (on/off, min-bytes, metric).
5. One-button Run Configuration + README with the honesty caveats stated plainly.

Verify current APIs/versions before each component (IntelliJ Platform Gradle Plugin
2.x, inlay-hints API, dhat, divan, syn 2.x). **Log findings in `DECISIONS.md`** —
training data is stale on versions.

---

## Non-goals (do not build)

- Live as-you-type heap byte amounts (impossible).
- Live line-number tracking after edits (hide-on-dirty only).
- Whole-program/MIR static analysis of allocations behind opaque calls (the runtime
  path covers those).
- Exact post-codegen stack frame size.

---

## Conventions

- Running notes: `.claude/agent-memory/tare/`.
- Performance: profile before optimizing the collector; the sampling and
  unresolved-capture choices above are the ones that matter — don't micro-opt around
  them.
- Prefer short, real-word naming with double meanings (house style).
- Test against `fixtures/` before real data, always.
