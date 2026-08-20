# ADR-0011: Opt-in Process Profiling Infrastructure

- **Status**: Accepted
- **Date**: 2026-08-12
- **Author**: Shilpo Core Team
- **Issue**: [#86](https://github.com/shilpo-rs/shilpo/issues/86)

## Context

Performance profiling across Shilpo's multi-process architecture (shell daemon, settings app, extension host, device daemon, theme daemon) requires high-resolution tracing without introducing continuous memory overhead or write I/O in production. Cross-platform crates (`core/theme`, `core/ext-api`, `shilpo-ui`) must remain independent of Linux-specific profiler layers.

## Decision

1. **Internal Observability Crate**: Create `shilpo-observability` (`desktop/observability`) as a dependency-light, non-published internal crate. Cross-platform crates emit standard `tracing` spans and never depend directly on `shilpo-observability`.
2. **Opt-in Activation**: Profiling is enabled strictly when `SHILPO_PROFILE` is set to `"1"` or `"true"` (case-insensitive). When disabled, zero profiler layers, threads, or files are created.
3. **Trace Path Resolution**: Active traces write to collision-resistant `<role>-<pid>-<timestamp>-<uuid>.json.part` files inside `$XDG_STATE_HOME/shilpo/profiles` (or `~/.local/state/shilpo/profiles`), overridable via absolute `SHILPO_PROFILE_DIR`.
4. **Orderly Finalization**: On process shutdown, `ObservabilityGuard` flushes tracing data and atomically renames `.json.part` to `.json`.
5. **Discovery & Export**: `shilpo profile export` validates source traces as complete JSON arrays and copies byte-for-byte to a target output path without overwriting existing files.
6. **Doctor Telemetry**: `shilpo doctor --telemetry` provides a read-only inventory summary of active and completed profile traces.

## Consequences

- No profiler layer, writer thread, profile directory, or trace-file I/O when `SHILPO_PROFILE` is disabled; normal `tracing` call-site filtering remains in place.
- Deterministic, collision-free Chrome Trace generation for performance analysis.
- Clean separation between cross-platform `tracing` emission and Linux process profiling execution.
