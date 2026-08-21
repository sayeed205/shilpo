//! Interactive post-install configuration wizard (`shilpo setup`).
//!
//! Distinct from `./setup install`, which only builds and installs the `shilpo` binary.
//! This command runs *after* the binary is on `PATH` (however it got there — built from
//! source or, eventually, a distro package) and turns a bare install into a working
//! desktop session: choose a compositor, stage its recommended configuration, install
//! GPU drivers, wire up systemd user units, and enable the system services a session
//! needs. All embedded config content is baked into the binary via `SetupAssets` so this
//! works without a source checkout on disk.

mod compositor;
mod distro;
mod gpu;
mod privilege;
mod services;
mod stage;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../data"]
pub(crate) struct SetupAssets;

pub fn run() -> Result<(), String> {
    println!("Shilpo Setup");
    println!("============\n");

    distro::require_arch()?;

    let compositor = compositor::choose()?;

    gpu::detect_and_install_drivers()?;

    let apply = dialoguer::Confirm::new()
        .with_prompt(format!(
            "Apply Shilpo's recommended {compositor} configuration now?"
        ))
        .default(true)
        .interact()
        .map_err(|e| e.to_string())?;

    if apply {
        stage::stage_configs(compositor)?;
        services::wire_up_session()?;
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
        println!("Reboot later, then choose Niri at login, to start using Shilpo.");
    }

    Ok(())
}

fn print_doctor_report() {
    let doctor = crate::cli::adapters::DoctorChecker::new();
    let items = doctor.run_diagnostics(false);
    println!("\n{}", doctor.format_report(&items));
}
