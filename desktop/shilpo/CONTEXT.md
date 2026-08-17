# Shilpo Desktop Product Context (`shilpo`)

Consolidated desktop product package. Produces the single installed executable binary target (`shilpo`).

## Internal Submodules

- `shell`: Shell daemon — top bar, workspace overview, notifications, OSD, extension surfaces, action dispatcher,
  animated theme transitions, and `org.shilpo.Shell` / `org.shilpo.Debug` D-Bus control plane
  (see [ADR-0012](../../docs/adr/0012-dbus-shell-control-plane.md), [ADR-0013](../../docs/adr/0013-runtime-debug-control.md), [ADR-0014](../../docs/adr/0014-animated-theme-transitions.md)).
- `settings`: Standalone control panel application for Shilpo configuration and system settings.
- `cli`: Command-line interface dispatcher for subcommands (`shilpo daemon`, `shilpo settings`, `shilpo config`,
  `shilpo theme`, `shilpo doctor`, `shilpo ext`, etc.).
- `config`: TOML configuration loading, schema validation, default resolution, per-output overrides.

