# Extension Manifest Reference (`extension.toml`)

The extension manifest `extension.toml` is the authoritative configuration file defining an extension's identity, entrypoint, contribution families, system event subscriptions, capability scopes, and settings schema.

```toml
schema_version = 1
api_version = "0.1.0"
min_shilpo_version = "0.1.0"
```

---

## 1. Top-Level Metadata

| Field | Type | Description |
| :--- | :--- | :--- |
| `id` | String | Unique canonical extension identifier in reverse-DNS format (e.g. `org.shilpo.example`, `dev.author.tool`). |
| `name` | String | Human-readable extension display name. |
| `version` | String | Semantic version string (e.g. `0.1.0`). |
| `schema_version` | Integer | Manifest schema version (currently `1`). |
| `api_version` | String | Target Shilpo Extension API version (currently `0.1.0`). |
| `min_shilpo_version` | String | Minimum supported Shilpo version. |
| `authors` | Array of Strings | Extension authors and email contacts. |
| `description` | String (Optional) | Short description of extension features. |
| `repository` | String (Optional) | Source code repository URL. |
| `license` | String (Optional) | SPDX license expression (e.g. `MIT`, `Apache-2.0`). |
| `[library]` | Table | Contains `path = "extension.wasm"` pointing to the WebAssembly component. |

---

## 2. Contribution Families

Shilpo supports **ten** distinct contribution families:

### Bar Widgets
Compact pills displayed in the shell's top status bar.
```toml
[[contributions.bar_widgets]]
id = "status-bar"
name = "Showcase Status Bar"
description = "Compact bar widget showing extension status"
```

### Bar Menus
Dropdown popup menus attached to a declared `bar_widget`.
```toml
[[contributions.bar_menus]]
id = "status-menu"
name = "Showcase Status Menu"
bar_widget = "status-bar"
```

### Desktop Widgets
Resizable canvas cards placed directly on the desktop.
```toml
[[contributions.desktop_widgets]]
id = "system-card"
name = "Showcase Desktop Card"
description = "Resizable desktop showcase widget"
default_width = 320
default_height = 240
min_width = 200
min_height = 150
```

### Settings Pages
Declarative settings panels integrated into the system Settings app.
```toml
[[contributions.settings_pages]]
id = "preferences"
name = "Showcase Preferences"
schema = "settings.schema.json"
```

### Side Panels
Dockable, full-height lateral panels for rich tooling and logs.
```toml
[[contributions.side_panels]]
id = "side-panel"
name = "Showcase Side Panel"
description = "Detailed diagnostics and inspector"
```

### Search Providers
Search backends queryable from the system launcher / search interface.
```toml
[[contributions.search_providers]]
id = "search-commands"
name = "Showcase Search"
description = "Searches showcase commands and presets"
```

### Actions
Executable commands registered into the command palette and shortcut system.
```toml
[[contributions.actions]]
id = "toggle-power"
name = "Toggle Showcase Mode"
description = "Toggles mode between active and idle"
```

### Keyboard Shortcuts
Global hotkey bindings bound to a declared `action`.
```toml
[[contributions.keyboard_shortcuts]]
id = "shortcut-toggle"
name = "Toggle Mode Shortcut"
action = "toggle-power"
default_binding = "Super+Shift+S"
```

### Background Tasks
Periodic or event-triggered background maintenance workers.
```toml
[[contributions.background_tasks]]
id = "sync-task"
name = "Showcase Sync Task"
```

### Wallpaper Providers
Dynamic wallpaper generators for global desktop or per-workspace surfaces.
```toml
[[contributions.wallpaper_providers]]
id = "solid-wallpapers"
name = "Showcase Wallpapers"
description = "Dynamic solid color wallpapers"
modes = ["manual", "slideshow"]
targets = ["global", "workspace"]
```

---

## 3. Subscriptions

Declare system events the extension is authorized to receive:

```toml
[[subscriptions]]
event = "palette_generated"

[[subscriptions]]
event = "wallpaper_changed"
```

Valid event categories: `outputs_changed`, `theme_changed`, `palette_generated`, `wallpaper_changed`, `network_changed`, `media_changed`, `power_changed`, `timer_fired`, `workspace_changed`.

---

## 4. Capabilities & Permissions

Declare explicit capability scopes required by extension operations:

```toml
[[capabilities]]
kind = "events:subscribe"
events = ["palette_generated", "wallpaper_changed"]

[[capabilities]]
kind = "theme:read"

[[capabilities]]
kind = "notifications:show"

[[capabilities]]
kind = "clipboard:read"

[[capabilities]]
kind = "clipboard:write"

[[capabilities]]
kind = "wallpaper:read"

[[capabilities]]
kind = "actions:invoke"
actions = ["toggle-power"]
```

---

## 5. Settings Schema (`settings.schema.json`)

Extensions providing `settings_pages` declare a JSON Schema Draft 2020-12 file defining types, defaults, and descriptions:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Showcase Settings",
  "type": "object",
  "properties": {
    "showNotifications": {
      "type": "boolean",
      "default": true,
      "description": "Show desktop notifications on state changes"
    },
    "refreshIntervalSeconds": {
      "type": "integer",
      "minimum": 5,
      "maximum": 3600,
      "default": 30,
      "description": "Sync interval in seconds"
    }
  },
  "additionalProperties": false
}
```

---

## 6. Linting & Validation (`shilpo ext lint`)

You can validate manifests, capabilities, schemas, assets, and project configurations ahead of time without running the extension:

```bash
# Human-readable diagnostic output
shilpo ext lint .

# Strict CI mode: fail on warnings as well as errors
shilpo ext lint . --deny-warnings

# Machine-readable JSON output
shilpo ext lint . --json

# Quiet mode: only output failures
shilpo ext lint . --quiet
```

