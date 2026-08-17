# ADR-0008: Primary-Only Ordered Configuration Version Migrations

- **Status**: Accepted

## Context

ADR-0007 established transactional layered configuration: defaults -> `config.toml` -> ordered
`conf.d/*.toml` fragments -> `overrides.toml`, resolved into a single validated `ShellConfig`
candidate with provenance and scoped recovery. ADR-0007 delegated schema version migrations and
configuration backup machinery to #77.

`ShellConfig` already requires a `version` field that validation restricts to `1`. Before #77 there
was no supported path from older documents to the current schema: an unversioned primary resolved
only because defaults supply `version = 1`, while any explicit version other than 1 failed with
`RejectCandidate`. We needed a safe, ordered pipeline that:

1. Migrates only the canonical hand-authored `config.toml` (the primary) to the latest schema,
   preserving comments, whitespace, ordering, quoted keys, arrays, and inline tables.
2. Never performs partial, unvalidated, or non-atomic writes.
3. Never rewrites `conf.d`, `overrides.toml`, session state, LMDB data, extension state, or caches.
4. Runs automatically at shell startup and on demand via `shilpo config migrate [--dry-run]`.

There is no historical pre-v1 declarative schema with a trustworthy field rename in repository
history. Inventing one would fabricate product semantics; the real initial migration only
establishes the supported legacy boundary.

## Decision

### 1. Version semantics and the v0 -> v1 boundary

One exported constant, `LATEST_CONFIG_VERSION = 1` in `desktop/shilpo/src/config/migration.rs`, is
the single authoritative latest version, used by `ShellConfig::default`, validation, migration
planning, diagnostics, and tests.

- A missing top-level `version` in a non-empty primary means legacy schema 0.
- An explicit integer `version = 0` also means schema 0.
- `version = 1` is current and requires no migration.
- Negative, non-integer, float, and out-of-range values are invalid version diagnostics.
- A version greater than `LATEST_CONFIG_VERSION` is a hard `FutureVersion` error; it is never
  downgraded, and startup never silently falls back to defaults over the source.
- A version older than the oldest registered migration is `UnsupportedOldVersion`.
- Missing or empty primaries retain ADR-0007 behavior: missing files may be created on first run,
  empty hand-authored files are never rewritten.

`version` is metadata, not an ordinary recoverable leaf: `classify_diagnostic` already maps the
`version` path to `RejectCandidate`, never `RejectValue` or `RetainPreviousComponent`.

### 2. Ordered migration registry

`MigrationRegistry` holds an ordered, immutable set of steps (`Migration { from, to, name, apply }`,
where `apply: fn(&mut DocumentMut) -> Result<(), String>`). Construction validates the invariants
before anything may plan or execute:

- at most one step begins at each old version (no duplicates);
- every step advances by exactly one version (`to == from + 1`);
- steps are contiguous (no gaps) and terminate exactly at `LATEST_CONFIG_VERSION`;
- after each executed step the document's top-level `version` must equal the step's `to`.

The pipeline builds the complete plan before applying any step, runs every step in order against an
in-memory `toml_edit::DocumentMut`, and stops on the first failure. A partially migrated document is
never written.

The real `v0 -> v1` step only inserts or replaces the top-level `version = 1`. A newly inserted
`version` is serialized by `toml_edit` before the first table header (after any existing root scalar
keys); an explicit `version = 0` is replaced in place with decorations preserved. Everything else in
the document keeps its byte-level formatting. Multi-step ordering, renaming, and decoration
preservation are proven with test-only synthetic registries; no fake production rename exists.

### 3. Principal ownership boundary

Only the primary `config.toml` is migration-owned. A `version` key found in a fragment or override
is rejected with a source-specific `InvalidSourceVersion` diagnostic. `conf.d/`, `overrides.toml`,
`ShellSessionState`, LMDB, extension state, and caches are never read as migration inputs or written
by the migration service.

### 4. Validate before write

One `MigrationService` (explicit primary path, mode `Preview` or `Apply`) serves both shell startup
and the CLI, so tests never touch process-global XDG variables. Its order is:

1. Read primary bytes and metadata.
2. Parse with `toml_edit`; detect the original version.
3. Build the complete ordered plan.
4. Apply every step in memory and render the migrated TOML.
5. Validate the migrated in-memory primary through the resolver's narrow injection hook
   (`ConfigResolver::resolve_candidate_with_primary`): the normal #75 unknown-key scan plus the
   #74 complete layered resolution with the current `conf.d` and `overrides.toml`. The real primary
   is never temporarily overwritten to validate.
6. Unknown-key diagnostics are non-fatal #75 warnings carried in the outcome.
7. Parse/type/semantic errors and invalid fragments/overrides block the migration as
   `CandidateValidation`; no backup or write occurs.
8. Before committing, verify the original primary has not changed since it was read (exact bytes
   plus stable metadata where available); a detected change is `ConcurrentModification`.
9. `Preview` stops here with zero filesystem writes. `Apply` commits.

### 5. Backup, atomic replacement, and durability

- Backup name: sibling `config.toml.bak.<UTC timestamp>` with subsecond precision;
  create-new semantics, deterministic numeric suffix on collision, an existing backup is never
  overwritten.
- Backup contents exactly equal the original primary bytes; written, flushed, and `sync_all`'d.
- Temporary replacement uses create-new names containing the process ID plus a collision-resistant
  UUID suffix, in the same directory, preserving the primary's Unix permission bits where available.
- The rename over `config.toml` is atomic and same-directory; the parent directory is `sync_all`'d
  afterwards on Unix. The rename is the commit point.
- Backup or temp-write failures leave the primary untouched; a failed rename leaves the original
  primary and backup intact and removes only the known temporary file. Cleanup never uses broad
  globs.

### 6. Startup auto-migration versus read-only reload

- Shell startup runs the service in `Apply` mode before initial layered resolution. Missing/empty
  primaries produce a `Current` outcome without writes (preserving first-run creation). An applied
  migration logs one structured `tracing::info!` with original/final versions, steps, primary path,
  and backup path. Any migration error is fatal to configuration startup: it is logged clearly and
  the existing degraded/default policy applies without rewriting or downgrading the source.
- Manual reload is strictly read-only: `primary_status()` inspects the version and, when migration
  is required (or the version is invalid/future), the reload is rejected with a diagnostic pointing
  to `shilpo config migrate`, retaining the previous committed snapshot. Reload never mutates files
  and never auto-migrates.

### 7. CLI contract

`shilpo config migrate [--dry-run]` calls the same service as startup. Default mode applies;
`--dry-run` previews with zero writes. Current config exits success with `changed: false`; needing
migration exits success with `changed: true`; errors exit `EXIT_FAILURE` with a stable
`config.migration.*` code. Human dry-run output includes original/final versions, ordered step
names, and the clearly delimited complete migrated TOML. JSON output is the serialized
`MigrationOutcome`: `path`, `mode`, `changed`, `from_version`, `to_version`, `steps`,
`backup_path` (null unless applied), `warnings`, and `migrated_toml` (dry-run only).

### 8. Why no fake production field rename

Repository history contains no pre-v1 declarative schema with a trustworthy field rename. The
initial migration therefore only establishes the `version = 1` boundary. Renaming and multi-step
composition are exercised exclusively by test-only synthetic migrations.

## Consequences

- Older supported primaries converge to the current schema before every resolution, with a durable
  byte-exact backup and atomic replacement.
- Migration can never clobber concurrent edits or accept a candidate that fails complete layered
  validation.
- User formatting, comments, and ordering survive migration; only the version line changes.
- Unknown keys remain warnings and stay in the user file.
- Future work: #80 owns file-watch triggers; #113 owns comment-preserving Settings writes to
  `overrides.toml`; #128 owns expanded `config validate/effective` CLI UX. Schema version 2 or any
  real product rename is out of scope for this ADR and will require its own ticket.