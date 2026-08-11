# Shilpo Device Context (`shilpo-device`)

Presentation-neutral versioned device domain protocol and typed DBus client.

## Internal Submodules

- `protocol`: Versioned device domain data types (`DeviceDomain`, `DomainLifecycle`, `DomainState`, `CommandId`, `CommandOutcome`, `DeviceCommand`).
- `client`: Typed DBus client used by Shell and Settings. Manages DBus client connections, debounced control setters, and typed degraded state when daemon is unavailable.
