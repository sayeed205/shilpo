# Third-party notices

## GPUI

Shilpo uses direct pinned `gpui = 0.2.2`, licensed Apache-2.0.

## Google Material Symbols

`ui/shilpo-icons/assets/` contains supplied Google Material Symbols assets from
https://fonts.google.com/icons. Copyright Google LLC. Licensed under Apache License 2.0. Modified
2026-07-16: replaced fixed fills with `currentColor` so GPUI can tint them from Shilpo theme
colors.

## Zed-derived source

`ui/shilpo-ui/src/zed_derived/` contains unchanged byte-copies from Zed commit
`97110fd5a119eb5fad49524dc04d7c042193e8ab`, licensed GPL-3.0-or-later. Exact source paths and
copy status are documented in `ui/shilpo-ui/src/zed_derived/IMPORT.md`. Raw modules are not
declared by Shilpo `lib.rs` and are pending adaptation. No Zed crate dependencies, icons, assets,
or fonts were copied. Full license text: `licenses/GPL-3.0-or-later.txt`.

## Vendored Zed crate stack

Vendored Zed sources use commit `97110fd5a119eb5fad49524dc04d7c042193e8ab`.

- GPUI core: `gpui`, `gpui_util`, `gpui_macros`, `gpui_shared_string`, `scheduler`, `sum_tree`, `http_client`, `media`, `util_macros` — Apache-2.0 metadata where declared.
- Theme stack: `collections`, `refineable`, `derive_refineable` — Apache-2.0; `syntax_theme`, `theme` — GPL-3.0-or-later.
- UI leaf: `component`, `icons`, `menu`, `ui_macros` — GPL-3.0-or-later.
- Normal-build support: `perf` — Apache-2.0; `zlog`, `ztracing`, `ztracing_macro` — GPL-3.0-or-later.

License texts are retained beside vendored crates in `vendor/zed/**` where provided by Zed. Vendored sources remain subject to upstream licenses and notices.
