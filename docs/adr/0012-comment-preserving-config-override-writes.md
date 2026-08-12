# ADR-0012: Comment-Preserving Configuration Override Writes

## Status

Accepted

## Context

ADR-0009 established Shilpo's layered configuration architecture (defaults -> `config.toml` -> sorted `conf.d/*.toml` fragments -> `overrides.toml`). ADR-0010 restricted schema version migrations to primary `config.toml` and specified that reloads must remain strictly read-only. ADR-0011 implemented directory-based watching and debounced reloading.

To support Settings app configuration edits, Shilpo required a dedicated configuration write service that:

1. Writes exclusively to `overrides.toml` (`ConfigSource::Overrides`).
2. Edits `toml_edit::DocumentMut` syntax trees in place so user comments, whitespace, key ordering, quoted keys, inline tables, and arrays survive replacement.
3. Supports transactional ordered batches of `Set` and `Remove` edits using explicit key path segments (supporting keys containing dots, such as extension IDs).
4. Rejects top-level `version` mutations (which belong only to primary `config.toml`).
5. Validates the complete layered candidate (defaults + primary + fragments + candidate override) through `ConfigResolver` before any filesystem mutation.
6. Employs durable same-directory atomic replacement with concurrency re-verification, permission preservation (`0o600` for new files), flush, file sync, atomic rename, and parent directory sync.
7. Avoids direct reload triggers, relying entirely on ADR-0011's directory watcher to observe the atomic commit.

## Decision

### 1. Dedicated Service and Ownership Boundary

We implement `ConfigOverrideService` and `OverrideEdit` in `desktop/shilpo/src/config/overrides.rs`.

- **Target Ownership**: `ConfigOverrideService` reads and mutates only `overrides.toml`. Primary `config.toml`, `conf.d/*.toml` fragments, operational session state (`session.json`), LMDB stores, and extension state are never written by this service.
- **In-Place `toml_edit` AST Edits**: Overrides are edited on `toml_edit::DocumentMut` syntax trees. We do not deserialize or reserialize through `ShellConfig` or standard `toml` serializers.

### 2. Path Semantics and Key Segments

- **Segment Representation**: Paths are represented as `Vec<String>` key segments (for example `["extensions", "settings", "org.example.clock"]`).
- **Dot-Containing Keys**: A single segment containing dots is treated as one quoted key in TOML syntax rather than split into sub-tables.
- **Path Validation**: Empty paths and empty segments are rejected as `OverrideError::InvalidPath`.
- **Primary Version Guard**: Any `Set` or `Remove` targeting top-level `version` is rejected with `OverrideError::VersionForbidden`.
- **Traversal Conflicts**: Navigating through existing scalars, arrays, or inline tables when a table is expected returns `OverrideError::TraversalConflict`.

### 3. Parse–Modify–Render Behavior

- **Decoration Preservation**: When replacing an existing leaf value, its surrounding `Decor` (prefix whitespace/comments and inline trailing comments) is copied to the new `toml_edit::Value`.
- **Pruning Policy**: `Remove` removes only the addressed leaf key. Existing empty tables or header comments are preserved.
- **No-Op Detection**: If the rendered TOML output matches the on-disk text byte-for-byte (e.g. setting an identical value or removing an absent key), `OverrideOutcome { changed: false, .. }` is returned and zero filesystem writes are performed.

### 4. Layered Validation Before Write

We implement `ConfigResolver::resolve_candidate_with_overrides(overrides_toml)` in `resolver.rs`.

- **Injection Hook**: Resolves defaults, real primary, current sorted `conf.d/*.toml` fragments, and the in-memory override candidate without merging the on-disk `overrides.toml`.
- **Strict Validation**: The candidate must pass `ShellConfig::validate()`. Candidate validation failures block the write completely and perform zero filesystem writes. Scoped recovery is never applied to write candidates.
- **Unknown Keys**: #75 unknown-key warnings are non-fatal and returned in `OverrideOutcome::warnings`. Unknown keys are byte-preserved in user files.

### 5. Atomicity, Durability, and Concurrency

- **Concurrency Re-Verification**: Before committing, `overrides_unchanged` verifies that the on-disk file bytes and metadata match what was read at transaction start. If modified externally, `OverrideError::ConcurrentModification` is returned.
- **Temporary Sibling Naming**: Temp files use same-directory names formatted as `overrides.tmp.<pid>.<uuid>` (never ending in `.toml`), ensuring ADR-0011's watcher ignores incomplete temp writes.
- **Permissions**: Existing Unix permission bits are preserved; newly created files use `0o600`.
- **Commit Sequence**:
  1. Write bytes to temporary sibling.
  2. `flush()` and `sync_all()`.
  3. Atomic `fs::rename` over `overrides.toml`.
  4. Parent directory `sync_all()` on Unix.
- **Durability Errors**: Pre-rename failures clean up the temporary file. If parent directory sync fails after rename, `OverrideError::DurabilityFailed` is reported.

### 6. Hot-Reload Integration

The override writer does not call `execute_reload_transaction`, mutate the worker's snapshot, or emit `ConfigUpdate`. ADR-0011's directory watcher detects the atomic replacement of `overrides.toml` and schedules a debounced reload.

## Consequences

- Settings app edits preserve user comments, formatting, key ordering, and inline comments in `overrides.toml`.
- Layered validation guarantees invalid Settings edits cannot be written to disk.
- Atomic commit semantics prevent corrupt or partial override files during crashes or power loss.
- Filesystem watching seamlessly triggers reactive desktop reloads upon override commit.
