# Published crates limited to core and SDK tiers

Only `core/` crates and official extension SDKs under `sdk/` are intended for publication:
- **Core Tier** (`core/` — crates.io): `shilpo-ui`, `shilpo-theme`, `shilpo-macros`, and `shilpo-ext-api`.
- **SDK Tier** (`sdk/`): `shilpo-ext-sdk` (`sdk/rust/` on crates.io) and `@shilpo/ext-sdk` (`sdk/typescript/` on JSR).

Icon assets are plain data located in `core/assets/icons/`, and applications are expected to bring their own asset source implementation. All `desktop/` crates are internal to the Shilpo desktop environment and never published.

This boundary constrains API design: `core/` and `sdk/` public APIs require semver discipline, documentation, and must not leak `desktop/` types. `desktop/` crates have no external API stability guarantees.
