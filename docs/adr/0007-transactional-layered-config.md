# ADR-0007: Transactional Layered Configuration with Provenance and Scoped Recovery

- **Status**: Accepted

## Context

Shilpo previously loaded declarative configuration directly from a single `config.toml` file into a fully materialized `ShellConfig` struct. As the desktop ecosystem evolved to support fragment directories (`conf.d/*.toml`), user overrides (`overrides.toml`), and runtime configuration reloads, a single-file deserialization path became insufficient. Furthermore, operational data (such as LMDB session stores, clipboard history, or window state) was at risk of being conflated with declarative intent.

We required a unified, deterministic configuration resolver that:
1. Merges defaults, primary configuration, fragment files, and overrides in strict precedence order.
2. Tracks winning source provenance (file location and 1-based line/column) for every effective leaf value.
3. Provides transactional publication and scoped recovery semantics upon validation failures.
4. Maintains strict separation between declarative intent and operational/session projections.

## Decision

We implement `ConfigResolver`, `ConfigSnapshot`, `ConfigProvenance`, and `ConfigChangeSet` in `desktop/shilpo/src/config/`.

### 1. Declarative Intent vs Session / Operational Projections

Declarative configuration (`ShellConfig`) represents human-authored user intent (theme, layout, widget placement, startup options, capture defaults).

Operational and session projections (`ShellSessionState`, LMDB session stores, clipboard history, audio/device state) are explicitly separated from declarative configuration:
- `ShellSessionState` is stored independently (`session.json`).
- Operational storage for desktop services is owned by `desktop/services` (LMDB heed store).
- Extension-scoped operational state is owned strictly by `shilpo-ext-runtime` (`state.lmdb`), separate from declarative TOML intent and from `desktop/services` session LMDB.
- The `ConfigResolver` never ingests, publishes, or records provenance for operational/session keys.

### 2. Source Precedence and Deep-Merge Rules

For any given configuration directory, resolution occurs in strict low-to-high precedence order:
1. `Defaults`: `ShellConfig::default()` (synthetic source).
2. `Primary`: `config.toml`, if present and non-empty.
3. `Fragment`: Regular files immediately inside `conf.d/` whose extension is `.toml`, sorted deterministically by byte/OS-string file name (`01-foo.toml`, `02-bar.toml`). Non-TOML files, nested directories, and subfiles are ignored.
4. `Overrides`: `overrides.toml`, if present and non-empty.

Deep-merge rules:
- **Tables**: Deep-merged recursively. Key conflicts recurse into nested tables.
- **Scalars & Arrays**: Replaced completely by the later value. Stale child provenance entries below replaced paths are removed.
- **Type Conflicts**: Table vs scalar/array conflicts are resolved by replacing the earlier value with the later value. Final typed validation determines candidate validity.

### 3. Transactional Publication and Scoped Recovery

Configuration resolution is transactional: sources are merged into a candidate TOML syntax tree, deserialized once, validated once, and published as a single immutable `ConfigSnapshot`. Intermediate or partially merged states are never exposed to consumers.

When validation fails, one of three recovery scopes is applied:
- `RejectValue`: Semantic validation failure isolated to a scalar/leaf field with an unambiguous default/previous value (`theme.font_family`, `bar.height`, etc.). The single leaf is restored from the previous valid snapshot (or default during initial load), provenance is restored, and the candidate is revalidated.
- `RetainPreviousComponent`: Component-level failure inside an independently owned top-level component (`theme`, `bar`, `desktop`, `extensions`, `outputs`, `startup`, `capture`). The entire component is restored from the previous valid snapshot (or default during initial load), provenance is restored, and the candidate is revalidated.
- `RejectCandidate`: TOML syntax errors, I/O errors, missing structural data, unsupported schema version (`version != 1`), cross-component failures, or candidates failing revalidation after scoped recovery. On reload, the previous valid snapshot is retained unchanged and an empty `ConfigChangeSet` is emitted. On initial load, an error is returned.

### 4. Provenance Tracking

Every effective value in `ConfigSnapshot` records its winning `SourceLocation`:
- `source`: `Defaults`, `Primary { path }`, `Fragment { path }`, or `Overrides { path }`.
- `line` and `column`: 1-based source position extracted from TOML key/value syntax spans when available.
- `EffectiveWithOriginsReport` formats effective values alongside their winning provenance for auditability.

### 5. Syntax-Tree Merging vs Partial Structs

We perform deep-merging at the TOML document syntax-tree level (`toml_edit::DocumentMut` / `Item`) before typed deserialization into `ShellConfig`.

**Rationale**: Maintaining a parallel hierarchy of `PartialShellConfig`, `PartialThemeConfig`, `PartialBarConfig`, etc. with `Option<T>` fields across dozens of structs creates extreme boilerplate, risks missing default attributes, and loses exact source span/location information. Syntax-tree merging operates directly on the parse tree, preserving spans and line numbers while reusing the single canonical `ShellConfig::validate()` definition.

### 6. Downstream Ownership Boundaries

This decision establishes the Phase 1 configuration foundation. Future tickets own specific downstream behaviors:
- **#75**: Levenshtein unknown-key diagnostics and typo suggestions.
- **#77**: Schema version migrations and configuration backup machinery.
- **#80**: Filesystem watching, debouncing, and automatic reload triggers.
- **#113**: Comment-preserving configuration edits in Settings app (`overrides.toml`).
- **#128**: Public CLI UX for `shilpo config effective --origins`.

## Consequences

- Configuration loading is transactional, deterministic, and resilient against partial edit errors.
- Every effective leaf has clear, queryable source provenance.
- Shell startup and reloads retain the last valid configuration on invalid candidate edits.
- First-run creation writes canonical `config.toml` only when missing, and leaves existing blank or hand-authored files untouched.
