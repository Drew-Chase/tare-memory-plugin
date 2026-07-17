# Phase 4 — JetBrains Plugin

Completed 2026-07-17.

## Architecture
- 6 Kotlin files under `plugin/src/main/kotlin/dev/tare/plugin/`
- Gradle build with IntelliJ Platform Gradle Plugin 2.18.1
- Targets IntelliJ IDEA Community 2024.3+ (sinceBuild=243)
- Dependencies: gson (JSON parsing), blake3 (content hashing)

## Components
1. **TareModel.kt** — Kotlin data classes mirroring tare-schema JSON
2. **TareDataService.kt** — project service, loads/caches allocations.json,
   VFS file watcher, blake3 content hash comparison for staleness
3. **TareInlayHintsProvider.kt** — OwnBypassCollector placing hints at
   line end offsets. Runtime = "23.4 KiB, 1000 allocs", static =
   "site: Vec::with_capacity". Stale = "⟳ re-run to refresh"
4. **TareLineMarkerProvider.kt** — gutter icons with HTML tooltips showing
   byte breakdown + top 3 call stacks (5 frames each)
5. **TareSettings.kt** — PersistentStateComponent: enabled, minBytes, metric
6. **TareSettingsConfigurable.kt** — Swing settings panel under Tools

## Key decisions
- OwnBypassCollector, not SharedBypassCollector (file-level, not PSI)
- language="" in plugin.xml (language-agnostic, filter in createCollector)
- blake3 Java lib: io.github.rctcwyvrn:blake3:1.3
- Can't test compilation without Gradle — needs IDE-based verification

## Next
Phase 5: Run Configuration + README
