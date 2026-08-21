# Installing Shilpo

`./setup install` and `shilpo setup` are local build/dev tools, not a polished end-user installer yet — real
distribution is planned via distro packaging (AUR, etc.), where package installation is declared as a dependency
instead of scripted here.

## 1. Build and install the binary

```bash
./setup install
```

This builds `shilpo` in release mode (`cargo build --locked --release -p shilpo`) and installs the binary to
`~/.local/bin/shilpo` (or `/usr/local/bin/shilpo` when run as root). Pass `--prefix DIR` to install elsewhere. Assumes
the Rust toolchain and GPUI's build dependencies are already present on your machine.

```text
./setup install [--prefix DIR]
./setup uninstall [--prefix DIR]
```

## 2. Configure your session

```bash
shilpo setup
```

An interactive wizard that turns a bare `shilpo` install into a working desktop session:

1. Detects the distro. On Arch Linux, you're asked which compositor to configure (**Niri** or **Hyprland** today; Sway
   is listed as coming soon). On an unrecognized distro, it instead detects an already-running Niri/Hyprland session
   and configures for that, skipping package installation.
2. On Arch, installs the desktop packages that compositor needs (bootstrapping `paru` if missing) and detects/installs
   GPU drivers (Intel/AMD/NVIDIA Turing-or-newer).
3. Stages Shilpo's recommended configuration for the chosen compositor — Niri gets `~/.config/niri/`; Hyprland gets
   `~/.config/hypr/hyprland.lua` (Hyprland deprecated its classic `.conf` format in 0.55 in favor of native Lua
   config) — plus the shared Kitty/Fish/Starship/Swaylock/Swayidle/Shilpo configuration and default wallpaper.
4. Wires up the session: all of Shilpo's daemons and helpers are systemd user units grouped under
   `shilpo-session.target`, wants-linked so `systemctl --user daemon-reload` picks them up. Each compositor's own
   staged config starts that one target with a single `systemctl --user start shilpo-session.target` call — this is
   what makes crash recovery and `journalctl`/`systemctl status` observability work the same way regardless of
   compositor, instead of depending on niri's own systemd-session integration. Also enables NetworkManager/Bluetooth,
   SDDM if no display manager is already configured, and switches the login shell to Fish.

It ends by offering to reboot. Run it again any time to re-apply or re-check your configuration. A user-owned
extension point is preserved on repeat runs: `niri/config.d/90-user-extra.kdl` for Niri,
`hypr/shilpo-user-extra.lua` for Hyprland.
