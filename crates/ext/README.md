# Shilpo Extensions

Status: Phase 1 (contract and catalog) is implemented. `shilpo-ext` provides validated manifest and ID types,
contribution and capability declarations, typed events/effects, a bounded declarative view tree, and an in-memory
host/runtime adapter for development and interface tests.

The WASM runtime, guest SDK, package installer, and `shilpo ext` commands described below belong to later phases and are
not available yet.

See [the extension architecture](../../docs/architecture/extensions.md) for the runtime, security, lifecycle, and
implementation plan.

## What an extension can add

An extension may contribute one or more:

- bar widgets;
- desktop widgets;
- side-panel pages;
- control-center entries;
- settings pages;
- launcher providers;
- shell actions;
- background tasks that react to typed shell events.

Extensions can react to events such as palette generation, theme changes, wallpaper changes, output changes, network
state, or timers. Operations that affect the system—such as changing wallpaper, running a command, reading files, or
using the network—require an explicit capability declaration and user grant.

An extension cannot import shell internals or create arbitrary GPUI elements. It returns a small declarative view tree
that Shilpo renders with `shilpo-ui`, preserving theme, accessibility, layout, and performance rules.

## Extension layout

A source repository containing Rust logic will normally look like:

```text
world-clock/
├── extension.toml
├── Cargo.toml
├── src/
│   └── lib.rs
├── settings.schema.json
├── assets/
├── i18n/
├── README.md
└── LICENSE
```

A built package contains:

```text
world-clock/
├── extension.toml
├── extension.wasm
├── settings.schema.json
├── assets/
├── i18n/
├── README.md
└── LICENSE
```

Extensions that only provide static data may omit `Cargo.toml`, `src/`, and
`extension.wasm`.

## Planned authoring workflow

### Prerequisites

- Rust installed through `rustup`;
- the `wasm32-wasip2` target;
- a Shilpo build that includes the extension host and CLI.

```bash
rustup target add wasm32-wasip2
shilpo ext new io.github.alice.world-clock
cd world-clock
```

The scaffold command will create a manifest, Rust guest crate, settings schema, README, license, and a minimal test.

### Manifest

`extension.toml` is the static source of truth. Shilpo reads it without executing the extension.

```toml
id = "io.github.alice.world-clock"
name = "World Clock"
version = "1.0.0"
schema_version = 1
api_version = "0.1.0"
min_shilpo_version = "0.1.0"
authors = ["Alice <alice@example.com>"]
description = "Shows local time for selected cities."
repository = "https://github.com/alice/shilpo-world-clock"
license = "MIT"

[library]
path = "extension.wasm"

[[contributions.bar_widgets]]
id = "bar"
name = "World Clock"
description = "A compact rotating city clock."

[[contributions.desktop_widgets]]
id = "desktop"
name = "World Clock"
description = "A resizable desktop clock."
default_width = 320
default_height = 180
min_width = 180
min_height = 100

[[contributions.settings_pages]]
id = "settings"
name = "World Clock"
schema = "settings.schema.json"

[[subscriptions]]
event = "palette_generated"

[[subscriptions]]
event = "timer_fired"

[[capabilities]]
kind = "events:subscribe"
events = ["palette_generated", "timer_fired"]

[[capabilities]]
kind = "notifications:show"
```

Rules:

- `id` uses reverse-domain form and remains stable after publishing.
- Contribution IDs are unique within the extension and remain stable once users can place or configure them.
- `schema_version` versions the manifest.
- `api_version` versions the WASM host/guest interface.
- `version` is the extension release.
- Contributions describe integration points; capabilities grant effects. Declaring a desktop widget does not grant
  filesystem, process, network, or wallpaper access.
- Paths are relative to the extension root and cannot escape it.

### Settings schema

Settings use JSON Schema so Shilpo can provide a usable, validated settings page before custom settings views are
supported.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "cities": {
      "type": "array",
      "title": "Cities",
      "items": { "type": "string" },
      "default": ["Asia/Kolkata", "Europe/London"]
    },
    "twenty_four_hour": {
      "type": "boolean",
      "title": "Use 24-hour time",
      "default": true
    }
  }
}
```

The host owns persistence and validation. The extension receives only the last valid settings snapshot. A later custom
settings contribution will use the same schema and save/cancel contract.

### Guest logic

The planned Rust SDK will compile to a WASM component:

```toml
[package]
name = "world-clock"
version = "1.0.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
shilpo-ext = "0.1"
```

The intended author interface is event-driven:

```rust,ignore
use shilpo_ext::{
    Event, Extension, ExtensionContext, ExtensionResult, View, column, export_extension, text,
};

struct WorldClock;

impl Extension for WorldClock {
    fn new() -> Self {
        Self
    }

    fn on_event(
        &mut self,
        event: Event,
        context: &mut ExtensionContext,
    ) -> ExtensionResult {
        if event.is_timer("rotate-city") {
            context.invalidate("bar");
            context.invalidate("desktop");
        }
        Ok(())
    }

    fn view(&self, contribution: &str, context: &ExtensionContext) -> View {
        column()
            .child(text(context.setting_string("active_city")))
            .child(text(context.local_time()))
            .build()
    }
}

export_extension!(WorldClock);
```

This example describes the shape of the planned SDK, not its current Rust interface. Guest code will receive typed
events and return views/effects. It will not receive GPUI contexts, shell runtime handles, or concrete service objects.

### Capabilities

Only declare capabilities the extension needs.

```toml
[[capabilities]]
kind = "events:subscribe"
events = ["palette_generated", "wallpaper_changed"]

[[capabilities]]
kind = "wallpaper:set"
sources = ["extension_asset", "local_file"]

[[capabilities]]
kind = "network:http"
hosts = ["api.example.com"]
paths = ["/v1/**"]

[[capabilities]]
kind = "process:exec"
command = "playerctl"
args = ["status"]
```

There is no ambient filesystem, environment, network, or process access. Shilpo checks the manifest declaration and the
user's stored grant for every privileged effect.

## Phase 1 development checks

Validate the manifest contract through the crate tests:

```bash
cargo test -p shilpo-ext
```

Regenerate the checked-in manifest schema after changing contract types:

```bash
cargo run -p shilpo-ext --example generate_schema -- \
  crates/ext/schema/extension-v1.schema.json
```

The generated schema is the machine-readable authoring contract. The tests compare it with the checked-in fixture to
prevent an accidental schema change.

### Planned validation and build

```bash
shilpo ext check .
cargo test
cargo build --target wasm32-wasip2 --release
shilpo ext pack .
```

`check` will validate:

- manifest syntax and IDs;
- schema and interface compatibility;
- referenced files and assets;
- settings schema and defaults;
- capabilities and scopes;
- package size and view assets;
- the WASM component's imported and exported interface.

`pack` will produce a versioned `.shilpo-ext` archive containing only runtime files.

## Planned development mode

Run an extension directly from its source directory:

```bash
shilpo ext dev /absolute/path/to/world-clock
```

Development mode will:

- validate and build the WASM component;
- register the absolute path without copying it;
- override an installed extension with the same ID;
- watch the manifest, WASM component, settings schema, translations, and assets;
- hot reload the guest while preserving compatible host-owned settings/state;
- show diagnostics without taking down the shell.

Useful commands:

```bash
shilpo ext list
shilpo ext reload io.github.alice.world-clock
shilpo ext logs io.github.alice.world-clock --follow
shilpo ext stop-dev io.github.alice.world-clock
```

Stopping development mode reveals the installed version again, if one exists.

For work on Shilpo and an extension together, the planned environment override is:

```bash
SHILPO_EXTENSION_DEV_PATHS=/absolute/path/to/world-clock cargo run -p shilpo-shell
```

Multiple paths use the platform path separator. Duplicate IDs are an error unless one path is explicitly selected as the
active override.

## Placing contributions

The proposed configuration uses namespaced contribution references.

### Bar widget

```toml
[bar.widgets]
start = ["builtin:launcher", "builtin:workspaces"]
center = ["ext:io.github.alice.world-clock/bar"]
end = ["builtin:network", "builtin:battery"]
```

An extension contribution can appear more than once by creating named instances:

```toml
[[extensions.instances]]
id = "london-clock"
contribution = "ext:io.github.alice.world-clock/bar"

[extensions.instances.settings]
cities = ["Europe/London"]

[[extensions.instances]]
id = "tokyo-clock"
contribution = "ext:io.github.alice.world-clock/bar"

[extensions.instances.settings]
cities = ["Asia/Tokyo"]
```

The bar owns orientation, available height, section placement, spacing, and error presentation. The extension renders
for the supplied surface context.

### Desktop widget

```toml
[[desktop.widgets]]
instance = "home-clock"
contribution = "ext:io.github.alice.world-clock/desktop"
output = "primary"
x = 32
y = 32
width = 320
height = 180
```

Shilpo owns drag, resize, output migration, stacking, and edit mode. Geometry is stored per instance, independently from
extension settings.

### Side panel and control center

Side-panel contributions are registered by canonical ID and may be opened by an extension action, a control-center
entry, a bar widget, or user keybinding:

```toml
[[keybindings]]
shortcut = "super+shift+w"
action = "ext:io.github.alice.world-clock/open-panel"
```

The shell owns placement, layer-shell configuration, focus restoration, dismissal, multi-output behavior, and size
constraints.

### Settings

Every installed extension appears under the planned Extensions category in the settings app. The page includes:

- enable/disable and version status;
- granted and requested capabilities;
- schema-generated extension settings;
- contribution instances and placement;
- diagnostics, logs, reload, update, and uninstall actions.

Settings remain available when an extension is disabled or incompatible so users can recover it.

## End-user installation

### Settings app

The planned Extensions page will support:

1. Install from the Shilpo registry.
2. Review source, version, requested capabilities, and package signature.
3. Grant or deny optional capabilities.
4. Enable the extension.
5. Add its contributions to supported shell surfaces.

An extension update requesting broader capabilities stays downloaded but inactive until the user reviews the new grants.

### Local package

```bash
shilpo ext install ./world-clock-1.0.0.shilpo-ext
shilpo ext enable io.github.alice.world-clock
```

### URL

```bash
shilpo ext install https://example.com/world-clock-1.0.0.shilpo-ext
```

URL installs require a package hash or trusted signed registry entry. Installing arbitrary Git repository source is a
development workflow, not the default end-user workflow.

### Management

```bash
shilpo ext list
shilpo ext info io.github.alice.world-clock
shilpo ext update io.github.alice.world-clock
shilpo ext disable io.github.alice.world-clock
shilpo ext enable io.github.alice.world-clock
shilpo ext uninstall io.github.alice.world-clock
```

Disable keeps package files, settings, grants, and extension-owned data. Uninstall removes the package and contribution
instances but asks separately whether persistent extension-owned data should also be removed.

## Installation paths

On Linux, the proposed paths are:

```text
$XDG_CONFIG_HOME/shilpo/extensions.toml
$XDG_CONFIG_HOME/shilpo/extensions/<id>.toml
$XDG_DATA_HOME/shilpo/extensions/installed/<id>/<version>/
$XDG_DATA_HOME/shilpo/extensions/data/<id>/
$XDG_CACHE_HOME/shilpo/extensions/compiled/
$XDG_STATE_HOME/shilpo/extensions/logs/
```

If an XDG variable is unset, Shilpo uses its standard home-directory fallback.

Installed package files are immutable. Extension-owned data survives updates. Compiled artifacts are disposable. Logs
and crash diagnostics live outside configuration.

## Failure behavior

An invalid or failing extension must not make the shell unusable.

- Invalid manifests are listed with diagnostics but never executed.
- Incompatible interface versions are disabled.
- A render failure keeps the last valid view.
- A contribution failure displays a host-owned error placeholder.
- Repeated traps, timeouts, invalid effects, or resource violations disable the extension for the session.
- Updates are atomic and preserve the previous working package for rollback.
- Removing a development override restores the installed package.

## First implementation milestones

1. Define the manifest, contribution, capability, settings, event/effect, and view-tree types in `shilpo-ext`.
2. Refactor Shilpo's closed bar-widget and action enums into namespaced registries.
3. Add an in-memory runtime adapter and host interface tests.
4. Add the WASM runtime and development commands.
5. Integrate bar and desktop contributions.
6. Add side-panel, settings, control-center, launcher, and background-task contributions.
7. Add package installation, rollback, permission review, and the settings extension manager.
8. Define signing and registry policy before enabling a public gallery or automatic updates.
