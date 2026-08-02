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

| Crate               | Description                                                        | Directory                          |
|:--------------------|:-------------------------------------------------------------------|:-----------------------------------|
| **`shilpo-ui`**     | Core desktop UI component library for GPUI applications            | [`crates/ui`](crates/ui)           |
| **`shilpo-macros`** | Procedural macros for icon generation and plot traits              | [`crates/macros`](crates/macros)   |
| **`shilpo-assets`** | Internal SVG icon set and demo asset loader (*unpublished*)        | [`crates/assets`](crates/assets)   |
| **`storybook`**     | Interactive component gallery application for reviewing components | [`apps/storybook`](apps/storybook) |

---

## Quick Start

To launch the interactive Storybook component gallery:

```bash
cargo run -p storybook
```

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
