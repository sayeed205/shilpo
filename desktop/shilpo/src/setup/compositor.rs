use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compositor {
    Niri,
    Hyprland,
}

impl fmt::Display for Compositor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Compositor::Niri => write!(f, "Niri"),
            Compositor::Hyprland => write!(f, "Hyprland"),
        }
    }
}

impl Compositor {
    /// Packages needed beyond the common desktop set (see `packages::COMMON_PACKAGES`).
    pub fn extra_packages(&self) -> &'static [&'static str] {
        match self {
            Compositor::Niri => &["niri"],
            Compositor::Hyprland => &["hyprland", "xdg-desktop-portal-hyprland"],
        }
    }

    /// The actual executable name to check for on `PATH` before staging config/units for
    /// this compositor — package installation can be declined or fail without aborting the
    /// rest of `shilpo setup`, which would otherwise silently configure a session that can
    /// never actually start.
    pub fn binary_name(&self) -> &'static str {
        match self {
            Compositor::Niri => "niri",
            Compositor::Hyprland => "Hyprland",
        }
    }

    /// Detects an already-running session via each compositor's own IPC socket env var,
    /// falling back to `XDG_CURRENT_DESKTOP`. Used when the distro is unrecognized and
    /// there's no package manager to drive an interactive choice against.
    pub fn detect_running() -> Option<Compositor> {
        if std::env::var_os("NIRI_SOCKET").is_some() {
            return Some(Compositor::Niri);
        }
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            return Some(Compositor::Hyprland);
        }
        let xdg = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_lowercase();
        if xdg.contains("niri") {
            return Some(Compositor::Niri);
        }
        if xdg.contains("hyprland") {
            return Some(Compositor::Hyprland);
        }
        None
    }
}

/// (menu label, selection). `None` entries are shown so users know what's coming, but
/// re-prompt instead of proceeding — there is no config to stage for them yet.
const OPTIONS: &[(&str, Option<Compositor>)] = &[
    ("Niri (recommended)", Some(Compositor::Niri)),
    ("Hyprland", Some(Compositor::Hyprland)),
    ("Sway (coming soon)", None),
];

pub fn choose() -> Result<Compositor, String> {
    loop {
        let labels: Vec<&str> = OPTIONS.iter().map(|(label, _)| *label).collect();
        let index = dialoguer::Select::new()
            .with_prompt("Which compositor do you want Shilpo to configure?")
            .items(&labels)
            .default(0)
            .interact()
            .map_err(|e| e.to_string())?;

        match OPTIONS[index].1 {
            Some(compositor) => return Ok(compositor),
            None => {
                let name = OPTIONS[index].0.trim_end_matches(" (coming soon)");
                println!(
                    "{name} isn't supported yet — Niri and Hyprland are the compositors Shilpo can configure right now.\n"
                );
            }
        }
    }
}
