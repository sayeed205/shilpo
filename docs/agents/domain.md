# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`CONTEXT-MAP.md`** at the repo root — it points at one `CONTEXT.md` per crate/app context. Read each one relevant to
  the topic.
- **`docs/adr/`** at the repo root — workspace-wide architectural decisions.
- **Per-crate `docs/adr/`** — read ADRs in the specific crate you're about to work in.

If any of these files don't exist, **proceed silently**. Don't flag their absence; don't suggest creating them upfront.
The `/domain-modeling` skill (reached via `/grill-with-docs` and `/improve-codebase-architecture`) creates them lazily
when terms or decisions actually get resolved.

## File structure

Multi-context repo (one `CONTEXT.md` per crate/app):

```
/
├── CONTEXT-MAP.md                        ← maps contexts to crates
├── docs/adr/                             ← workspace-wide decisions
├── crates/
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
│   ├── services/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── config/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── ext/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── shell/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   ├── settings/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/
│   └── cli/
│       ├── CONTEXT.md
│       └── docs/adr/
├── apps/storybook/
│   ├── CONTEXT.md
│   └── docs/adr/
└── dotfiles/shilpo-dotfiles/
    ├── CONTEXT.md
    └── docs/adr/
```

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the
term as defined in the relevant crate's `CONTEXT.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal — either you're inventing language the project
doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0007 (event-sourced orders) — but worth reopening because…_
