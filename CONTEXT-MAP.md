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
- **Assets** (`core/assets`) — Asset loader primitives. `rust-embed` on native desktop,
  `reqwest` CDN fetching on WASM. Apps bring their own asset loader; this crate provides the interface and default
  bundled SVG icons.

### Linux Desktop (`desktop/`)

- **Shell** (`desktop/shell`) — The main desktop shell daemon. Top bar, control center, workspace overview,
  notifications, OSD, extension surface rendering, action registry, keybinding management. Also owns shell-specific
  presentational widgets (Bluetooth, Network, SysInfo, Media, Caffeine).
- **Settings** (`desktop/settings`) — Standalone control panel application. Material 3 settings UI with navigation rail,
  category pages for network, bluetooth, themes, bar, desktop configuration. Same product as Shell, separate binary.
- **Services** (`desktop/services`) — Linux system integration services. Wayland/Niri compositor IPC, audio, bluetooth,
  brightness, caffeine, clipboard, location, media, network, night light, notifications, power profile, screen capture,
  tray, upower, app scanning.
- **Config** (`desktop/config`) — Shell configuration management. TOML config loading/validation, XDG directory
  resolution, LMDB session storage via `heed`. Imports extension ID types from Ext for validating extension contribution
  references in config.
- **Ext** (`desktop/ext`) — Wasmtime-sandboxed extension runtime. Capability-based security model, extension manifests,
  package catalog/registry, WASI component-model host, ViewTree schema rendering. Shell-only — extends shell
  capabilities.
- **Theme Daemon** (`desktop/theme-daemon`) — Linux theme system integration. DBus service (`org.shilpo.Theme`), XDG
  portal appearance sync, wallpaper watching, atomic JSON persistence, theme adapters for third-party tools (GTK, Foot,
  Alacritty, Kitty, Hyprland).
- **CLI** (`desktop/cli`) — Command-line interface (`shilpo`). Controls shell daemon, switches theme modes, manages
  extensions, runs environment doctor checks.

### Applications (`apps/`)

- **Storybook** (`apps/storybook`) — Interactive desktop gallery for exploring and testing core UI components.
  Cross-platform. Demos the generic M3 component library only, not shell-specific widgets.

## Relationships

- **UI → Theme**: UI imports `ThemeMode`, `SchemeVariant`, `ThemeState` as pure data types. UI owns the GPUI `Theme`
  global and `ThemeColor` rendering; Theme provides the color math.
- **UI → Macros**: UI uses `icon_named!` to generate icon enums from SVG assets at compile time.
- **UI → Assets**: UI loads bundled SVG icons via the asset source interface.
- **Shell → UI**: Shell renders all UI using core M3 components.
- **Shell → Theme Daemon**: Shell subscribes to theme daemon for system-wide theme synchronization.
- **Shell → Services**: Shell wires system service data into presentational widgets via service worker channels.
- **Shell → Config**: Shell loads and validates its configuration.
- **Shell → Ext**: Shell hosts and coordinates extensions.
- **Settings → UI, Services, Theme Daemon, Config, Ext**: Same dependency set as Shell — same product, separate binary.
- **Config → Ext**: Config imports `CanonicalId`, `ExtensionId` for validating extension references in TOML config
  files. Lightweight type-sharing, not deep coupling.
- **Theme Daemon → Theme**: Daemon uses core theme types and color generation.
- **Theme Daemon → Config**: Daemon reads shell config and uses XDG state directories for persistence.
- **Storybook → UI, Theme, Assets**: Demos the core component library.
