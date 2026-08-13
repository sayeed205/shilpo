# Shilpo

## Install from source

Shilpo includes an automated source installer for its system dependencies, release binaries, user services, and bundled
weather extension:

```bash
./setup install
```

See [the installation guide](docs/installation.md) for dependency lists, dry-run/update/uninstall commands, and the
daily-driver cutover checklist.

A modern, high-performance desktop UI component framework built on top of [GPUI](https://github.com/zed-industries/zed),
featuring Material Design 3 (M3) & Material Expressive design inspirations.

---

## Workspace Crates

| Crate                     | Description                                                     | Directory                                      |
|:--------------------------|:----------------------------------------------------------------|:-----------------------------------------------|
| **`shilpo-ui`**           | Core desktop UI component library for GPUI applications         | [`core/ui`](core/ui)                           |
| **`shilpo-theme`**        | M3 color math & data types (*cross-platform core*)              | [`core/theme`](core/theme)                     |
| **`shilpo-macros`**       | Procedural macros for icon generation and plot traits           | [`core/macros`](core/macros)                   |
| **`shilpo-ext-api`**      | Cross-platform extension contract                               | [`core/ext-api`](core/ext-api)                 |
| **`shilpo`**              | Consolidated desktop product (Shell, Settings, CLI, Config)     | [`desktop/shilpo`](desktop/shilpo)             |
| **`shilpo-device`**       | Presentation-neutral device domain protocol & typed DBus client | [`desktop/device`](desktop/device)             |
| **`shilpo-services`**     | Linux system service integrations & capture domain              | [`desktop/services`](desktop/services)         |
| **`shilpo-ext-runtime`**  | Wasmtime extension runtime                                      | [`desktop/ext-runtime`](desktop/ext-runtime)   |
| **`shilpo-theme-daemon`** | Theme DBus daemon & system sync                                 | [`desktop/theme-daemon`](desktop/theme-daemon) |
| **`storybook`**           | Interactive component gallery application                       | [`apps/storybook`](apps/storybook)             |

---

## Quick Start

To launch the interactive Storybook component gallery:

```bash
cargo run -p storybook
```

---

## Developer Workflows

Shilpo uses [`just`](https://just.systems/) as a command runner for common development, formatting, linting, testing, and static analysis workflows. Development commands require `just` along with Cargo tools (`cargo-nextest` for tests and `cargo-llvm-cov` for coverage).

To discover all available recipes:

```bash
just --list
```

Common recipes:

```bash
# Format Rust files in place
just fmt

# Run Clippy lints with zero warning tolerance
just lint

# Run all workspace tests
just test

# Run tests for a specific crate
just test shilpo-ui

# Run mutating formatting, linting, and workspace tests in sequence
just check
```

> **Note**: `just fmt` (and consequently `just check`) modifies Rust source files in place to enforce workspace formatting rules.

### Opt-in profiling

Profile a durable role locally by setting `SHILPO_PROFILE=1` before starting or restarting it, exercise the workload, then
stop or restart the role so its completed trace is finalized. Inspect the local inventory and export a trace with:

```bash
SHILPO_PROFILE=1 shilpo shell restart
shilpo doctor --telemetry
shilpo profile export --output trace.json
```

Open the exported JSON in Perfetto or Chrome Trace. Active `.json.part` files are incomplete; runtime rotation belongs to
the later runtime-control work.

### Shell D-Bus control

The running shell owns `org.shilpo.Shell` on the user session bus at `/org/shilpo/Shell`. Inspect the typed interfaces (`org.shilpo.Shell` and `org.shilpo.Debug`) with:

```bash
busctl --user introspect org.shilpo.Shell /org/shilpo/Shell
busctl --user call org.shilpo.Shell /org/shilpo/Shell org.shilpo.Shell GetStatus
busctl --user call org.shilpo.Shell /org/shilpo/Shell org.shilpo.Shell ToggleBar
busctl --user call org.shilpo.Shell /org/shilpo/Shell org.shilpo.Debug GetLogFilter
busctl --user call org.shilpo.Shell /org/shilpo/Shell org.shilpo.Debug SetLogFilter s "info,shilpo=debug"
busctl --user call org.shilpo.Shell /org/shilpo/Shell org.shilpo.Debug EmitTestNotification ss "Test Title" "Test Body"
```

The `shilpo shell`, workspace, window, capture, brightness, and config commands use this interface; debug operations are
available through the `busctl` calls above. No shell socket or lock file is required.


---

## Guidelines for AI Assistants & Contributors

If you are an AI coding assistant or open-source contributor working on this repository, please consult [
`AGENTS.md`](AGENTS.md) for architecture layout, `rtk` command execution guidelines, clippy/nextest standards, and
design system rules.

---

## Acknowledgements & Prior Art

`Shilpo` started as a fork and copy of [`gpui-component`](https://github.com/longbridge/gpui-component). We extend our
deep gratitude to the original authors and maintainers of `gpui-component` for creating a fantastic foundation.

`Shilpo` has since evolved with extensive modifications, including Material Design 3 / Material Expressive design
tokens, customized layout physics, desktop notification integrations, and tailored component styling.

> **Disclaimer**: `Shilpo` is an independent open-source project and is **not affiliated with, endorsed by, or supported
by Google or the `gpui-component` maintainers in any way.**

---

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
