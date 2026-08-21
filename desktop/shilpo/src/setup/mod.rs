//! Interactive post-install configuration wizard (`shilpo setup`).
//!
//! Distinct from `./setup install`, which only builds and installs the `shilpo` binary and
//! exists for local dev/testing. This command runs *after* the binary is on `PATH` and turns
//! a bare install into a working desktop session: detect the distro, choose (or detect) a
//! compositor, install its packages, stage its recommended configuration, install GPU
//! drivers, and wire up the session. All embedded config content is baked into the binary
//! via `SetupAssets` so this works without a source checkout on disk.

mod compositor;
mod distro;
mod gpu;
mod packages;
mod privilege;
mod services;
mod stage;

use std::path::Path;

use rust_embed::RustEmbed;

use compositor::Compositor;
use distro::Distro;

#[derive(RustEmbed)]
#[folder = "../../data"]
pub(crate) struct SetupAssets;

/// Shared by `stage` (exec-once/spawn lines) and `services` (systemd unit `ExecStart`):
/// resolves whichever agent is actually present, since the exact path/package name varies
/// across distros and desktops.
pub(crate) fn polkit_agent_path() -> &'static str {
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

pub fn run() -> Result<(), String> {
    println!("Shilpo Setup");
    println!("============\n");

    let distro = distro::detect();

    let compositor = match distro {
        Distro::Arch => compositor::choose()?,
        Distro::Unknown => match Compositor::detect_running() {
            Some(compositor) => {
                println!(
                    "Could not recognize this distro, so package installation is unavailable. \
                     Detected an active {compositor} session — configuring for that.\n"
                );
                compositor
            }
            None => {
                return Err(
                    "could not recognize this distro or detect an active Niri/Hyprland session; \
                     shilpo setup doesn't know what to configure here"
                        .to_string(),
                );
            }
        },
    };

    match distro {
        Distro::Arch => {
            packages::install_for(compositor)?;
            gpu::detect_and_install_drivers()?;
        }
        Distro::Unknown => {
            println!(
                "Automatic package installation is only supported on Arch Linux right now; \
                 install {compositor}'s packages yourself before continuing.\n"
            );
        }
    }

    if !privilege::is_on_path(compositor.binary_name()) {
        return Err(format!(
            "{compositor}'s `{}` executable is not on PATH — it wasn't installed (declined, or \
             package installation failed/was skipped). Install {compositor} yourself, then \
             re-run shilpo setup; nothing was staged.",
            compositor.binary_name()
        ));
    }

    let apply = dialoguer::Confirm::new()
        .with_prompt(format!(
            "Apply Shilpo's recommended {compositor} configuration now?"
        ))
        .default(true)
        .interact()
        .map_err(|e| e.to_string())?;

    if apply {
        stage::stage_configs(compositor)?;
        services::wire_up_session(compositor)?;
        println!("\nConfiguration applied.");
    } else {
        println!("\nSkipped applying configuration; nothing was changed.");
    }

    print_doctor_report();

    let reboot = dialoguer::Confirm::new()
        .with_prompt("Reboot now to start using Shilpo?")
        .default(true)
        .interact()
        .map_err(|e| e.to_string())?;

    if reboot {
        println!("Rebooting...");
        let status = std::process::Command::new("systemctl")
            .arg("reboot")
            .status()
            .map_err(|e| format!("failed to invoke systemctl reboot: {e}"))?;
        if !status.success() {
            return Err("systemctl reboot exited with a non-zero status".to_string());
        }
    } else {
        println!("Reboot later, then choose {compositor} at login, to start using Shilpo.");
    }

    Ok(())
}

fn print_doctor_report() {
    let doctor = crate::cli::adapters::DoctorChecker::new();
    let items = doctor.run_diagnostics(false);
    println!("\n{}", doctor.format_report(&items));
}
