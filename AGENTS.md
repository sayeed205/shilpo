# Agent Guidelines for Shilpo

Welcome! This document serves as a guide for AI agents and coding assistants working on the `Shilpo` codebase.

---

## 1. Project Architecture Overview

`Shilpo` is a Linux desktop environment ecosystem built on [GPUI](https://github.com/zed-industries/zed), inspired by
**Material Design 3 (M3 Expressive / Material You)**
design systems. It includes a cross-platform UI component library (`shilpo-ui`), a Linux desktop shell, a settings app,
system services, a theme daemon, an extension runtime, and a CLI.

See `CONTEXT-MAP.md` for the full context map and inter-crate relationships, and `docs/adr/` for architectural decision
records.

### Workspace Structure

The workspace is split into two tiers (see [ADR-0001](docs/adr/0001-cross-platform-linux-split.md)):

#### Cross-Platform (`core/` — eventually published)

- **[`shilpo-ui`](core/ui)**: M3 GPUI component library. Generic, publishable UI primitives.
- **[`shilpo-theme`](core/theme)**: M3 color math, scheme generation, and theme data types. Pure computation, no I/O.
- **[`shilpo-macros`](core/macros)**: Procedural macros (`icon_named!`, `#[derive(IntoPlot)]`).
- **[`shilpo-assets`](core/assets)**: Asset loader primitives and bundled demo SVG icons. **Note**: Unpublished;
  applications bring their own asset loader.

#### Linux Desktop (`desktop/` — internal, never published)

- **[`shilpo-shell`](desktop/shell)**: Desktop shell daemon — bar, control center, workspace overview, notifications,
  OSD, extension surfaces, shell-specific widgets.
- **[`shilpo-settings`](desktop/settings)**: Control panel application — same product as shell, separate binary.
- **[`shilpo-services`](desktop/services)**: System service integrations — Wayland/Niri, audio, bluetooth, brightness,
  network, notifications, media, tray, upower, IPC.
- **[`shilpo-config`](desktop/config)**: TOML configuration, XDG directory resolution, LMDB session storage.
- **[`shilpo-ext-types`](desktop/ext-types)**: Extension ID types (`ExtensionId`, `ContributionId`, `CanonicalId`) and string validation logic.
- **[`shilpo-ext`](desktop/ext)**: Wasmtime-sandboxed extension runtime with capability-based security.
- **[`shilpo-theme-daemon`](desktop/theme-daemon)**: Theme DBus daemon, XDG portal sync, persistence, third-party
  adapters (see [ADR-0002](docs/adr/0002-theme-crate-split.md)).
- **[`shilpo-cli`](desktop/cli)**: CLI tool for controlling the shell, themes, and extensions.

#### Applications (`apps/`)

- **[`storybook`](apps/storybook)**: Interactive desktop gallery for exploring and testing core UI components.

---

## 2. Using the `rtk` Prefix for Command Execution

> **CRITICAL RULE FOR AI AGENTS**:
> Whenever executing `cargo` commands in the terminal (clippy, testing, building, coverage), **always prefix the command
with `rtk`**.

### Recommended `rtk` Commands

| Purpose              | Standard Command                         | **Agent Command (Use This)**                 |
|:---------------------|:-----------------------------------------|:---------------------------------------------|
| **Linting / Clippy** | `cargo clippy --workspace --all-targets` | `rtk cargo clippy --workspace --all-targets` |
| **Fast Testing**     | `cargo nextest run --workspace`          | `rtk cargo nextest run --workspace`          |
| **Standard Testing** | `cargo test -p shilpo-ui --lib`          | `rtk cargo test -p shilpo-ui --lib`          |
| **Code Coverage**    | `cargo llvm-cov --workspace`             | `rtk cargo llvm-cov --workspace`             |
| **Workspace Build**  | `cargo build --workspace`                | `rtk cargo build --workspace`                |
| **Storybook App**    | `cargo run -p storybook`                 | `rtk cargo run -p storybook`                 |

---

## 3. Tooling Standards: Clippy, Nextest, and LLVM-Cov

### Clippy (`rtk cargo clippy --workspace --all-targets`)

- **Zero Warnings Policy**: All code added or modified must pass `rtk cargo clippy --workspace --all-targets` with **0
  errors and 0 warnings**.
- Keep doc comments updated and clean up unused imports.

### Nextest (`rtk cargo nextest run`)

- `cargo-nextest` is the preferred test runner for running tests in parallel.
- Run unit tests for individual crates using:
  ```bash
  rtk cargo nextest run -p shilpo-ui
  ```

### LLVM Coverage (`rtk cargo llvm-cov`)

- Use `llvm-cov` to audit code coverage when implementing or refactoring core components:
  ```bash
  rtk cargo llvm-cov --workspace --summary-only
  ```

---

## 4. Coding & Design System Guidelines

1. **Material 3 Expressive Aesthetics**:
    - Use curated M3 theme color tokens (`cx.theme().primary`, `cx.theme().surface_container`,
      `cx.theme().on_surface_variant`, etc.) instead of hardcoded colors.
    - Support M3 motion curves (e.g. M3 Emphasized Easing `cubic_bezier(0.2, 0.0, 0.0, 1.0)`)
      over $200\text{ms}$–$300\text{ms}$.
    - **Desktop Target Adaptations**: Focus on native desktop UI patterns. Use full stadium pill shapes (`rounded_full`,
      `shadow_lg`) for `FloatingToolbar` and `rounded_3xl` for `Carousel`. Omit mobile-only screen headers (`TopAppBar`,
      mobile search overlays) when desktop window titlebars serve the layout.
2. **GPUI Element Patterns**:
    - Implement GPUI traits (`IntoElement`, `RenderOnce`, `Sizable`, `Selectable`, `Disabled`) consistently.
    - Support mouse interaction safety (e.g. `cx.stop_propagation()` on mouse down for draggable titlebars).
3. **Interactive Documentation**:
    - When introducing or modifying UI components, add interactive stories in `apps/storybook/src/stories/`.
    - **Full Event Handler Wiring**: Ensure all component interactive events (`on_click`, `on_index_change`,
      `on_change`)
      are explicitly wired in Storybook stories using `cx.entity().clone()` / `entity.update(cx, ...)` so all toggles,
      slides, and selections are testable live.

---

## 5. Documentation Maintenance

- Standard `README.md` files aimed at human developers must use standard `cargo` commands (e.g.,
  `cargo run -p storybook`).
- Do not add `rtk` prefixes to public `README.md` files; keep `rtk` instructions internal to `AGENTS.md` and agent
  workflows.

---

## 6. Agent Skills

### Issue tracker

Issues are tracked in GitHub Issues on `sayeed205/shilpo`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default label vocabulary — `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See
`docs/agents/triage-labels.md`.

### Domain docs

Multi-context layout — root `CONTEXT-MAP.md` + per-crate `CONTEXT.md` and `docs/adr/`. See `docs/agents/domain.md`.
