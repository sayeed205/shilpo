# ADR-0001: Workspace Tiers, Publication Boundaries, and Repo Scope

- **Status**: Accepted

## Context

Shilpo started as a UI component library and evolved into a full desktop environment. The UI library needs to remain
independent and cross-platform for third-party GPUI apps and future Shilpo cross-platform apps, while the shell
ecosystem is inherently Linux-specific (Wayland, DBus, XDG, Niri). This raises three related but distinct questions:
which crates live in which repository, which of those crates are ever published, and which future applications
belong in this repository at all.

## Decision

### 1. Cross-platform / Linux-only split

Cross-platform code and the Linux-only desktop shell are kept separate:

- **`core/`** in this repo — cross-platform crates that are specifically about this repo's own extension contract:
  `shilpo-ext-api`, and the extension registry wire contract (index, release, and signature types) that the registry's
  index generator must consume without pulling in the Linux host runtime — see
  [ADR-0018](0018-extension-registry-distribution.md).
- **The UI component library, its theme/color math, and shared macros live in a separate repository**,
  [shilpo-rs/ui](https://github.com/shilpo-rs/ui) (`shilpo-m3e`, `shilpo-theme`, `shilpo-macros`), consumed here as a
  git dependency pinned to an exact revision — not a local workspace member. Unlike `core/ext-api`, this code has no
  reason to be scoped to Shilpo's own extension contract, so it gets its own repository rather than a subdirectory
  here (see [ADR-0002](0002-theme-crate-split.md) for the theme split that preceded this, and the SDK Tier precedent
  below).
- **`desktop/`** — Linux-only crates forming the desktop shell environment: `shilpo` (consolidated
  Shell/Settings/CLI/config binary), `shilpo-device`, `shilpo-services`, `shilpo-ext-runtime`, `shilpo-theme-daemon`,
  `shilpo-observability`.

`core/` crates must never depend on `desktop/` crates. `desktop/` crates may depend on `core/` crates, and on
`shilpo-rs/ui` crates, freely.

### 2. Publication boundary

- **Core Tier** (`core/` → crates.io): `shilpo-ext-api` and the extension registry wire contract.
- **UI Tier**: `shilpo-m3e`, `shilpo-theme`, `shilpo-macros` (crates.io), maintained in
  [shilpo-rs/ui](https://github.com/shilpo-rs/ui) rather than under `core/` in this one.
- **SDK Tier**: `shilpo-ext-sdk` (crates.io) and `@shilpo/ext-sdk` (JSR), maintained in
  [shilpo-rs/sdks](https://github.com/shilpo-rs/sdks). They target `core/ext-api`'s WIT contract at a pinned revision
  instead of a live path dependency.
- **Registry Tier**: [shilpo-rs/extensions](https://github.com/shilpo-rs/extensions) is the extension registry. It holds
  extension source, builds and signs artifacts in CI, and publishes the signed index. It is a distribution repository
  rather than a published crate, and consumes the Core Tier wire contract at a pinned revision. See
  [ADR-0018](0018-extension-registry-distribution.md).

Icon assets are plain data in `core/assets/icons/`; applications bring their own asset-source implementation. All
`desktop/` crates are internal to the Shilpo desktop environment and are never published.

This boundary constrains API design: `core/` and any published-tier public APIs require semver discipline,
documentation, and must not leak `desktop/` types. `desktop/` crates have no external API stability guarantees.

### 3. Where future cross-platform apps live

Future cross-platform Shilpo apps (file manager, etc.) that are not part of the desktop shell environment live in their
own repositories, not in this workspace — `storybook`, the UI component gallery, already made this move alongside the
UI library itself. They consume `shilpo-m3e` and `shilpo-theme` as published crate dependencies from crates.io, not as
workspace path dependencies. This keeps the shell workspace focused and gives each app its own release cycle, CI, and
issue tracker.

Shell-integrated apps (Settings, CLI) stay in this workspace because they share the desktop ecosystem's internal crates
(`shilpo-services`, config, `shilpo-ext-runtime`) and ship inside the single `shilpo` binary (see
[ADR-0005](0005-unified-executable-process-roles.md)). The dividing line: if an app depends on `desktop/` crates, it
belongs here; if it only depends on `core/`-tier crates, it gets its own repo.

## Consequences

- Third-party GPUI apps and future Shilpo cross-platform apps can consume `shilpo-m3e`/`shilpo-theme` without pulling
  in any Linux-only dependency, and without cloning this repository at all.
- `desktop/` crates are free to take on Linux-specific dependencies and internal API churn without a semver contract.
- A new crate's home is decided by two questions: does it need to run outside Linux, and does it need to be
  independently published? The answers place it in `core/`, `desktop/`, its own repo, the UI Tier repo
  ([shilpo-rs/ui](https://github.com/shilpo-rs/ui)), or the SDK Tier repo ([shilpo-rs/sdks](https://github.com/shilpo-rs/sdks)).
- Cross-repo dependencies (UI Tier, SDK Tier) are always git dependencies pinned to an exact revision, never a branch,
  matching how this repo already pins its gpui fork — reproducible regardless of later commits upstream.
