use std::collections::BTreeSet;
use std::fs;

use super::SetupAssets;
use super::privilege::run_privileged;

pub fn detect_and_install_drivers() -> Result<(), String> {
    let (has_intel, has_amd, nvidia_device) = detect_vendors();

    if !has_intel && !has_amd && nvidia_device.is_none() {
        println!(
            "No supported GPU vendor detected under /sys/class/drm; skipping driver installation.\n"
        );
        return Ok(());
    }

    let mut packages: BTreeSet<String> = BTreeSet::new();
    packages.insert("mesa".to_string());
    if has_intel {
        packages.insert("vulkan-intel".to_string());
        packages.insert("intel-media-driver".to_string());
    }
    if has_amd {
        packages.insert("vulkan-radeon".to_string());
    }
    if let Some(device) = &nvidia_device {
        verify_nvidia_supported(device)?;

        let kernels = installed_kernels();
        match kernels.as_slice() {
            [only] if only == "linux" => {
                packages.insert("nvidia-open".to_string());
            }
            [only] if only == "linux-lts" => {
                packages.insert("nvidia-open-lts".to_string());
            }
            _ => {
                packages.insert("nvidia-open-dkms".to_string());
                for kernel in &kernels {
                    packages.insert(format!("{kernel}-headers"));
                }
            }
        }
        packages.insert("nvidia-utils".to_string());
        packages.insert("libva-nvidia-driver".to_string());
        if has_intel || has_amd {
            packages.insert("nvidia-prime".to_string());
        }
    }

    println!(
        "Detected GPU vendor(s): {}",
        vendor_summary(has_intel, has_amd, nvidia_device.is_some())
    );
    println!(
        "Driver packages: {}\n",
        packages.iter().cloned().collect::<Vec<_>>().join(", ")
    );

    let proceed = dialoguer::Confirm::new()
        .with_prompt("Install these GPU driver packages now (pacman -Syu --needed)?")
        .default(true)
        .interact()
        .map_err(|e| e.to_string())?;

    if !proceed {
        println!("Skipped GPU driver installation.\n");
        return Ok(());
    }

    let mut argv: Vec<&str> = vec!["pacman", "-Syu", "--needed", "--noconfirm"];
    argv.extend(packages.iter().map(String::as_str));
    run_privileged(&argv)?;
    println!();
    Ok(())
}

fn vendor_summary(has_intel: bool, has_amd: bool, has_nvidia: bool) -> String {
    let mut parts = Vec::new();
    if has_intel {
        parts.push("Intel");
    }
    if has_amd {
        parts.push("AMD");
    }
    if has_nvidia {
        parts.push("NVIDIA");
    }
    parts.join(", ")
}

/// Returns (has_intel, has_amd, nvidia_device_id) by reading PCI vendor/device ids straight
/// out of sysfs. No `lspci`/`pciutils` dependency needed, which matters since a minimal Arch
/// install may not have it yet.
fn detect_vendors() -> (bool, bool, Option<String>) {
    let mut has_intel = false;
    let mut has_amd = false;
    let mut nvidia_device = None;

    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return (false, false, None);
    };
    for entry in entries.flatten() {
        let device_dir = entry.path().join("device");
        let Ok(vendor) = fs::read_to_string(device_dir.join("vendor")) else {
            continue;
        };
        let Ok(device) = fs::read_to_string(device_dir.join("device")) else {
            continue;
        };
        match vendor.trim().to_lowercase().as_str() {
            "0x8086" => has_intel = true,
            "0x1002" => has_amd = true,
            "0x10de" => nvidia_device = Some(device.trim().to_lowercase()),
            _ => {}
        }
    }
    (has_intel, has_amd, nvidia_device)
}

fn verify_nvidia_supported(device: &str) -> Result<(), String> {
    let table = SetupAssets::get("nvidia/turing_newer_pci_ids.txt")
        .ok_or("missing embedded NVIDIA PCI id table")?;
    let table = std::str::from_utf8(&table.data)
        .map_err(|e| format!("embedded NVIDIA PCI id table is not valid UTF-8: {e}"))?;
    let supported = table
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case(device));
    if !supported {
        return Err(format!(
            "NVIDIA GPU device {device} is legacy or not in the committed Turing-or-newer hardware \
             table. Arch's official repositories support Turing (RTX 20xx / GTX 16xx) and newer via \
             open kernel modules; older cards need legacy AUR drivers (nvidia-390xx-dkms / \
             nvidia-470xx-dkms). See https://archlinux.org/news/nvidia-590-driver-drops-pascal-support-main-packages-switch-to-open-kernel-modules/"
        ));
    }
    Ok(())
}

fn installed_kernels() -> Vec<String> {
    ["linux", "linux-lts", "linux-zen", "linux-hardened"]
        .into_iter()
        .filter(|kernel| {
            std::process::Command::new("pacman")
                .args(["-Qq", kernel])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
        })
        .map(str::to_string)
        .collect()
}
