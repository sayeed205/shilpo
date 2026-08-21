use std::fs;

/// `shilpo setup` only knows how to configure Arch Linux today. Other distributions are
/// planned once real distro packaging exists; until then, the package-manager assumptions
/// baked into `gpu`/`services` (pacman, systemd units) would silently do the wrong thing
/// anywhere else, so refuse to proceed rather than guess.
pub fn require_arch() -> Result<(), String> {
    let os_release = fs::read_to_string("/etc/os-release")
        .map_err(|e| format!("could not read /etc/os-release: {e}"))?;

    let is_arch = os_release
        .lines()
        .any(|line| line == "ID=arch" || (line.starts_with("ID_LIKE=") && line.contains("arch")));

    if !is_arch {
        return Err(
            "shilpo setup only supports Arch Linux right now. Other distributions are planned; \
             see docs/installation.md for a manual setup path in the meantime."
                .to_string(),
        );
    }
    Ok(())
}
