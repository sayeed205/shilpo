# Shilpo Wallpaper Extension

Official host-supervised wallpaper provider extension for Shilpo (`org.shilpo.wallpaper`).

## Overview

This extension implements the `wallpaper_providers` contribution contract over `shilpo:extension@0.1.0`. It receives host-supervised `wallpaper_request` events for manual next, slideshow ticks, and workspace transitions, selecting appropriate wallpapers and calling `shilpo:extension/wallpaper.set` with correlated request IDs and targets.

Configure an ordered, non-empty `wallpaper_paths` list in the extension settings. Paths are local files validated by the
host; the extension does not read the filesystem. `slideshow_enabled`, `slideshow_interval_seconds`, and
`workspace_map` control host-scheduled rotation and deterministic workspace selection.

## Capabilities

- `events:subscribe`: `workspace_changed`
- `wallpaper:set`: Set validated local wallpaper files

## Building

```bash
cargo build --manifest-path extensions/Cargo.toml --workspace --target wasm32-wasip2 --release
```
