# Phase 0 — Scaffold + Contract

Completed 2026-07-17.

## What was built
- Workspace with 6 crates: tare-schema, tare-collector (stub), tare-static (stub),
  tare-aggregate (stub), xtask (stub), sample
- tare-schema: full v1 JSON contract types with serde, BTreeMap-keyed
- 3 fixture files: runtime_only, static_only, merged — all parse + round-trip
- sample crate with 5 deliberate alloc patterns:
  Vec::with_capacity, Box::new, .collect(), .clone() on String, format!
- sample works both with and without tare-profile feature
- dhat integration verified: 153,927 bytes in 2,006 blocks captured

## Key observations
- dhat 0.3.3 tracks ALL allocations (no sampling) — this is fine for profiling
  runs but means alloc-heavy workloads get full capture cost
- divan 0.1.21 AllocProfiler is a GlobalAlloc — confirmed mutual exclusion with dhat
- Entry type uses flat struct with Option fields + skip_serializing_if for clean JSON
  (runtime fields null for static entries, vice versa)
- BTreeMap chosen over HashMap for deterministic JSON output order (sorted keys)

## Next
Phase 1: tare-static (syn-based allocation-site analyzer)
