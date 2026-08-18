use std::fmt;

/// Supported Wayland compositor kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositorKind {
    Unknown,
    Niri,
    Hyprland,
    Sway,
    Labwc,
    Dwl,
    River,
    Kde,
}

impl CompositorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Niri => "niri",
            Self::Hyprland => "hyprland",
            Self::Sway => "sway",
            Self::Labwc => "labwc",
            Self::Dwl => "dwl",
            Self::River => "river",
            Self::Kde => "kde",
        }
    }
}

impl fmt::Display for CompositorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Detects active compositor using an injectable environment variable lookup.
///
/// Invariant: Compositor-set socket/pid environment variables take precedence over
/// desktop session hints. Evaluated in order: `NIRI_SOCKET`, `HYPRLAND_INSTANCE_SIGNATURE`,
/// `SWAYSOCK`, `LABWC_PID`.
///
/// If none are present, falls back to substring matching over the joined desktop hints
/// (`XDG_CURRENT_DESKTOP:XDG_SESSION_DESKTOP:DESKTOP_SESSION`).
pub fn detect_from(get_var: &dyn Fn(&str) -> Option<String>) -> CompositorKind {
    // 1. Direct compositor-set environment variables (highest priority)
    if is_set_and_non_empty(get_var, "NIRI_SOCKET")
        || is_set_and_non_empty(get_var, "NIRI_SOCKET_PATH")
    {
        return CompositorKind::Niri;
    }
    if is_set_and_non_empty(get_var, "HYPRLAND_INSTANCE_SIGNATURE") {
        return CompositorKind::Hyprland;
    }
    if is_set_and_non_empty(get_var, "SWAYSOCK") {
        return CompositorKind::Sway;
    }
    if is_set_and_non_empty(get_var, "LABWC_PID") {
        return CompositorKind::Labwc;
    }

    // 2. Desktop session hints fallback
    let xdg_current = get_var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let xdg_session = get_var("XDG_SESSION_DESKTOP").unwrap_or_default();
    let desktop_session = get_var("DESKTOP_SESSION").unwrap_or_default();

    let hint = format!("{xdg_current}:{xdg_session}:{desktop_session}").to_lowercase();
    if hint.is_empty() || hint == "::" {
        return CompositorKind::Unknown;
    }

    if hint.contains("niri") {
        CompositorKind::Niri
    } else if hint.contains("hyprland") {
        CompositorKind::Hyprland
    } else if hint.contains("sway") {
        CompositorKind::Sway
    } else if hint.contains("labwc") {
        CompositorKind::Labwc
    } else if hint.contains("dwl") {
        CompositorKind::Dwl
    } else if hint.contains("river") {
        CompositorKind::River
    } else if hint.contains("kde") || hint.contains("plasma") {
        CompositorKind::Kde
    } else {
        CompositorKind::Unknown
    }
}

fn is_set_and_non_empty(get_var: &dyn Fn(&str) -> Option<String>, key: &str) -> bool {
    get_var(key).is_some_and(|v| !v.trim().is_empty())
}

/// Detects the active compositor from process environment.
///
/// Deterministic and hermetic: does not use a process-global OnceLock.
pub fn detect() -> CompositorKind {
    detect_from(&|key| std::env::var(key).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    type EnvLookup = Box<dyn Fn(&str) -> Option<String>>;

    fn make_env(vars: &[(&str, &str)]) -> EnvLookup {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Box::new(move |key: &str| map.get(key).cloned())
    }

    #[test]
    fn test_empty_env_yields_unknown() {
        let env = make_env(&[]);
        assert_eq!(detect_from(&env), CompositorKind::Unknown);
    }

    #[test]
    fn test_compositor_socket_vars_precedence() {
        let cases = [
            (
                "NIRI_SOCKET",
                "/run/user/1000/niri.sock",
                CompositorKind::Niri,
            ),
            (
                "NIRI_SOCKET_PATH",
                "/run/user/1000/niri.sock",
                CompositorKind::Niri,
            ),
            (
                "HYPRLAND_INSTANCE_SIGNATURE",
                "abcd1234",
                CompositorKind::Hyprland,
            ),
            (
                "SWAYSOCK",
                "/run/user/1000/sway-ipc.sock",
                CompositorKind::Sway,
            ),
            ("LABWC_PID", "12345", CompositorKind::Labwc),
        ];

        for (var_name, val, expected) in cases {
            let env = make_env(&[(var_name, val)]);
            assert_eq!(detect_from(&env), expected, "failed for {var_name}");
        }
    }

    #[test]
    fn test_socket_var_wins_over_conflicting_desktop_hint() {
        let env = make_env(&[
            ("NIRI_SOCKET", "/run/user/1000/niri.sock"),
            ("XDG_CURRENT_DESKTOP", "Hyprland"),
        ]);
        assert_eq!(detect_from(&env), CompositorKind::Niri);

        let env2 = make_env(&[
            ("HYPRLAND_INSTANCE_SIGNATURE", "sig"),
            ("XDG_CURRENT_DESKTOP", "sway"),
        ]);
        assert_eq!(detect_from(&env2), CompositorKind::Hyprland);
    }

    #[test]
    fn test_desktop_hints_substring_matching() {
        let cases = [
            ("XDG_CURRENT_DESKTOP", "niri", CompositorKind::Niri),
            ("XDG_CURRENT_DESKTOP", "Hyprland", CompositorKind::Hyprland),
            ("XDG_CURRENT_DESKTOP", "sway", CompositorKind::Sway),
            ("XDG_CURRENT_DESKTOP", "labwc", CompositorKind::Labwc),
            ("XDG_CURRENT_DESKTOP", "dwl", CompositorKind::Dwl),
            ("XDG_CURRENT_DESKTOP", "river", CompositorKind::River),
            ("XDG_CURRENT_DESKTOP", "KDE", CompositorKind::Kde),
            ("XDG_CURRENT_DESKTOP", "plasma", CompositorKind::Kde),
            ("XDG_SESSION_DESKTOP", "plasmawayland", CompositorKind::Kde),
            (
                "DESKTOP_SESSION",
                "/usr/share/wayland-sessions/dwl",
                CompositorKind::Dwl,
            ),
            ("XDG_CURRENT_DESKTOP", "GNOME", CompositorKind::Unknown),
            ("XDG_CURRENT_DESKTOP", "XFCE", CompositorKind::Unknown),
        ];

        for (var, val, expected) in cases {
            let env = make_env(&[(var, val)]);
            assert_eq!(detect_from(&env), expected, "failed for {var}={val}");
        }
    }

    #[test]
    fn test_detect_from_order_precedence_among_sockets() {
        // Niri socket beats Hyprland signature if both are set
        let env = make_env(&[
            ("NIRI_SOCKET", "/run/user/1000/niri.sock"),
            ("HYPRLAND_INSTANCE_SIGNATURE", "sig"),
        ]);
        assert_eq!(detect_from(&env), CompositorKind::Niri);

        // Hyprland signature beats Sway socket
        let env2 = make_env(&[
            ("HYPRLAND_INSTANCE_SIGNATURE", "sig"),
            ("SWAYSOCK", "/run/user/1000/sway.sock"),
        ]);
        assert_eq!(detect_from(&env2), CompositorKind::Hyprland);
    }
}
