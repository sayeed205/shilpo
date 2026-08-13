# Canonical WIT Extension Contract and Process Execution Removal

**Status**: Accepted

## Context

Prior extension contracts in Shilpo relied on generic JSON-encoded string payloads and a generic `process:exec` host
capability. This created security and design risks:

1. Untyped host boundaries (`event-json`, string-serialized JSON trees) bypassed compile-time component validation.
2. `process:exec` allowed extensions to invoke arbitrary host binaries, defeating Wasmtime capability-based sandboxing
   and security guarantees.

## Decision

1. **Canonical WIT Contract (`shilpo:extension@0.3.0`)**:
    - The WASM component boundary is defined exclusively by typed WebAssembly Interface Type (WIT) interfaces in
      `core/ext-api/wit/extension.wit`.
    - Manifest schema version is bumped to `2`, and API version is bumped to `0.3.0`.
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

4. **Internal IPC Boundary Topology**:
    - Per [ADR-0007](0007-unified-executable-process-roles.md), the private Shell ↔ Extension-Host worker IPC channel
      remains a versioned, length-framed JSON protocol.
    - Zero JSON-over-string applies strictly to the sandboxed WASM component boundary inside `ext-runtime`.

5. **Capability Enforcement**:
    - Capability authorization checks, runtime resource limits (fuel, memory, deadline), circuit-breaker behavior, and
      generation validation remain strictly enforced at the host level before executing host operations.

## Consequences & Follow-ups

- Extensions must compile against `shilpo:extension@0.1.0` WIT definitions.
- Host capability stubs (`secrets`, `theme`, `wallpaper`, `clipboard` read, `filesystem` write) return typed
  `error-kind::unsupported` errors until full service implementations land.
- Follow-up tracker issues: #78 (TypeScript SDK WIT bindgen), #92 (Rust SDK WIT bindgen), #96 (hot-reload), #100 (CLI
  scaffolding), #131 (benchmark suite), #138 (service trait integration).
