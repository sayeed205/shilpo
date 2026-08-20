# ADR-0002: Theme Crate Split into Cross-Platform Core and Linux Daemon

- **Status**: Accepted

`shilpo-theme` is split into two crates: a cross-platform core (`shilpo-theme`, now living in
[shilpo-rs/ui](https://github.com/shilpo-rs/ui) alongside `shilpo-m3e`, which re-exports it — see
[ADR-0001](0001-cross-platform-linux-split.md)) containing M3 color math, scheme generation, and pure data types
(`ThemeMode`, `SchemeVariant`, `ThemeState`, `ThemeCommand`, `reduce()`), and a Linux-specific daemon layer
(`shilpo-theme-daemon` in `desktop/theme-daemon`, staying in this repo) containing the DBus service, XDG portal sync,
wallpaper watching, persistence, and third-party adapters.

The split is necessary because `shilpo-m3e` depends on theme types but must compile cross-platform. The original
`shilpo-theme` unconditionally depends on `zbus` and `ashpd` (Linux D-Bus/portal crates) with no cfg gates, making it a
cross-platform blocker. The separation is clean — `state.rs` (pure computation, zero I/O) becomes the core crate;
everything else (daemon, dbus, portal, client, persistence, adapters) moves to the daemon crate.

Cross-platform apps read the OS wallpaper/accent color and generate M3 schemes on-the-fly using `shilpo-theme`. The
Linux shell uses `shilpo-theme-daemon` to control system-wide theming and persist state.
