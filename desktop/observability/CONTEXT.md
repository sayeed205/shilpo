# Context: shilpo-observability

## Overview

`shilpo-observability` is an internal Linux desktop crate providing subscriber initialization, opt-in Chrome trace generation, trace discovery, trace export, and local telemetry inventory diagnostics.

## Architecture

- **Subscriber Initialization**: Single global subscriber entry point for all durable process roles (`Shell`, `Settings`, `ExtensionHost`, `DeviceDaemon`, `ThemeDaemon`).
- **Reloadable Filter Control**: Thread-safe `LogFilterController` for dynamic `EnvFilter` directive replacement without subscriber re-initialization.
- **Opt-in Chrome Profiling**: Activated when `SHILPO_PROFILE` is `1` or `true`.
- **Trace Lifecycle**: Writes active traces as `<role>-<pid>-<timestamp>-<uuid>.json.part` and atomically renames to `.json` on orderly process shutdown.
- **Discovery & Export**: Discovers newest completed `.json` array trace and copies byte-for-byte to export destination.
- **Doctor Telemetry**: Read-only local profiling summary for `shilpo doctor --telemetry`.
