# ADR-0014: Standardized D-Bus Shell Control Plane

- **Status**: Accepted
- **Date**: 2026-08-12
- **Author**: Shilpo Core Team
- **Issue**: [#125](https://github.com/sayeed205/shilpo/issues/125)

---

## Context & Problem Statement

Prior to this decision, the `shilpo` shell process exposed its control plane via a custom Unix domain socket transport (`ipc.sock`) with manual length-prefixed framing, ad-hoc JSON request/response serialization, and a custom lock file (`instance.lock`).

This transitional transport had several limitations:
1. **Lack of Standard Interoperability**: External desktop components, systemd, desktop environment scripts, and extensions could not standardly discover or introspect shell commands using Linux D-Bus tools (`busctl`, `gdbus`, `d-feet`).
2. **Duplicated Transport Logic**: Custom socket listeners, framing encoders/decoders, and lock-file cleanup logic added maintenance overhead without leveraging standard session bus single-instance semantics.
3. **Imperfect Signal Broadcasts**: Broadcasting lifecycle, theme, workspace, or config events to multiple listeners required custom socket streaming rather than standard D-Bus signal emission.

---

## Decision

We retire the legacy Unix domain socket control plane (`desktop/services/src/ipc/mod.rs`, `ipc.sock`, `instance.lock`, `IpcRequest`, `IpcResponse`, `ShellIpcServer`, `ShellIpcClient`) and replace it with a single, introspectable D-Bus service: `org.shilpo.Shell` owned by the durable shell daemon.

### 1. Bus topology & Singleton Ownership
- **Bus**: User Session Bus (`zbus::Connection::session()`).
- **Well-known Name**: `org.shilpo.Shell` (requested with `RequestNameFlags::DoNotQueue`). If another process owns `org.shilpo.Shell`, startup fails cleanly with exit code 1/2.
- **Object Path**: `/org/shilpo/Shell`
- **Interface**: `org.shilpo.Shell`

### 2. Method Surface & Contract
The D-Bus interface exposes typed methods:
- `ReloadConfig() -> ()`
- `ShowBar() -> ()`
- `HideBar() -> ()`
- `ToggleBar() -> ()`
- `ShowOverview() -> ()`
- `HideOverview() -> ()`
- `ToggleOverview() -> ()`
- `FocusWorkspace(workspace_id: u64) -> CommandResult`
- `CreateWorkspace() -> CommandResult`
- `FocusWindow(window_id: u64) -> CommandResult`
- `FocusPreviousWindow() -> CommandResult`
- `CloseWindow(window_id: u64) -> CommandResult`
- `MoveWindowToWorkspace(window_id: u64, workspace_id: u64) -> CommandResult`
- `SetBrightness(percentage: u8) -> ()`
- `SetDisplayBrightness(display_id: String, percentage: u8) -> ()`
- `GetStatus() -> ShellStatus`
- `GetTelemetry() -> ShellTelemetry`
- `Capture(intent: String) -> ()`
- `InvokeAction(action_id: String, payload_json: Option<String>) -> ()`
- `NextWallpaper() -> ()`
- `StartDevSession(extension_id: String, source_root: String) -> String`
- `ReloadDevSession(session_id: String, build_sequence: u64, artifact_path: String) -> DevReloadResult`
- `EndDevSession(session_id: String) -> ()`

### 3. Wire Types
- **`CommandResult`**: Typed struct preserving compositor terminal outcomes (`Applied`, `ReconciledApplied`, `Rejected`, `TimedOut`, `Cancelled`) with owner generation, revision, and failure reason details.
- **`ShellStatus`**: Real D-Bus struct with status fields (`running`, `instance_id`, `pid`, `readiness`, `bar_state`, `overview_visible`).
- **`ShellTelemetry`**: Typed struct for runtime health & subsystem status.
  - *Narrow Exception*: `extension_host_diagnostics_json: String` contains the serialized diagnostic summary string from the Wasmtime extension host runtime as a documented narrow exception to avoid exporting dynamic host schema bags.
- **`DevReloadResult`**: Wire signature `(sttss)` returning terminal dev reload outcome (`outcome`, `host_generation`, `engine_generation`, `diagnostic_code`, `message`).

### 4. Threading & Command Mailbox
- **Mailbox**: Shell action and brightness commands are queued into a bounded `tokio::sync::mpsc` channel with capacity **128**.
- **Overflow Policy**: If full, method handlers immediately return `org.freedesktop.DBus.Error.LimitsExceeded`. No accepted command is lost.
- **FIFO Execution**: The GPUI main loop drains up to 128 commands per tick in FIFO order.
- **Non-blocking Queries**: `GetStatus` and `GetTelemetry` read atomic snapshots without entering or blocking on the GPUI main loop.

### 5. Signals
The interface defines 5 D-Bus signals:
1. `ShellStarted(instance_id: String, pid: u32)`: Emitted once after object registration.
2. `ShellStopping(instance_id: String)`: Emitted once during orderly shutdown.
3. `WorkspaceChanged(workspace_id: u64, owner_generation: u64, revision: u64)`: Emitted when focused workspace changes; deduplicated across equal snapshots.
4. `ThemeChanged(mode: String, scheme_variant: String)`: Emitted when theme mode or scheme variant changes; deduplicated across equal values. Initial population is not a change signal.
5. `ConfigReloaded(success: bool, changed_components: Vec<String>, diagnostic_count: u32)`: Emitted after reload completion with sorted component list.

---

## Consequences

### Positive
- Fully standard Linux D-Bus integration inspectable via `busctl --user introspect org.shilpo.Shell /org/shilpo/Shell`.
- Zero custom socket framing or manual lock file management code.
- High performance non-blocking query execution and bounded mailbox safety.
- Reliable CLI exit codes mapped cleanly from D-Bus errors.

### Negative
- Inter-process shell commands require an active user D-Bus session daemon (standard on all modern Linux desktop environments).
