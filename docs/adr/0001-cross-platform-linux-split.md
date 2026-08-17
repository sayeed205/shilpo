# ADR-0001: Workspace Tiers, Publication Boundaries, and Repo Scope

- **Status**: Accepted

## Context

Shilpo started as a UI component library and evolved into a full desktop environment. The UI library needs to remain
independent and cross-platform for third-party GPUI apps and future Shilpo cross-platform apps, while the shell
ecosystem is inherently Linux-specific (Wayland, DBus, XDG, Niri). This raises three related but distinct questions:
which crates live in which workspace tier, which of those crates are ever published, and which future applications
belong in this repository at all.

## Decision

### 1. Cross-platform / Linux-only workspace split

The workspace is split into two tiers by platform:

- **`core/`** — cross-platform crates: `shilpo-ui`, `shilpo-theme`, `shilpo-macros`, `shilpo-ext-api`.
- **`desktop/`** — Linux-only crates forming the desktop shell environment: `shilpo` (consolidated
  Shell/Settings/CLI/config binary), `shilpo-device`, `shilpo-services`, `shilpo-ext-runtime`, `shilpo-theme-daemon`,
  `shilpo-observability`.

`core/` crates must never depend on `desktop/` crates. `desktop/` crates may depend on `core/` crates freely.

### 2. Publication boundary

Only `core/` crates and official extension SDKs under `sdk/` are intended for publication:

- **Core Tier** (`core/` → crates.io): `shilpo-ui`, `shilpo-theme`, `shilpo-macros`, `shilpo-ext-api`.
- **SDK Tier** (`sdk/`): `shilpo-ext-sdk` (`sdk/rust`, crates.io) and `@shilpo/ext-sdk` (`sdk/typescript`, JSR).

Icon assets are plain data in `core/assets/icons/`; applications bring their own asset-source implementation. All
`desktop/` crates are internal to the Shilpo desktop environment and are never published.

This boundary constrains API design: `core/` and `sdk/` public APIs require semver discipline, documentation, and must
not leak `desktop/` types. `desktop/` crates have no external API stability guarantees.

### 3. Where future cross-platform apps live

Future cross-platform Shilpo apps (file manager, etc.) that are not part of the desktop shell environment live in their
own repositories, not in this workspace. They consume `shilpo-ui` and `shilpo-theme` as published crate dependencies
from crates.io, not as workspace path dependencies. This keeps the shell workspace focused and gives each app its own
release cycle, CI, and issue tracker.

Shell-integrated apps (Settings, CLI) stay in this workspace because they share the desktop ecosystem's internal crates
(`shilpo-services`, config, `shilpo-ext-runtime`) and ship inside the single `shilpo` binary (see
[ADR-0005](0005-unified-executable-process-roles.md)). The dividing line: if an app depends on `desktop/` crates, it
belongs here; if it only depends on `core/` crates, it gets its own repo.

## Consequences

- Third-party GPUI apps and future Shilpo cross-platform apps can consume `shilpo-ui`/`shilpo-theme` without pulling in
  any Linux-only dependency.
- `desktop/` crates are free to take on Linux-specific dependencies and internal API churn without a semver contract.
- A new crate's home is decided by two questions: does it need to run outside Linux, and does it need to be
  independently published? The answers place it in `core/`, `desktop/`, its own repo, or `sdk/`.
