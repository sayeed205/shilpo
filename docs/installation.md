# Installing Shilpo

```bash
./setup install
```

This builds `shilpo` in release mode (`cargo build --locked --release -p shilpo`) and installs the binary to
`~/.local/bin/shilpo` (or `/usr/local/bin/shilpo` when run as root). Pass `--prefix DIR` to install elsewhere.

```text
./setup install [--prefix DIR]
./setup uninstall [--prefix DIR]
```

`./setup uninstall` removes the installed binary. It does not touch configuration, extension data, or system
packages/services — none of those are managed by `./setup`.

Distro packages (AUR, etc.) and system/session integration (Niri config, systemd units, D-Bus activation) are planned
separately; this script only builds and installs the `shilpo` binary itself so the `shilpo` command is available.
