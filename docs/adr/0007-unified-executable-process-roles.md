# Unified executable with explicit process roles

Shilpo ships one desktop executable, `shilpo`, but uses separate operating-system processes for failure isolation.
Packaging and process topology are intentionally different decisions.

## Roles

- `shilpo daemon` owns GPUI shell surfaces, shell coordination, and `org.shilpo.Shell`.
- `shilpo settings` is a separate GPUI process.
- `shilpo extension-host` is a private child of the shell and is the only process that constructs Wasmtime.
- `shilpo device-daemon` owns `org.shilpo.Device`.
- `shilpo theme-daemon` owns `org.shilpo.Theme`.
- Other `shilpo` commands are short-lived clients.

Settings and extension execution never share mutable shell memory. A failure in either process must leave shell surfaces
alive.

## Extension-host seam

The shell creates one supervised child and connects to it through piped stdin/stdout. The channel has no filesystem
path, DBus name, TCP port, or independently connectable endpoint. Frames contain a four-byte big-endian length and
UTF-8 JSON payload. Zero-length, malformed, protocol-mismatched, and payloads larger than 8 MiB are rejected. Command
and update queues are bounded at 64 entries.

Every message carries protocol version 1, a monotonically increasing host generation, and a request ID. Extension
engine generations remain distinct and are checked separately. The shell rejects stale host or engine generations before
publishing snapshots or executing effects.

## Supervision

The supervisor states are `Starting`, `Ready`, `Backoff`, `Quarantined`, `Stopping`, and `Stopped`. Unexpected exits
retry after 250 ms, 1 s, and 4 s. Three unexpected exits in a rolling 60-second window enter `Quarantined`. Five
minutes of readiness clears the rolling crash window while retaining the session restart count for diagnostics.

Expected shutdown sends a typed `Shutdown`, waits at most two seconds for acknowledgement and process exit, then closes
the pipe and kills/reaps the child if necessary. Non-idempotent effects are never replayed after restart.

## Activation and singleton ownership

Durable roles acquire their DBus well-known names with `DoNotQueue` before announcing readiness. A duplicate owner exits
non-zero. The shell retains its DBus connection for the lifetime of the process. The child relationship, not a lock file,
is the extension-host singleton authority.

## Rejected alternatives

- One OS process for all roles: a settings, service, or Wasmtime failure would take down shell surfaces.
- A public extension-host socket or DBus service: it would add an unnecessary addressable attack surface.
- A parallel lock-file singleton protocol: DBus already provides authoritative name ownership.
- A generic public supervisor abstraction: process supervision is concrete here; shared service-domain semantics belong to
  the domain-port decision in #137.

## Follow-up boundaries

- #125 completes public `org.shilpo.Shell` methods/signals and removes transitional shell socket/lock code.
- #115 moves extension API/runtime ownership into the dependency-aligned crates.
- #122 consolidates role modules into the target workspace topology.
