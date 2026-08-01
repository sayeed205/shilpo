# Shilpo Shell (`shilpo-shell`)

`shilpo-shell` is a modern, high-performance desktop shell built on [GPUI](https://github.com/zed-industries/zed) and
inspired by **Material Design 3 (M3 Expressive / Material You)** design systems, specifically tailored for Wayland
compositors like [Niri](https://github.com/YaLteR/niri).

---

## 1. Overview & Architecture

The shell is structured into modular layers:

- **`shilpo-ui`**: Core desktop UI component library.
- **`shilpo-services`**: Background system services (Compositor IPC, Brightness, Audio, Application Scanner,
  Notifications, System Tray).
- **`shilpo-config`**: Versioned TOML configuration & session state persistence.
- **`shilpo-shell`**: Desktop bar, workspace overview, control center, and notification center runtime.

---

## 2. Requirements & Environment

### Target Platform Statement

- **Desktop Shell (`shilpo-shell`)**: Currently **Linux / Niri-only** as the primary Wayland compositor target. Support
  for additional compositors like **Hyprland** is planned for future releases. On non-Linux platforms or when Wayland
  compositor IPC is unavailable, the shell operates in graceful offline fallback mode.
- **Application & UI Crates (`shilpo-ui`, `apps/storybook`)**: Built to be **cross-platform** (Linux, macOS, Windows).

- **OS**: Linux (for shell runtime); Cross-platform for UI/Storybook apps
- **Compositor**: Wayland (`Niri 0.1.0+` currently supported; Hyprland planned)
- **Runtime Dependencies**: `DBus`, `PipeWire` / `WirePlumber`, `backlight` / `sysfs`
- **Rust Toolchain**: `1.85+` (Edition 2024)

---

## 3. Building & Running

### Building the Crate

```bash
cargo build -p shilpo-shell --release
```

### Running the Shell

```bash
cargo run -p shilpo-shell
```

---

## 4. Configuration Schema (`config.toml`)

The configuration file is loaded from `$XDG_CONFIG_HOME/shilpo/config.toml` (or `~/.config/shilpo/config.toml`).

```toml
[bar]
position = "Top" # Top | Bottom | Left | Right
style = "FloatingCapsule" # FloatingCapsule | FullEdge
height = 48
opacity = 0.95
corner_radius = 16.0

[theme]
mode = "Dark" # Light | Dark | System
accent_color = "#006C4C"
high_contrast = false
reduced_motion = false
corner_radius_scale = 1.0

[locale]
locale = "en-US" # en-US | bn-IN | ar-SA
```

---

## 5. IPC Protocol & Security Model

`shilpo-shell` exposes a secure UNIX domain socket at `$XDG_RUNTIME_DIR/shilpo-shell/ipc.sock` (mode `0600`).

### Available Commands

- `ToggleOverview`: Toggles the workspace overview & spotlight search surface.
- `ToggleControlCenter`: Toggles the control center overlay.
- `ToggleNotifications`: Toggles the notification panel.
- `ToggleNightLight`: Toggles night-light mode.
- `SetDnd { enabled: bool }`: Updates Do Not Disturb mode.

---

## 6. Systemd User Service & Development Handoff

### Installation & Enablement

Install the provided user unit file to `~/.config/systemd/user/shilpo-shell.service`:

```bash
mkdir -p ~/.config/systemd/user/
cp data/systemd/user/shilpo-shell.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now shilpo-shell
```

### Viewing Logs

```bash
journalctl --user -u shilpo-shell -f
```

### Guarded Development Handoff

To test changes without conflicting with an active release service:

```bash
./scripts/dev-shell
```

This helper detects if `shilpo-shell.service` is running, gracefully stops it, waits for the IPC socket and lock to
release, runs `cargo run -p shilpo-shell`, and restores the release service automatically upon exit.

---

## 7. Manual Cutover Smoke Test

Before disabling fallback/inir modes, execute the following manual cutover smoke test:

1. **Bar Startup**: Enable and start `shilpo-shell.service` via `systemctl --user start shilpo-shell`. Verify the bar
   renders on the primary monitor.
2. **Overview Launch**: Trigger `shilpo msg toggle-overview` or press shortcut. Confirm search input focus and rapid
   query response.
3. **Desktop Services Loss & Recovery**: Disconnect NetworkManager or PipeWire daemon; confirm UI controls transition to
   reconnecting/unavailable state without repeated toast popups. Restart services and confirm automatic recovery.
4. **Notifications**: Send test notification via `notify-send "Test" "Body"`. Verify toast display, DND toggle
   preservation, and signal cleanup.
5. **Service Crash Restart**: Kill `shilpo-shell` with `kill -9 <pid>`; verify systemd automatically restarts the
   service after 2 seconds.
6. **Development Handoff**: Run `./scripts/dev-shell`. Verify release unit is paused, dev binary runs, and release unit
   restores cleanly upon closing.

---

## 8. Troubleshooting & Logs

- **Socket Errors**: Verify `$XDG_RUNTIME_DIR` ownership and permissions (`chmod 0700 $XDG_RUNTIME_DIR/shilpo-shell`).
- **Compositor Communication**: Check Niri socket connection at `$NIRI_SOCKET`.
