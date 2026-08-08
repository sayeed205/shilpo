# Published crates limited to core tier

Only `core/` crates are intended for eventual publication to crates.io: `shilpo-ui`, `shilpo-theme`, and
`shilpo-macros`. `shilpo-assets` is never published — it bundles demo SVG icons and provides a reference asset loader,
but applications are expected to bring their own asset source implementation. All `desktop/` crates are internal to the
Shilpo desktop environment and never published.

This boundary constrains API design: `core/` crate public APIs require semver discipline, documentation, and must not
leak `desktop/` types. `desktop/` crates have no external API stability guarantees.
