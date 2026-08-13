# Dependency upgrade notes

This file records dependency upgrades that can require maintainer attention beyond refreshing `Cargo.lock`. Registry
dependencies in manifests use major-only requirements for stable `1.x` and later releases, and `0.x` requirements retain
the compatible minor line. Git dependencies are intentionally excluded from dependency refreshes.

## Current major-line migrations

### `fake` 2.x to 5.x

The Storybook data-generation code continues to use the `Fake` trait and the
`fake::faker` helpers. If a future `fake` release changes those APIs, update the Storybook stories under
`apps/storybook/src/stories/` and run the Storybook checks.

### `rand` 0.8 to 0.10

The Storybook uses `rand::random` and slice selection. The 0.10 API uses the new RNG naming and selection traits; keep
those call sites aligned with the version selected in `apps/storybook/Cargo.toml`.

### `syn` 2.x to 3.x

The procedural macros in `core/macros` use parsing, token, and derive-input APIs from `syn`. Any future incompatibility
belongs in the macro crate and must be checked by building the proc-macro consumers.

`notify` 9 is currently prerelease-only, so the workspace remains on the stable 8.x line until a stable 9.x release is
available. The desktop watchers use
`RecommendedWatcher`, `Event`, `Config`, and `RecursiveMode`; update the watchers in `desktop/shilpo` and
`desktop/services` when that stable migration becomes appropriate.

`ddc` remains on the 0.2 line because `ddc-i2c` 0.2.2 implements its device traits against `ddc` 0.2. Upgrading the
workspace dependency independently to 0.3 creates two incompatible trait versions and breaks the brightness adapter.
Upgrade both crates together once `ddc-i2c` publishes support for `ddc` 0.3.
