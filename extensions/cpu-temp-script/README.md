# CPU Temperature — Trusted Local Script Reference

> **Classification**: **Trusted Local Script** (Unsandboxed, Local-Only).

This directory contains a reference implementation of a Shilpo **Trusted Local Script**.

## What is a Trusted Local Script?

Unlike WebAssembly extensions, Trusted Local Scripts:

- Run directly on the host system using the local user's execution privileges without WebAssembly sandboxing.
- Operate outside the WASM capability and permission model.
- Are strictly limited to read-only polling or streaming **bar widgets** (`contributions.bar_widgets`).
- Cannot declare or provide interactive UI (ViewTree events, menus, dialogs), side panels, desktop widgets, settings pages, or privileged capabilities.
- Are validated through the `ScriptRuntime` manifest and record decode path rather than WASM `shilpo ext check`.

## Manifest Structure

A Trusted Local Script uses an `extension.toml` manifest with `[runtime]` configuration:

```toml
schema_version = 1
id = "local.script.cpu-temp"
name = "CPU Temperature Script"
version = "0.1.0"

[runtime]
mode = "poll"
executable = "cpu-temp.sh"
args = []
interval_ms = 5000
timeout_ms = 1000

[[contributions.bar_widgets]]
id = "cpu-temp"
name = "CPU Temperature"
description = "Polls and displays current CPU temperature"
```

## Record Output Protocol

On each poll interval, the executable emits a newline-delimited JSON record (`ScriptRecord`) on `stdout`:

```json
{
  "schema_version": 1,
  "contribution": "cpu-temp",
  "kind": "text",
  "text": "48°C",
  "tooltip": "CPU Package Temperature: 48°C",
  "icon": "device_thermostat"
}
```

See [Trusted Local Scripts Guide](../../docs/extensions/trusted-local-scripts.md) for complete details.
