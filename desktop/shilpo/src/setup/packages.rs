use std::process::Command;

use super::compositor::Compositor;
use super::privilege::{is_on_path, run_privileged};

/// Desktop packages Shilpo itself needs, regardless of compositor: session services,
/// fonts/icons/cursors, and the small CLI tools Shilpo's own binds/config shell out to
/// (kitty, nautilus, swaylock, swayidle, ...). Compiled from the earlier comparison against
/// iNiR's own dependency list, trimmed to what Shilpo actually uses today (e.g. no Qt/KDE
/// theming stack — GPUI needs none of it; no cliphist/dunst — Shilpo has no clipboard
/// history yet and owns notifications natively).
const COMMON_PACKAGES: &[&str] = &[
    // Session services
    "systemd",
    "dbus",
    "networkmanager",
    "pipewire",
    "pipewire-pulse",
    "pipewire-alsa",
    "wireplumber",
    "bluez",
    "bluez-utils",
    "brightnessctl",
    "power-profiles-daemon",
    "upower",
    "geoclue",
    "xdg-desktop-portal",
    "xdg-desktop-portal-gtk",
    "xdg-utils",
    "xdg-user-dirs",
    "awww",
    "librsvg",
    "gtk3",
    "libnotify",
    "polkit",
    "vulkan-tools",
    // Fonts / icons / cursors
    "noto-fonts",
    "noto-fonts-emoji",
    "ttf-jetbrains-mono-nerd",
    "capitaine-cursors",
    "breeze-icons",
    "hicolor-icon-theme",
    "adwaita-icon-theme",
    "papirus-icon-theme",
    // Desktop tools
    "linux-firmware",
    "sof-firmware",
    "alsa-ucm-conf",
    "pciutils",
    "usbutils",
    "fish",
    "starship",
    "kitty",
    "nautilus",
    "gvfs",
    "gvfs-mtp",
    "polkit-gnome",
    "gnome-keyring",
    "network-manager-applet",
    "playerctl",
    "xwayland-satellite",
    "swaylock",
    "swayidle",
    "pavucontrol",
    "wlsunset",
    "sddm",
];

pub fn install_for(compositor: Compositor) -> Result<(), String> {
    ensure_paru()?;

    let mut packages: Vec<&str> = COMMON_PACKAGES.to_vec();
    packages.extend(compositor.extra_packages());
    packages.sort_unstable();
    packages.dedup();

    println!(
        "\nPackages needed for {compositor}: {}\n",
        packages.join(", ")
    );
    let proceed = dialoguer::Confirm::new()
        .with_prompt(format!(
            "Install these {} packages now (paru -S --needed)?",
            packages.len()
        ))
        .default(true)
        .interact()
        .map_err(|e| e.to_string())?;

    if !proceed {
        println!("Skipped package installation.\n");
        return Ok(());
    }

    if unsafe { libc::geteuid() } == 0 {
        return Err("paru refuses to run as root; run shilpo setup as a normal user".to_string());
    }
    let mut argv: Vec<&str> = vec!["paru", "-S", "--needed", "--noconfirm"];
    argv.extend(packages);
    let status = Command::new(argv[0])
        .args(&argv[1..])
        .status()
        .map_err(|e| format!("failed to run paru: {e}"))?;
    if !status.success() {
        return Err("paru exited with a non-zero status".to_string());
    }
    println!();
    Ok(())
}

fn ensure_paru() -> Result<(), String> {
    if is_on_path("paru") {
        return Ok(());
    }

    println!("Bootstrapping paru (AUR helper)...");
    run_privileged(&[
        "pacman",
        "-Syu",
        "--needed",
        "--noconfirm",
        "base-devel",
        "git",
    ])?;

    let tmp = std::env::temp_dir().join(format!("shilpo-setup-paru-{}", std::process::id()));
    let status = Command::new("git")
        .args([
            "clone",
            "https://aur.archlinux.org/paru.git",
            &tmp.to_string_lossy(),
        ])
        .status()
        .map_err(|e| format!("failed to clone paru: {e}"))?;
    if !status.success() {
        return Err("git clone of paru failed".to_string());
    }

    let status = Command::new("makepkg")
        .args(["-si", "--noconfirm"])
        .current_dir(&tmp)
        .status()
        .map_err(|e| format!("failed to run makepkg: {e}"))?;
    let _ = std::fs::remove_dir_all(&tmp);

    if !status.success() || !is_on_path("paru") {
        return Err("paru build/install failed".to_string());
    }
    Ok(())
}
