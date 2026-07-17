# Phase 2 — Runtime Collector (tare-collector)

Completed 2026-07-17.

## Architecture
- v1 approach: **post-process dhat's JSON** rather than custom GlobalAlloc
- dhat captures all allocations with backtraces, writes `dhat-heap.json`
- We parse that JSON, resolve frames, attribute to workspace source lines
- Behind `CollectorBackend` trait seam for future v2 custom allocator

## dhat JSON format (key fields)
- `ftbl`: string array of frames, format `"0xADDR: func_name (file:line:col)"`
- `pps`: array of program points with `tb` (total bytes), `tbk` (blocks),
  `gb` (peak bytes), `fs` (frame indices into ftbl)

## Key challenges solved
- **Frame parsing**: function names can contain `(` and `)` in generics
  (e.g., `String (*)(ref$<str$>)>`). Fixed by searching backwards for
  the last ` (` that leads to a valid `file:line:col)` pattern.
- **Attribution**: deepest (first) frame under workspace root gets blamed.
  Filters out alloc/core/std/iter/slice stdlib paths.
- **Path normalization**: dhat uses backslashes on Windows; normalized to
  forward slashes. Handles both absolute and relative frame paths.
  Strips crate-name prefix when workspace root's last component matches.

## Test coverage
- 13 tests: frame parsing (5), attribution logic (4), end-to-end (2),
  regression for complex generics (2)
- End-to-end verified: lines 12, 25, 45, 50 of sample/src/lib.rs get
  attributed correctly from real dhat output

## Next
Phase 3: tare-aggregate + xtask orchestration
