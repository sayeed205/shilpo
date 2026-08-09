# Shilpo Extensions

`shilpo-ext` provides the validated extension contract, policy-owning host, in-memory and Wasmtime Component Model
runtime adapters, resource budgets, structured diagnostics, deterministic `.shilpo-ext` archives, Ed25519 signing,
atomic installation and rollback, host-owned receipts and grants, signed registry resolution, update selection, and the
catalog snapshots consumed by the shell, CLI, and Settings app.

See [the extension architecture](../../docs/architecture/extensions.md) for the runtime, security, lifecycle, and
distribution decisions.

## What an extension can add

An extension may contribute one or more:

- bar widgets;
- desktop widgets;
- side-panel pages;
- settings pages;
- launcher providers;
- shell actions;
- background tasks that react to typed shell events.

Extensions can react to events such as palette generation, theme changes, wallpaper changes, output changes, network
state, or timers. Operations that affect the system—such as changing wallpaper, running a command, reading files,
reading system location (`location:read`), or using the network—require an explicit capability declaration and user
grant.

An extension cannot import shell internals or create arbitrary GPUI elements. It returns a small declarative view tree
that Shilpo renders with `shilpo-ui`, preserving theme, accessibility, layout, and performance rules.

The view protocol also exposes semantic host components for behaviors that need native rendering or animation. For
example, an extension can return a `loading_indicator` node and the shell renders the Material 3 Expressive loader.
Semantic color tokens such as `on_surface_variant` are resolved by the host against the active theme, so extensions do
not hardcode light- or dark-mode colors.

Official extension sources live in [`extensions`](../../extensions). They use a dedicated WASI guest workspace so the
native Shilpo workspace does not compile every guest during ordinary development.

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

## Authoring workflow

### Prerequisites

- Rust installed through `rustup`;
- the `wasm32-wasip2` target;
- a Shilpo build that includes the extension host and CLI;
- `wit-bindgen` for the current low-level Rust guest interface.

```bash
rustup target add wasm32-wasip2
```

Use [`examples/world-clock`](../../examples/world-clock) as the working scaffold. The CLI does not currently provide an
extension scaffolding command.

### Manifest

`extension.toml` is the static source of truth. Shilpo reads it without executing the extension.

```toml
id = "io.github.alice.world-clock"
name = "World Clock"
version = "1.0.0"
schema_version = 1
api_version = "0.2.0"
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

The host owns persistence and validation. The extension receives only the last valid settings snapshot. Custom settings
views must use the same schema and save/cancel contract.

### Guest logic

The current low-level Rust guest interface uses `wit-bindgen` and compiles to a WASI Preview 2 component:

```toml
[package]
name = "world-clock"
version = "1.0.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde_json = "1"
wit-bindgen = "0.57"
```

The versioned contract lives in [`wit/extension.wit`](wit/extension.wit):

```wit
package shilpo:extension@0.1.0;

world extension {
    export on-event: func(event-json: string) -> string;
    export view: func(contribution-id: string) -> string;
}
```

The JSON strings carry the crate's typed `ExtensionEvent`, `Vec<HostEffect>`, and `Option<ViewTree>` wire formats. This
keeps the Component Model ABI versioned while allowing a higher-level guest SDK to wrap it later. Guest code never
receives GPUI contexts, shell runtime handles, or concrete service objects.

The runtime supplies a closed WASI context only for the standard Rust component adapter. It inherits no files,
environment variables, arguments, terminal streams, or network access. Privileged work still goes through host effects
and capability checks.

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

## Contract development checks

Validate the manifest contract through the crate tests:

```bash
cargo test -p shilpo-ext
```

Regenerate the checked-in manifest schema after changing contract types:

```bash
cargo run -p shilpo-ext --example generate_schema -- \
  crates/ext/schema/extension-v1.schema.json
cargo run -p shilpo-ext --example generate_distribution_schemas -- \
  crates/ext/schema
```

The generated distribution schemas define the package-signature sidecar and signed registry-index envelope consumed by
the extension catalog.

The generated schema is the machine-readable authoring contract. The tests compare it with the checked-in fixture to
prevent an accidental schema change.

### Validate, build, and package

```bash
cargo test
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/world_clock.wasm extension.wasm
shilpo ext check .
shilpo ext pack .
```

Build and copy the component to the manifest's `library.path` before running `check`. `check` validates:

- manifest syntax and IDs;
- schema and interface compatibility;
- referenced files and assets;
- settings schema and defaults;
- capabilities and scopes;
- package size and view assets;
- the WASM component's imported and exported interface.

`pack` produces a versioned `.shilpo-ext` archive containing only runtime files.

### Signing and publishing

Publishing keeps the extension and contribution IDs stable, increments `version`, and produces an immutable package:

```bash
shilpo ext check .
shilpo ext pack .
shilpo ext keygen alice.shilpo-key
shilpo ext sign world-clock-1.0.0.shilpo-ext \
  --key alice.shilpo-key \
  --publisher Alice
```

Keep the generated private key offline and publish the `.pub` file. `sign` writes
`<package>.sig.json`; the signature binds the publisher identity to the package SHA-256 digest. Uploading the immutable
archive and its release metadata is registry-specific. The registry independently signs its complete index. A release
entry records the extension and API versions, minimum Shilpo version, channel, package URL and hash, publisher key,
capability digest, publication time, and whether the release has been yanked.

Authors may also publish the package through a website or release service. A local archive can be installed manually. A
direct URL is a one-off installation source; publish releases through a signed registry to provide update discovery.

## Development mode

Run an extension directly from its source directory:

```bash
shilpo ext dev /absolute/path/to/world-clock
```

Development mode will:

- validate the already-built WASM component and its exact WIT exports;
- instantiate it under the production sandbox, deadline, fuel, memory, and transfer budgets;
- smoke-test startup plus bar and desktop view output without applying returned effects;
- register the absolute path without copying it;
- persist the registration under the XDG state directory;
- record validation and reload activity in a per-extension log.

Useful commands:

```bash
shilpo ext list
shilpo ext reload io.github.alice.world-clock
shilpo ext logs io.github.alice.world-clock --follow
```

Rebuild the component, then run `reload` to validate it and advance the persisted generation. A running shell also
detects changes to the registered manifest, component, settings schema, and assets. It constructs and validates a
replacement generation before swapping, preserves host-owned extension state and contribution instances, and keeps the
last valid runtime and view visible when a development edit is broken.

## Placing contributions

Configuration uses namespaced contribution references.

### Bar widget

```toml
[bar.widgets]
start = ["builtin:workspaces"]
center = ["ext:io.github.alice.world-clock/bar"]
end = ["builtin:network", "builtin:battery"]
```

Extension-wide settings are keyed by extension ID and delivered to each contribution:

```toml
[extensions.settings."io.github.alice.world-clock"]
cities = ["Europe/London", "Asia/Tokyo"]
```

The bar owns orientation, available height, section placement, spacing, and error presentation. The extension renders
for the supplied surface context. Desktop widgets may additionally override extension-wide values through their instance
`settings`.

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

### Side panel

Side-panel contributions are registered by canonical ID and may be opened by an extension action, a bar widget, or
user keybinding:

```toml
[[keybindings]]
shortcut = "super+shift+w"
action = "ext:io.github.alice.world-clock/open-panel"
```

The shell owns placement, layer-shell configuration, focus restoration, dismissal, multi-output behavior, and size
constraints.

### Settings

The Extensions category in the Settings app exposes:

- Discover, Installed, Updates, and Sources views;
- enable, disable, update, and uninstall actions;
- version, trust, source, and capability summaries;
- permission review for updates requesting broader capabilities;
- registry refresh and third-party source removal.

Development extensions that contribute a settings page also expose their JSON Schema fields through the Settings page
registry.

## Publisher identity and trust

Built-in functionality uses `builtin:*`. Every extension contribution—including an official Shilpo extension—uses
`ext:*`.

An extension cannot declare itself official. Shilpo derives trust from the verified package signature and installation
source:

- **Official**: signed by a Shilpo-controlled publisher key;
- **Verified publisher**: signed by an identity verified by a trusted registry;
- **Signed third-party**: valid signature without registry publisher verification;
- **Unverified**: local or remote package without a trusted publisher identity.

An `org.shilpo.*` ID is a naming convention, not proof of official status. Official extensions receive no hidden
permissions and use the same sandbox, capability checks, resource limits, and permission review as third-party
extensions.

Shilpo stores the package source, publisher-key fingerprint, package hash, trust state, selected channel, and installed
version in a host-owned installation receipt. Updates must preserve both the extension ID and publisher identity. A
different key requires a signed key-rotation delegation; otherwise Shilpo reports a publisher conflict.

## End-user installation

### Discover in Settings

The Settings app is the primary graphical discovery and management interface:

```text
Settings
└── Extensions
    ├── Discover
    ├── Installed
    ├── Updates
    └── Sources
```

Discover lists verified registry metadata without downloading or executing extension code. Entries show the extension
identity, version, description, trust state, and requested capability count.

The installation flow is:

1. Install an extension from Discover.
2. Shilpo downloads and verifies the package.
3. The package is installed disabled.
4. Enable it from Installed.
5. Configure its contribution instances in the corresponding shell surface.

An extension update requesting broader capabilities stays downloaded but inactive until the user reviews the new grants.

Installed shows enablement, version, trust, and grant summaries. Updates separates ordinary updates from permission
reviews, incompatible releases, and rollback results. Sources lists configured registries, refreshes their signed
indexes, and removes third-party registries.

The CLI uses the same signed catalog metadata as Settings:

```bash
shilpo ext search wallpaper
shilpo ext info io.github.alice.world-clock
shilpo ext install io.github.alice.world-clock
```

A future web listing may open the corresponding Settings detail page, but cannot bypass signature verification or
permission review. Local archives and signed URLs remain alternative installation routes rather than entries in the
default gallery.

Configure a registry only with its independently obtained Ed25519 root public key:

```bash
shilpo ext source add community "Community" \
  https://extensions.example.org/index.json community-root.pub
shilpo ext refresh-sources
```

For offline development or registry testing, verify and cache an already downloaded index with:

```bash
shilpo ext source sync community ./signed-index.json
```

Indexes are reverified from the configured root key whenever read. Removing a third-party source also removes its cached
index. Official status is not configurable: release builds embed it through
`SHILPO_OFFICIAL_EXTENSIONS_ROOT_KEY` (and optionally `SHILPO_OFFICIAL_EXTENSIONS_INDEX_URL`). User-added sources are
always third-party sources, even if they use an `org.shilpo.*` extension ID.

### Local package

```bash
shilpo ext install ./world-clock-1.0.0.shilpo-ext
shilpo ext enable io.github.alice.world-clock
```

### URL

```bash
shilpo ext install https://example.com/world-clock-1.0.0.shilpo-ext
shilpo ext install https://example.com/world-clock-1.0.0.shilpo-ext \
  --hash sha256:<digest>
```

URL installs require a package hash or trusted signed registry entry. Installing arbitrary Git repository source is a
development workflow, not the default end-user workflow.

### Management

```bash
shilpo ext list
shilpo ext info io.github.alice.world-clock
shilpo ext update io.github.alice.world-clock
shilpo ext approve io.github.alice.world-clock --grant-all
shilpo ext rollback io.github.alice.world-clock
shilpo ext disable io.github.alice.world-clock
shilpo ext enable io.github.alice.world-clock
shilpo ext uninstall io.github.alice.world-clock
```

Disable keeps package files, settings, grants, and extension-owned data. Uninstall removes the package and contribution
instances but asks separately whether persistent extension-owned data should also be removed.

### Update discovery

Shilpo checks for extension updates; extension guest code does not. The catalog selects the highest non-yanked semantic
version that matches the extension ID, expected publisher, selected channel, current Shilpo version, and supported
extension interface.

```bash
shilpo ext check-updates
shilpo ext update io.github.alice.world-clock
shilpo ext update --all
shilpo ext update --all --dry-run
shilpo ext channel io.github.alice.world-clock beta
```

Update behavior follows the installation source:

| Source                       | Behavior                                          |
|------------------------------|---------------------------------------------------|
| Official or trusted registry | Catalog discovery and user-triggered installation |
| One-off local archive or URL | Manual replacement                                |
| Development path             | Never updated automatically                       |

Updates are downloaded into staging, verified, compatibility-checked, and activated atomically. The previous working
version remains available for rollback. Broader capabilities require new approval, and a publisher-key mismatch blocks
the update.

Settings exposes states such as up to date, update available, awaiting permission review, incompatible, publisher
conflict, yanked, failed while using the previous version, rollback active, and development override active.

## Installation paths

On Linux, the implemented paths are:

```text
$XDG_CONFIG_HOME/shilpo/extensions/sources.toml
$XDG_CONFIG_HOME/shilpo/extensions/grants/<id>.toml
$XDG_DATA_HOME/shilpo/extensions/installed/<id>/<version>/
$XDG_DATA_HOME/shilpo/extensions/receipts/<id>.toml
$XDG_DATA_HOME/shilpo/extensions/indexes/<source>.json
$XDG_DATA_HOME/shilpo/extensions/staging/
$XDG_STATE_HOME/shilpo/extensions/dev/<id>.toml
$XDG_STATE_HOME/shilpo/extensions/logs/
```

If an XDG variable is unset, Shilpo uses its standard home-directory fallback.

Installed package files are immutable. Downloads and extraction use the data-directory staging area. Development
registrations and their logs live outside configuration.

## Failure behavior

An invalid or failing extension must not make the shell unusable.

- Invalid manifests are listed with diagnostics but never executed.
- Incompatible interface versions are disabled.
- A render failure keeps the last valid view.
- A contribution failure displays a host-owned error placeholder.
- Repeated traps, timeouts, invalid effects, or resource violations disable the extension for the session.
- Updates are atomic and preserve the previous working package for rollback.
- Removing a development override restores the installed package.
