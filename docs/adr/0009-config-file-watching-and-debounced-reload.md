# ADR-0009: Declarative Configuration File Watching and Debounced Reload

- **Status**: Accepted

## Context

ADR-0007 established Shilpo's transactional layered configuration architecture (`config.toml`, `conf.d/*.toml`
fragments, and `overrides.toml`), resolved by `ConfigResolver` into immutable `ConfigSnapshot` instances with provenance
and scoped recovery. ADR-0008 specified primary-only schema migrations and established that reload operations must
remain strictly read-only, retaining the last valid committed snapshot on candidate failure.

Previously, configuration reloading occurred only on manual trigger (via `WorkerCommand::ReloadConfig`) or through a
basic un-debounced directory watcher in `ServiceHub`. As user configuration changes occur frequently via text editors
(which employ atomic replacement write sequences) or Settings overrides, Shilpo required a robust, reactive filesystem
watching system that:

1. Watches all declarative configuration sources owned by ADR-0007 (`config.toml`, `overrides.toml`, and immediate
   regular `conf.d/*.toml` files).
2. Classifies paths precisely to ignore editor temporary files, backup copies, non-TOML files, nested subdirectories,
   and operational/session state.
3. Implements a deterministic trailing-edge debounce (100 ms) to coalesce rapid editor save bursts into single reload
   transactions.
4. Shares one serialized, read-only reload transaction across both manual IPC triggers and automatic filesystem watcher
   events.
5. Retains committed snapshots on invalid candidate edits, publishing granular `ConfigChangeSet` flags on success and
   emitting single diagnostic notifications per debounced burst on failure.

## Decision

### 1. Directory-Based Watching and Pure Path Classification

We implement path classification and watcher lifecycle in `desktop/shilpo/src/config/watcher.rs`.

- **Directory Watches**: Rather than watching individual files (which lose watches when text editors perform atomic
  rename-over writes), `ConfigWatcher` watches the main configuration directory (`config_dir`) and `conf.d` (when
  present) using non-recursive directory watches via the workspace `notify` dependency.
- **Dynamic `conf.d` Watch Refresh**: Creation, removal, or recreation of `conf.d` is detected dynamically, updating
  directory watches accordingly.
- **Pure Classification (`classify_path`)**: Filesystem events are filtered before debounce:
    - `config.toml` -> `ClassifiedPath::Primary` (relevant)
    - `overrides.toml` -> `ClassifiedPath::Overrides` (relevant)
    - `conf.d` directory -> `ClassifiedPath::ConfDir` (relevant)
    - Immediate `conf.d/*.toml` regular files -> `ClassifiedPath::Fragment` (relevant)
    - Backup files (`*.bak`, `*.bak.*`), migration temp files (`*.tmp`), hidden files (`.*`), editor swap files (`*~`,
      `*.swp`), non-TOML files (`*.txt`, `*.json`), nested subdirectory files (`conf.d/sub/file.toml`), and
      operational/session files (`session.json`, LMDB stores) -> `ClassifiedPath::Irrelevant` (ignored).

### 2. Trailing-Edge Debounce Semantics (100 ms)

We implement `DebounceStateMachine` driven by explicit `Instant` timestamps:

- **First Relevant Event**: Transitions from `Idle` to `Debouncing` with `deadline = event_time + 100ms`.
- **Rapid Events**: Every subsequent relevant event before expiration advances the deadline to
  `latest_event_time + 100ms`.
- **Deadline Expiration**: Ticking the state machine at or after the deadline transitions state to `Reloading` and
  returns `DebounceAction::TriggerReload { burst_size }`.
- **Events During Reload**: Events arriving while a reload transaction is in progress set `pending_event = true`. Upon
  reload completion (`on_reload_complete`), if `pending_event` is set, a follow-up trailing-edge debounce cycle of 100
  ms is automatically scheduled.
- **Testability**: The state machine contains zero blocking calls or sleeps and is fully testable using injected virtual
  `Instant` values.

### 3. One Shared Read-Only Reload Transaction

Manual IPC commands (`WorkerCommand::ReloadConfig`) and automatic watcher triggers call one shared helper:
`execute_reload_transaction`.

1. **Read-Only Migration Guard**: Evaluates `MigrationService::primary_status()`. Reload is strictly read-only and never
   invokes migrations or modifies disk. Old, future, or invalid primary versions retain the previous snapshot, perform
   zero writes, and emit a diagnostic directing the user to `shilpo config migrate`.
2. **Resolver Invocation**: Calls `ConfigResolver::resolve_reload(&committed_snapshot)` exactly once.
3. **Unknown Key Warnings**: Logs non-fatal #75 unknown-key warnings through `log_unknown_key_warnings`.
4. **Candidate Rejection**: On `RecoveryScope::RejectCandidate`, retains `committed_snapshot`, emits an empty
   `ConfigChangeSet`, logs structured diagnostics, and sends one user-visible `ConfigUpdate::Failed` notification per
   burst.
5. **No-Op Candidates**: If resolution succeeds or recovers but the resulting `ConfigChangeSet` is empty
   (`is_empty() == true`), no redundant `ConfigUpdate::Loaded` update is published and no UI re-render is triggered. A
   `tracing::debug!` event records the no-op reload.
6. **Changed Candidates**: On a non-empty `ConfigChangeSet`, atomically replaces `committed_snapshot` and publishes
   `ConfigUpdate::Loaded { config, changeset }`.

### 4. `ConfigChangeSet` as Granular Publication Contract

`ConfigUpdate::Loaded` carries both the new `Box<ShellConfig>` and the calculated `ConfigChangeSet`. Downstream shell
surfaces (`view.rs`, `service_hub.rs`) react only to component flags they own (e.g. `theme`, `bar`, `desktop`,
`extensions`). Initial worker startup publishes `ConfigUpdate::Loaded` with `ConfigChangeSet::all()`.

### 5. Watcher Ownership and Lifecycle

- `ConfigWatcher` is owned by `service_worker::run` for the worker thread lifetime.
- Non-blocking `notify` callback classifies paths and sends events over a bounded channel. Channel saturation coalesces
  into an atomic pending flag (`take_pending`), preventing lost reloads or thread blocking.
- Initial watcher setup failure is non-fatal: the shell emits a failure notification, retains existing config, and
  preserves manual IPC reload capability.

## Consequences

- Configuration reloads trigger automatically, safely, and deterministically on file edits.
- Complex text editor save operations (atomic temporary writes, renames) produce exactly one debounced reload.
- Corrupted or invalid configuration edits retain the previous valid snapshot without killing the shell daemon.
- Manual and watcher reloads are fully aligned under one read-only transaction semantics.
