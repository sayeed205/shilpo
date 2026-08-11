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
- **[`shilpo-ext-api`](core/ext-api)**: Extension identity types (`ExtensionId`, `ContributionId`, `CanonicalId`, `IdError`), manifest, events, guest host effects, ViewTree, WIT interface, schema files, and validation.

#### Linux Desktop (`desktop/` — internal, never published)

- **[`shilpo`](desktop/shilpo)**: Consolidated desktop product — Shell daemon, Settings app, CLI dispatch, and declarative TOML config in one executable binary target (`shilpo`).
- **[`shilpo-device`](desktop/device)**: Presentation-neutral versioned device domain protocol and typed DBus client with degraded/reconnect projections and client-side debounce.
- **[`shilpo-services`](desktop/services)**: System service integrations — Wayland/Niri, audio, bluetooth, brightness, network, notifications, media, tray, upower, IPC, screen capture domain, and LMDB session store.
- **[`shilpo-ext-runtime`](desktop/ext-runtime)**: Wasmtime-sandboxed extension runtime with capability authorization, catalog, and worker process protocol.
- **[`shilpo-theme-daemon`](desktop/theme-daemon)**: Theme DBus daemon, XDG portal sync, persistence, third-party adapters (see [ADR-0002](docs/adr/0002-theme-crate-split.md)).

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
    - When introducing or modifying core UI components in `shilpo-ui` (`core/ui`), add interactive stories in
      `apps/storybook/src/stories/`. Storybook is strictly reserved for reusable core UI components, not internal
      desktop shell widgets.
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

Issues and specs live as GitHub Issues on `sayeed205/shilpo`. Use the `gh` CLI for all operations.

#### Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- **Read an issue**: `gh issue view <number> --comments`, filtering comments by `jq` and also fetching labels.
- **List issues**:
  `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`
  with appropriate `--label` and `--state` filters.
- **Comment on an issue**: `gh issue comment <number> --body "..."`
- **Apply / remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "..."`

Infer the repo from `git remote -v` — `gh` does this automatically when run inside a clone.

#### Pull requests as a triage surface

**PRs as a request surface: no.** _(Set to `yes` if this repo treats external PRs as feature requests; `/triage` reads
this flag.)_

When set to `yes`, PRs run through the same labels and states as issues, using the `gh pr` equivalents:

- **Read a PR**: `gh pr view <number> --comments` and `gh pr diff <number>` for the diff.
- **List external PRs for triage**:
  `gh pr list --state open --json number,title,body,labels,author,authorAssociation,comments` then keep only
  `authorAssociation` of `CONTRIBUTOR`, `FIRST_TIME_CONTRIBUTOR`, or `NONE` (drop `OWNER`/`MEMBER`/`COLLABORATOR`).
- **Comment / label / close**: `gh pr comment`, `gh pr edit --add-label`/`--remove-label`, `gh pr close`.

GitHub shares one number space across issues and PRs, so a bare `#42` may be either — resolve with `gh pr view 42` and
fall back to `gh issue view 42`.

#### When a skill says "publish to the issue tracker"

Create a GitHub issue.

#### When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --comments`.

#### Wayfinding operations

Used by `/wayfinder`. The **map** is a single issue with **child** issues as tickets.

- **Map**: a single issue labelled `wayfinder:map`, holding the Notes / Decisions-so-far / Fog body.
  `gh issue create --label wayfinder:map`.
- **Child ticket**: an issue linked to the map as a GitHub sub-issue (`gh api` on the sub-issues endpoint). Where
  sub-issues aren't enabled, add the child to a task list in the map body and put `Part of #<map>` at the top of the
  child body. Labels: `wayfinder:<type>` (`research`/`prototype`/`grilling`/`task`). Once claimed, the ticket is
  assigned to the driving dev.
- **Blocking**: GitHub's **native issue dependencies** — the canonical, UI-visible representation. Add an edge with
  `gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`, where
  `<blocker-db-id>` is the blocker's numeric **database id** (`gh api repos/<owner>/<repo>/issues/<n> --jq .id`, _not_
  the `#number` or `node_id`). GitHub reports `issue_dependencies_summary.blocked_by` (open blockers only — the live
  gate). Where dependencies aren't available, fall back to a `Blocked by: #<n>, #<n>` line at the top of the child body.
  A ticket is unblocked when every blocker is closed.
- **Frontier query**: list the map's open children (`gh issue list --state open`, scoped to the map's sub-issues / task
  list), drop any with an open blocker (`issue_dependencies_summary.blocked_by > 0`, or an open issue in the
  `Blocked by` line) or an assignee; first in map order wins.
- **Claim**: `gh issue edit <n> --add-assignee @me` — the session's first write.
- **Resolve**: `gh issue comment <n> --body "<answer>"`, then `gh issue close <n>`, then append a context pointer
  (gist + link) to the map's Decisions-so-far.

### Triage labels

The skills speak in terms of five canonical triage roles. This section maps those roles to the actual label strings used
in this repo's issue tracker.

#### Triage roles

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
|----------------------------|----------------------|------------------------------------------|
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the corresponding label string from this
table.

> **Note:** `needs-triage`, `needs-info`, and `ready-for-human` are reserved for future use. They are not yet created in
> the repo. Currently, only `ready-for-agent` and `wontfix` exist as deployed labels.

#### Area labels

Scope issues to a specific subsystem. Apply exactly one `area:` label per issue when applicable.

| Label             | Description                                     |
|-------------------|-------------------------------------------------|
| `area:config`     | Configuration system (TOML, schema, migrations) |
| `area:extensions` | Extension runtime, SDK & tooling                |
| `area:theme`      | Theme system (colors, transitions, adapters)    |
| `area:services`   | System service integrations                     |
| `area:compositor` | Compositor integration & backends               |
| `area:cli`        | CLI tooling                                     |
| `area:profiling`  | Observability, tracing & telemetry              |
| `area:i18n`       | Internationalization & localization             |
| `area:sdk`        | Extension SDK (Rust & TypeScript)               |
| `area:store`      | Extension registry & distribution               |
| `area:wit`        | WIT interface definitions                       |

#### Type labels

Classify the nature of the change. Apply one or more `type:` labels.

| Label               | Description                              |
|---------------------|------------------------------------------|
| `type:architecture` | Architectural decisions and improvements |
| `type:dx`           | Developer experience improvement         |
| `type:infra`        | Build, CI, or infrastructure             |
| `type:breaking`     | Breaking change                          |

GitHub's built-in labels (`bug`, `enhancement`, `documentation`) are also used for standard issue types.

#### Priority labels

Assign exactly one `priority:` label per issue.

| Label         | Description                  |
|---------------|------------------------------|
| `priority:p0` | Critical — blocks other work |
| `priority:p1` | High priority                |
| `priority:p2` | Medium priority              |
| `priority:p3` | Low priority                 |

#### Phase labels

Track which roadmap milestone an issue belongs to. Apply exactly one `phase:` label.

| Label               | Description                                |
|---------------------|--------------------------------------------|
| `phase:1-core`      | Phase 1: Core Infrastructure               |
| `phase:2-runtime`   | Phase 2: Extension Runtime                 |
| `phase:3-dx`        | Phase 3: Developer Experience              |
| `phase:4-ecosystem` | Phase 4: Compositor & Service Architecture |

> **Note:** Phase 5 (Pre-Release Hardening) does not have a phase label yet. Issues in Phase 5 are tracked via their
> milestone assignment only.

### Roadmap

The project follows a 5-phase milestone roadmap tracked in GitHub Issues (#111). Phases:

1. **Core Infrastructure** — config overhaul, profiling, developer automation.
2. **Extension Runtime** — typed WIT, ext-api split, contribution types, state store.
3. **Developer Experience** — TypeScript/Rust SDKs, hot-reload, scaffolding CLI, benchmarks.
4. **Compositor & Service Architecture** — multi-compositor support, service trait abstraction, registry.
5. **Pre-Release Hardening** — CI/CD, security, i18n, release process, deferred contribution types.

### Domain docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

#### Before exploring, read these

- **`CONTEXT-MAP.md`** at the repo root — it points at one `CONTEXT.md` per crate/app context. Read each one relevant to
  the topic.
- **`docs/adr/`** at the repo root — workspace-wide architectural decisions.
- **Per-crate `docs/adr/`** — read ADRs in the specific crate you're about to work in.

If any of these files don't exist, **proceed silently**. Don't flag their absence; don't suggest creating them upfront.
The `/domain-modeling` skill (reached via `/grill-with-docs` and `/improve-codebase-architecture`) creates them lazily
when terms or decisions actually get resolved.

#### File structure

Multi-context repo (one `CONTEXT.md` per crate/app):

```
/
├── CONTEXT-MAP.md                        ← maps contexts to crates
├── docs/adr/                             ← workspace-wide decisions
├── core/
│   ├── ui/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── theme/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── macros/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── assets/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   └── ext-api/
│       ├── CONTEXT.md
│       └── docs/adr/
├── desktop/
│   ├── shell/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── settings/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── services/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── config/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── ext-runtime/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── theme-daemon/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   └── cli/
│       ├── CONTEXT.md
│       └── docs/adr/
└── apps/storybook/
    ├── CONTEXT.md
    └── docs/adr/
```

#### Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the
term as defined in the relevant crate's `CONTEXT.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal — either you're inventing language the project
doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

#### Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0007 (event-sourced orders) — but worth reopening because…_

