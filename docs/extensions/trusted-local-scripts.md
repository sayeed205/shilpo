# Trusted Local Scripts Reference

**Trusted Local Scripts** are lightweight, user-owned scripts that run directly on the host system to display read-only status widgets on the Shilpo top bar.

---

## 1. Security & Architectural Boundary

```
┌─────────────────────────────────────────────────────────────┐
│                    Trusted Local Scripts                    │
├─────────────────────────────────────────────────────────────┤
│ • Local-only: Lives exclusively in local config directories │
│ • Unsandboxed: Executes directly under the user's login     │
│ • No WASM Capability Model: Uses standard OS permissions    │
│ • Strictly Bar Widgets Only: Cannot declare interactive UI, │
│   menus, side panels, desktop widgets, or settings pages    │
│ • Non-Interactive: Emits read-only JSON records on stdout   │
└─────────────────────────────────────────────────────────────┘
```

> **Important**: Because Trusted Local Scripts run with the full permissions of your user account without WebAssembly sandboxing, only install and run scripts you have authored or reviewed yourself.

---

## 2. Script Manifest Format (`extension.toml`)

Trusted Local Scripts use a simplified `extension.toml` manifest specifying `[runtime]` configuration:

```toml
schema_version = 1
id = "local.script.cpu-temp"
name = "CPU Temperature Script"
version = "0.1.0"

[runtime]
mode = "poll"                 # "poll" (interval-based) or "stream" (continuous lines)
executable = "cpu-temp.sh"    # Relative path to executable script
args = []                     # Optional CLI arguments
interval_ms = 5000            # Polling interval in milliseconds
timeout_ms = 1000             # Max allowed execution time per run

[[contributions.bar_widgets]]
id = "cpu-temp"
name = "CPU Temperature"
description = "Polls and displays current CPU temperature"
```

---

## 3. Emitting Output Records

On each execution or stream line, the script writes a newline-delimited JSON record (`ScriptRecord`) to standard output:

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

### Record Fields
- `schema_version`: Must be `1`.
- `contribution`: Must match the declared `bar_widget` `id`.
- `kind`: `"text"` or `"view"`.
- `text`: Short string displayed in the bar pill.
- `tooltip`: Optional hover tooltip text.
- `icon`: Optional Material icon name displayed alongside the text.

---

## 4. Reference Implementation

See [`extensions/cpu-temp-script`](https://github.com/shilpo-rs/shilpo-extensions/tree/main/cpu-temp-script) for a complete working example.
