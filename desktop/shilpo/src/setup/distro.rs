use std::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distro {
    Arch,
    Unknown,
}

/// Package-manager-backed installation (GPU drivers, desktop packages) only knows how to
/// talk to pacman/paru today, so it's gated on this. An unrecognized distro can still be
/// configured — see `compositor::detect_running` — just not have packages installed for it.
pub fn detect() -> Distro {
    let Ok(os_release) = fs::read_to_string("/etc/os-release") else {
        return Distro::Unknown;
    };
    let is_arch = os_release
        .lines()
        .any(|line| line == "ID=arch" || (line.starts_with("ID_LIKE=") && line.contains("arch")));
    if is_arch {
        Distro::Arch
    } else {
        Distro::Unknown
    }
}
