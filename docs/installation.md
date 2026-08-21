# Installing Shilpo

## 1. Build and install the binary

```bash
./setup install
```

This builds `shilpo` in release mode (`cargo build --locked --release -p shilpo`) and installs the binary to
`~/.local/bin/shilpo` (or `/usr/local/bin/shilpo` when run as root). Pass `--prefix DIR` to install elsewhere.

```text
./setup install [--prefix DIR]
./setup uninstall [--prefix DIR]
```

`./setup uninstall` removes the installed binary only. Distro packages (AUR, etc.) are planned separately; until then
this is the only way to get the `shilpo` command onto your machine.

## 2. Configure your session

```bash
shilpo setup
```

An interactive, Arch Linux-only wizard that turns a bare `shilpo` install into a working desktop session: choose a
compositor (Niri today; others are listed as coming soon), stage its recommended Shilpo configuration, detect and
install GPU drivers, wire up the Shilpo-owned systemd user units, and enable NetworkManager/Bluetooth. It ends by
offering to reboot. Run it again any time to re-apply or re-check your configuration.

It does not install compositor/desktop packages (Niri, Kitty, Fish, PipeWire, etc.) — those are expected to already be
present (either installed manually today, or via a future AUR package's dependencies).
