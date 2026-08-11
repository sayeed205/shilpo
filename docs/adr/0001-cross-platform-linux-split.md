# Cross-platform / Linux-only workspace split

Shilpo's workspace is split into two tiers by platform: `core/` for cross-platform crates that will be published to
crates.io, and `desktop/` for Linux-only crates that form the desktop shell environment. The split exists because Shilpo
started as a UI component library and evolved into a full desktop environment — the UI library needs to remain
independent and cross-platform for third-party GPUI apps and future Shilpo cross-platform apps, while the shell
ecosystem is inherently Linux-specific (Wayland, DBus, XDG, Niri).

Cross-platform crates (`core/`): `shilpo-ui`, `shilpo-theme`, `shilpo-macros`, `shilpo-ext-api`. Linux-only crates
(`desktop/`): `shilpo`, `shilpo-device`, `shilpo-services`, `shilpo-ext-runtime`, `shilpo-theme-daemon`.

`core/` crates must never depend on `desktop/` crates. `desktop/` crates may depend on `core/` crates freely.
