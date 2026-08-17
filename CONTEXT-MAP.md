# Context Map

Shilpo is a Linux desktop environment ecosystem built on GPUI, with a cross-platform Material Design 3 UI component
library at its core.

## Contexts

### Cross-Platform (`core/`)

- **UI** (`core/ui`) — Material Design 3 GPUI component library. Generic, publishable UI primitives: buttons, dialogs,
  inputs, tables, trees, menus, popovers, sheets, virtual lists. Owns the `Theme` GPUI global, `ActiveTheme` trait, and
  `ThemeColor` token system. Platform-gated for macOS (AppKit), Windows (Win32), and WASM.
- **Theme** (`core/theme`) — M3 color math and data types. Pure computation: seed color → M3 scheme generation via
  `mcu_material_color`. Defines `ThemeMode`, `SchemeVariant`, `ColorSource`, `ThemeState`,
  `ThemeCommand`, `reduce()`. Zero I/O, zero system dependencies.
- **Macros** (`core/macros`) — Procedural macros. `icon_named!` generates icon enums from SVG asset directories.
  `#[derive(IntoPlot)]` for chart traits.
- **Assets** (`core/assets/icons`) — Plain canonical SVG icon data. Applications own their GPUI `AssetSource`; no asset
  Cargo package or default runtime loader is published.
- **Extension API** (`core/ext-api`) — Extension identity types (`ExtensionId`, `ContributionId`, `CanonicalId`,
  `IdError`), extension manifest schemas, events, guest host effects, ViewTree declarative UI family, API/schema
  constants, and WIT interface contract (`wit/extension.wit`). Cross-platform, zero runtime/Wasmtime dependencies.

### Linux Desktop (`desktop/`)

- **Shilpo** (`desktop/shilpo`) — Consolidated desktop product package. Contains Shell daemon (`shell`), Settings app
  (`settings`), public CLI dispatch (`cli`), and declarative TOML configuration/validation (`config`). Exposes the
  `org.shilpo.Shell` and `org.shilpo.Debug` D-Bus control plane
  ([ADR-0012](docs/adr/0012-dbus-shell-control-plane.md), [ADR-0013](docs/adr/0013-runtime-debug-control.md)). Produces
  the single installed executable binary target (`shilpo`).
- **Device** (`desktop/device`) — Presentation-neutral versioned device domain protocol (`protocol`) and typed DBus
  client (`client`) with degraded/reconnect projections and client-side debounce.
- **Services** (`desktop/services`) — Linux system integration services. Device daemon (`DeviceDaemonService`),
  Wayland/Niri compositor IPC, audio, bluetooth, brightness, caffeine, clipboard, location, media, network, night light,
  notifications, power profile, screen capture domain (`capture`), tray, upower, app scanning, and LMDB session store.
- **Extension Runtime** (`desktop/ext-runtime`) — Wasmtime-sandboxed extension runtime. Capability authorization,
  package catalog/registry index, WASI component-model host, worker process protocol (`shilpo extension-host`).
- **Theme Daemon** (`desktop/theme-daemon`) — Linux theme system integration. DBus service (`org.shilpo.Theme`), XDG
  portal appearance sync, wallpaper watching, bounded in-memory wallpaper analysis caching, atomic JSON persistence,
  theme adapters for third-party tools (GTK, Foot, Alacritty, Kitty, Hyprland).
- **Observability** (`desktop/observability`) — Internal process observability crate. Standardized subscriber
  initialization with reloadable filter controller (`LogFilterController`), opt-in Chrome trace generation
  (`SHILPO_PROFILE`), collision-resistant trace path management, trace discovery/export (`shilpo profile export`), and
  local telemetry summary inventory (`shilpo doctor --telemetry`).

### Applications (`apps/`)

- **Storybook** (`apps/storybook`) — Interactive desktop gallery for exploring and testing core UI components.
  Cross-platform. Demos the generic M3 component library only, not shell-specific widgets.

### SDKs (`sdk/`)

- **TypeScript SDK** (`sdk/typescript`) — Official TypeScript SDK (`@shilpo/ext-sdk@0.1.0`) for developing sandboxed
  WebAssembly extensions. Provides declarative ViewTree builders, typed `DataValue` helpers, `defineExtension` lifecycle
  adapter, host import facade, and in-memory test host. Published exclusively to JSR.
- **Rust SDK** (`sdk/rust`) — Official Rust SDK (`shilpo-ext-sdk@0.1.0`) for developing sandboxed WebAssembly
  extensions. Provides declarative ViewTree builders, `view!` macro, typed `DataValue` conversions, `Extension`
  lifecycle trait, `State` helper, and re-exported canonical WIT bindings. Published to crates.io.

## Relationships

- **UI → Theme**: UI imports `ThemeMode`, `SchemeVariant`, `ThemeState` as pure data types. UI owns the GPUI `Theme`
  global and `ThemeColor` rendering; Theme provides the color math.
- **UI → Macros**: UI uses `icon_named!` to generate icon enums from SVG assets at compile time.
- **Shilpo → UI**: Shilpo renders Shell and Settings UI using core M3 components.
- **Shilpo → Theme Daemon**: Shilpo subscribes to theme daemon for system-wide theme synchronization and runs the theme
  daemon role with narrow options.
- **Shilpo → Services**: Shilpo wires system service data into presentational widgets via service worker channels.
- **Shilpo → Device**: Shilpo submits live device commands and observes per-domain daemon revisions through the typed
  client seam.
- **Services → Device**: Services hosts the device daemon (`DeviceDaemonService`), which owns command arbitration,
  per-domain queues, and Linux service adapters.
- **Shilpo → Ext Runtime**: Shilpo hosts and coordinates extensions via supervisor worker process
  (`shilpo extension-host`).
- **Ext Runtime → Ext API**: Ext Runtime implements guest contracts and capability authorization for Extension API
  types.
- **Services → LMDB Session Store**: Services owns operational/session persistence (clipboard history, output state)
  independently of Shilpo declarative config.
- **Theme Daemon → Theme**: Daemon uses core theme types and color generation.
- **TypeScript SDK → Ext API**: TypeScript SDK generates typed interfaces from `core/ext-api/wit/extension.wit` and
  manifest schema from `core/ext-api/schema/extension-v1.schema.json`.
- **Storybook → UI, Theme**: Demos the core component library.
- **Services Domain Ports**: Long-lived service domains use domain-specific ports with the shared ADR-0006 operational
  semantics; process-owned ports use typed DBus clients and in-process ports use narrow handles.
