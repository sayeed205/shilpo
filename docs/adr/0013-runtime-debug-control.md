# ADR-0013: Runtime Debug Control Interface

- **Status**: Accepted
- **Date**: 2026-08-13
- **Author**: Shilpo Core Team
- **Issue**: [#114](https://github.com/sayeed205/shilpo/issues/114)

---

## Context & Problem Statement

Developers need a mechanism to dynamically inspect and change active `tracing_subscriber::EnvFilter` directives and inject visible test notifications into the Shilpo shell daemon without restarting the process. Prior to this decision, log levels could only be set at startup via `RUST_LOG` or default directives, and test notifications required manual IPC triggers or external desktop notification tools.

---

## Decision

1. **Session Bus Owner & Object Topology**:
   - Retain the existing `org.shilpo.Shell` session-bus owner and `/org/shilpo/Shell` object path.
   - Register `org.shilpo.Debug` as a second interface on `/org/shilpo/Shell` before the shell daemon completes startup.
   - Do not introduce additional bus names, Unix domain sockets, subscriber instances, or processes.

2. **D-Bus Wire Contract**:
   - `SetLogFilter(filter: String) -> ()`
   - `GetLogFilter() -> String`
   - `EmitTestNotification(title: String, body: String) -> ()`

3. **Reloadable Observability Filter**:
   - `shilpo-observability` wraps `tracing_subscriber::EnvFilter` in a `tracing_subscriber::reload::Layer` during process subscriber initialization.
   - Exposes a thread-safe, cloneable `LogFilterController` with `current_filter() -> String` and `set_filter(&str) -> Result<(), FilterError>`.
   - `set_filter` parses directives strictly with `EnvFilter::builder().parse()`. Malformed or empty filter directives leave the active filter unchanged and return `InvalidArgs`.
   - Concurrent updates are serialized under a internal lock so `GetLogFilter` always reflects the active installed directive.

4. **Test Notification Mailbox Routing**:
   - `DebugDbusService` validates input bounds (non-empty title <= 256 bytes, body <= 4096 bytes) and enqueues `ShellCommand::EmitTestNotification { title, body }` into the shell's bounded (128 capacity) `mpsc` mailbox.
   - `ShellRuntime::drain_dbus_commands` drains the command on the GPUI thread and pushes a normal `shilpo_services::Notification` via `ServiceHub::push_notification`.

5. **Security & Scope Boundaries**:
   - Same-user session-bus ownership is the trust boundary.
   - Non-goals: Enabling/disabling Chrome profiling, trace rotation, persistent filter configuration, config schema modifications, or custom authentication wrappers.

---

## Consequences

### Positive
- Runtime inspectability and log directive filtering without restarting the shell.
- Integrated notification testing traversing existing DND, history, and UI toast paths.
- Compile-time checked D-Bus contract with zero additional socket/process overhead.

### Negative
- Dynamic filter reloading is scoped to `tracing_subscriber` layers initialized through `shilpo-observability`.
