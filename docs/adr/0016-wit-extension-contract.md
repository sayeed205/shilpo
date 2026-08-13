# Canonical WIT Extension Contract and Process Execution Removal

**Status**: Accepted

## Context

Prior extension contracts in Shilpo relied on generic JSON-encoded string payloads and a generic `process:exec` host
capability. This created security and design risks:

1. Untyped host boundaries (`event-json`, string-serialized JSON trees) bypassed compile-time component validation.
2. `process:exec` allowed extensions to invoke arbitrary host binaries, defeating Wasmtime capability-based sandboxing
   and security guarantees.

## Decision

1. **Canonical WIT Contract (`shilpo:extension@0.1.0`)**:
    - The WASM component boundary is defined exclusively by typed WebAssembly Interface Type (WIT) interfaces in
      `core/ext-api/wit/extension.wit`.
    - Manifest schema version remains `1`, and API version is `0.1.0`.
    - All guest exports (`activate`, `deactivate`, `on-event`, `view`) and host imports (`actions`, `clipboard`,
      `filesystem`, `http`, `location`, `notifications`, `state`, `secrets`, `theme`, `wallpaper`) are strictly typed
      WIT types. Zero JSON-over-string serialization is permitted across the component boundary.

2. **Total Removal of Generic `process:exec`**:
    - Generic process execution capability (`process:exec`, `ProcessExec`, `ExecProcess`) is permanently deleted from
      manifests, schemas, WIT definitions, runtime authorization, diagnostic systems, fixtures, and shell execution
      paths.
    - No fallback ABI, JSON legacy compatibility layer, or replacement process execution capability is introduced.

3. **Asynchronous Request/Completion Model**:
    - Host capability imports returning asynchronous data (e.g. `http::request`, `location::read`) use an explicit,
      typed `request-id` correlation model.
    - Host operations return immediate `result<_, error>` receipts, and asynchronous completions are delivered back to
      the guest via `on-event(ExtensionEvent::HttpRequestCompleted { .. })` or `LocationReadCompleted { .. }`.

4. **Extension Secret Broker & `SecretRef` Settings Contract**:
    - Secret persistence is provided via `secrets` WIT interface (`set`, `read`, `delete`) backed by `Oo7SecretBroker`
      (Freedesktop Secret Service over DBus via `oo7`).
    - Manifest capability `secrets` requires explicit `purposes` declarations (`["auth-token", "api-key"]`). Host calls
      verify `secrets:<purpose>` capability grant dynamically on every invocation.
    - `SecretRef` handle references are opaque, unique strings. Plaintext secret bytes and secret handles are strictly
      redacted from logs, tracing, worker JSON, TOML config, and LMDB session stores (`SecretRef(<redacted>)`).

5. **Threat Model & Security Guarantees**:
    - **Cross-Extension & Cross-Purpose Isolation**: Secret items in system keyring are indexed with DBus attributes
      `shilpo:app = "shilpo"`, `shilpo:extension_id`, `shilpo:purpose`, and `shilpo:handle`. Extensions cannot read or mutate
      secrets belonging to other extensions or ungranted purposes.
    - **Dynamic Grant Revocation**: Granular permission checks occur on every `secrets` host call. Revoking a purpose grant
      immediately denies subsequent host calls without restarting or reloading the Wasm runtime.
    - **Hermetic Testing**: `FakeSecretBroker` provides in-memory hermetic isolation only when explicitly injected by tests.
    - **Uninstall Secret Lifecycle Policy**: Uninstall defaults to `SecretPolicy::Retain` to prevent accidental loss, with
      an explicit `SecretPolicy::Delete` option to purge all extension secret attributes from Secret Service.

6. **Durable Extension State Store & Reactive Watch Contract**:
    - Extension state is stored in a durable, per-extension-namespaced LMDB environment at `<CatalogPaths.data_dir>/extensions/state.lmdb` managed by `HeedStateStore` in `shilpo-ext-runtime`.
    - Typed WIT interface `state` (`read`, `write`, `delete`, `watch`, `unwatch`) supports synchronous KV operations with atomic watch registration snapshots (`watch-registration { watch-id, snapshot }`) closing read/watch races.
    - Persisted state enforces quotas (max 256 keys, max 64 KiB value, max 4 MiB total) and monotonic per-extension revisions in a single LMDB transaction.
    - `SecretRef` values are rejected on state write/read. State values never cross diagnostic, worker JSON IPC, tracing, or TOML boundaries.
    - Reactive watches deliver ordered `ExtensionEvent::StateValue` events deferred until host calls return, coalescing updates under backpressure.
    - Uninstall supports explicit `StatePolicy::{Retain, Delete}` with rollback-safe artifact staging.

7. **Trusted Local Script Boundary vs WASM Capabilities**:
    - Trusted local scripts (`$XDG_CONFIG_HOME/shilpo/scripts/<bundle>/manifest.toml`) run with the user's OS authority as local-only status bar widgets.
    - WASM guests compiled against `shilpo:extension@0.1.0` WIT contract have zero access to process execution, cannot spawn child processes, and cannot configure, invoke, or influence trusted local script execution.

## Consequences & Follow-ups

- Extensions must compile against `shilpo:extension@0.1.0` WIT definitions.
- Secret Service integration is operational via `Oo7SecretBroker` for `secrets` WIT calls. Runtime initialization fails
  closed when Secret Service is unavailable; there is no production plaintext or in-memory fallback.
- Durable LMDB state persistence and reactive watch subscription contract are operational via `HeedStateStore` for `state` WIT calls.
- Follow-up tracker issues: #78 (state store completed), #92 (ViewTree layout & interaction enhancements), #96 (hot-reload), #100 (CLI scaffolding), and #131 (benchmark suite).
