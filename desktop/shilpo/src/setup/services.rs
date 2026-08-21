use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::SetupAssets;
use super::privilege::{is_on_path, run_privileged};

const UNITS: &[&str] = &[
    "shilpo-shell.service",
    "shilpo-themed.service",
    "shilpo-device-daemon.service",
    "shilpo-wallpaper.service",
    "shilpo-swayidle.service",
    "shilpo-network-agent.service",
    "shilpo-keyring.service",
    "shilpo-polkit-agent.service",
    "shilpo-first-login.service",
];

const DISPLAY_MANAGERS: &[&str] = &["sddm", "gdm", "lightdm", "ly"];

pub fn wire_up_session() -> Result<(), String> {
    install_units()?;
    enable_network_and_bluetooth()?;
    enable_display_manager_if_none();
    set_login_shell_to_fish();
    prepare_xdg_dirs();
    Ok(())
}

fn systemd_user_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".config"))
        .join("systemd/user")
}

fn state_dir() -> PathBuf {
    dirs::state_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".local/state"))
        .join("shilpo")
}

fn polkit_agent_path() -> &'static str {
    for candidate in [
        "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1",
        "/usr/libexec/polkit-gnome-authentication-agent-1",
    ] {
        if Path::new(candidate).is_file() {
            return candidate;
        }
    }
    "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1"
}

fn install_units() -> Result<(), String> {
    let bin = env::current_exe()
        .map_err(|e| format!("could not resolve current executable path: {e}"))?
        .to_str()
        .ok_or("current executable path is not valid UTF-8")?
        .to_string();
    let unit_dir = systemd_user_dir();
    fs::create_dir_all(&unit_dir)
        .map_err(|e| format!("could not create {}: {e}", unit_dir.display()))?;

    let marker = state_dir().join("first-login-completed");
    let polkit_agent = polkit_agent_path();

    println!("Installing Shilpo systemd user units...");
    for name in UNITS {
        let rel = format!("systemd/user/{name}");
        let raw = SetupAssets::get(&rel).ok_or_else(|| format!("missing embedded unit {rel}"))?;
        let mut text = std::str::from_utf8(&raw.data)
            .map_err(|e| format!("embedded unit {name} is not valid UTF-8: {e}"))?
            .to_string();
        text = text.replace("/usr/bin/shilpo", &bin);
        text = text.replace("@FIRST_LOGIN_MARKER@", &marker.to_string_lossy());
        text = text.replace("@POLKIT_AGENT@", polkit_agent);
        fs::write(unit_dir.join(name), text).map_err(|e| format!("could not write {name}: {e}"))?;
    }

    // Wants-link every unit into the Niri session target directly, rather than going
    // through `systemctl --user enable`: setup commonly runs from a bare TTY right after
    // install, before any user systemd manager/session bus exists to talk to.
    let wants_dir = unit_dir.join("niri.service.wants");
    fs::create_dir_all(&wants_dir)
        .map_err(|e| format!("could not create {}: {e}", wants_dir.display()))?;
    for name in UNITS {
        let link = wants_dir.join(name);
        if !link.exists() {
            symlink(unit_dir.join(name), &link)
                .map_err(|e| format!("could not link {name} into niri.service.wants: {e}"))?;
        }
    }

    // Best-effort: only meaningful (and only possible) inside an already-running user
    // session; a fresh login picks these units up through niri.service.wants regardless.
    if env::var_os("WAYLAND_DISPLAY").is_some() || env::var_os("NIRI_SOCKET").is_some() {
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
        for name in UNITS {
            let status = Command::new("systemctl")
                .args(["--user", "start", name])
                .status();
            if !matches!(status, Ok(s) if s.success()) {
                eprintln!("warning: could not start user unit {name} in the active session");
            }
        }
    }

    Ok(())
}

fn enable_network_and_bluetooth() -> Result<(), String> {
    println!("Enabling NetworkManager and Bluetooth...");
    run_privileged(&[
        "systemctl",
        "enable",
        "--now",
        "NetworkManager.service",
        "bluetooth.service",
    ])
}

fn enable_display_manager_if_none() {
    let already_configured = Path::new("/etc/systemd/system/display-manager.service").exists()
        || DISPLAY_MANAGERS.iter().any(|dm| {
            Command::new("systemctl")
                .args(["is-enabled", &format!("{dm}.service")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        });

    if already_configured {
        println!("Existing display manager configuration preserved.");
        return;
    }

    println!("No display manager enabled; enabling SDDM for next boot...");
    if let Err(e) = run_privileged(&["systemctl", "enable", "sddm.service"]) {
        eprintln!(
            "warning: could not enable sddm.service ({e}); install a display manager and \
             enable it yourself, or start Niri from a TTY"
        );
    }
}

fn set_login_shell_to_fish() {
    if !is_on_path("fish") {
        return;
    }
    let user = env::var("USER").unwrap_or_default();
    if user.is_empty() {
        return;
    }
    let current_shell = Command::new("getent")
        .args(["passwd", &user])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|line| line.trim().split(':').nth(6).map(str::to_string));
    if current_shell.as_deref() == Some("/usr/bin/fish") {
        return;
    }

    println!("Changing login shell to /usr/bin/fish...");
    let result = if unsafe { libc::geteuid() } == 0 {
        Command::new("chsh")
            .args(["-s", "/usr/bin/fish", &user])
            .status()
    } else if is_on_path("sudo") {
        Command::new("sudo")
            .args(["chsh", "-s", "/usr/bin/fish", &user])
            .status()
    } else {
        Command::new("chsh").args(["-s", "/usr/bin/fish"]).status()
    };
    if !matches!(result, Ok(s) if s.success()) {
        eprintln!("warning: could not change login shell to fish");
    }
}

fn prepare_xdg_dirs() {
    if is_on_path("xdg-user-dirs-update") {
        let _ = Command::new("xdg-user-dirs-update").status();
    }
    if let Some(home) = dirs::home_dir() {
        let _ = fs::create_dir_all(home.join("Pictures/Screenshots"));
        let _ = fs::create_dir_all(home.join("Pictures/Wallpapers"));
    }
}
