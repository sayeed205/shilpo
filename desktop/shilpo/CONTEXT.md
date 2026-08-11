# Shilpo Desktop Product Context (`shilpo`)

Consolidated desktop product package. Produces the single installed executable binary target (`shilpo`).

## Internal Submodules

- `shell`: Shell daemon — top bar, workspace overview, notifications, OSD, extension surfaces, action dispatcher.
- `settings`: Standalone control panel application for Shilpo configuration and system settings.
- `cli`: Command-line interface dispatcher for subcommands (`shilpo daemon`, `shilpo settings`, `shilpo config`, `shilpo theme`, `shilpo doctor`, `shilpo ext`, etc.).
- `config`: TOML configuration loading, schema validation, default resolution, per-output overrides.
