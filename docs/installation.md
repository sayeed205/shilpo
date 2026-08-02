# Installing Shilpo on Arch Linux

The installer targets a pure Arch Linux installation. Run it as your normal desktop user; it uses `sudo` or `doas` only
for system package and service operations.

```bash
./setup install
```

Supported commands are:

```text
./setup install [-y|--dry-run]
./setup update  [-y|--dry-run]
./setup doctor
./setup uninstall
```

`--dry-run` performs preflight checks and renders the planned files without building or changing the live system. `-y`
suppresses package-manager and reboot prompts; it never reboots automatically.

## What install configures

The installer installs the Rust/WASM build toolchain and the Shilpo desktop runtime: Niri, Kitty, Fish/Starship, fonts,
cursor and icon themes, Nautilus and GVfs, PipeWire/WirePlumber, NetworkManager, Bluetooth, portals, awww, locking/idle
tools, screenshots, and the required GPU/Vulkan packages.

It detects Intel, AMD, and Turing-or-newer NVIDIA hardware. Unsupported NVIDIA hardware stops before package
installation. Paru is bootstrapped from the official Arch User Repository when it is missing.

Shilpo-owned configuration is authoritative: Niri, Kitty, Starship, Fish, Swaylock, Swayidle, Shilpo configuration, user
units, and the default wallpaper are installed from `data/`. Existing display-manager selection is preserved; SDDM is
enabled only when no display manager is configured. The installer does not remove system packages or overwrite unrelated
user data.

The Niri session owns the Shilpo shell, theme daemon, wallpaper daemon, NetworkManager secret agent, GNOME Keyring,
Polkit agent, idle manager, and first-login diagnostics through `niri.service.wants`. NetworkManager and Bluetooth
system services are enabled for the first boot.

The bundled weather extension is built, packed, installed, approved, and enabled as part of install/update. A failure in
any of those steps fails the installer instead of producing a partial “ready” result.

After an install from a TTY, reboot and choose Niri in the existing display manager. The first graphical login runs one
doctor report and writes reports under `$XDG_STATE_HOME/shilpo` (or `~/.local/state/shilpo`).

## Configuration and updates

`./setup update` rebuilds and reapplies the authoritative Shilpo files. Keep personal Niri additions in
`~/.config/niri/config.d/90-user-extra.kdl`; that file is supplied as the user-owned extension point. Shilpo
configuration is at
`$XDG_CONFIG_HOME/shilpo/config.toml` (or `~/.config/shilpo/config.toml`).

`./setup doctor` reports compositor, systemd session links, D-Bus activation, desktop services, GPU/Vulkan, wallpaper
configuration, keybindings, weather, fonts/cursors, and XDG media directories. It does not silently repair a broken
installation.

`./setup uninstall` removes Shilpo binaries, D-Bus activation, user units, and Niri wants links. It preserves installed
packages, display-manager choice, user configuration, wallpapers, extension data, and the Fish login shell.
