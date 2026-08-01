use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticItem {
    pub category: String,
    pub name: String,
    pub status: DiagnosticStatus,
    pub message: String,
    pub fix_applied: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DoctorChecker;

impl DoctorChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn default_config_path() -> PathBuf {
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(".config/shilpo/config.toml"))
            .unwrap_or_else(|_| PathBuf::from(".config/shilpo/config.toml"))
    }

    pub fn run_diagnostics(&self, auto_fix: bool) -> Vec<DiagnosticItem> {
        vec![
            self.check_niri_compositor(),
            self.check_shell_ipc(),
            self.check_config_file(auto_fix),
            self.check_wallpaper_directory(auto_fix),
            self.check_awww_backend(),
        ]
    }

    pub fn check_niri_compositor(&self) -> DiagnosticItem {
        if let Some(path) = shilpo_services::compositor::niri::resolve_niri_socket_path() {
            if path.exists() {
                DiagnosticItem {
                    category: "Compositor".into(),
                    name: "Niri IPC Socket".into(),
                    status: DiagnosticStatus::Pass,
                    message: format!("Active socket found at {}", path.display()),
                    fix_applied: false,
                }
            } else {
                DiagnosticItem {
                    category: "Compositor".into(),
                    name: "Niri IPC Socket".into(),
                    status: DiagnosticStatus::Warn,
                    message: format!(
                        "Socket path set to {} but socket file does not exist",
                        path.display()
                    ),
                    fix_applied: false,
                }
            }
        } else {
            DiagnosticItem {
                category: "Compositor".into(),
                name: "Niri IPC Socket".into(),
                status: DiagnosticStatus::Warn,
                message:
                "Neither $NIRI_SOCKET nor $NIRI_SOCKET_PATH is set (running in offline/headless fallback mode)"
                    .into(),
                fix_applied: false,
            }
        }
    }

    pub fn check_shell_ipc(&self) -> DiagnosticItem {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());

        let shilpo_ipc_dir = runtime_dir.join("shilpo-shell");
        if shilpo_ipc_dir.exists() {
            DiagnosticItem {
                category: "Shell IPC".into(),
                name: "Runtime Directory".into(),
                status: DiagnosticStatus::Pass,
                message: format!("Directory ready at {}", shilpo_ipc_dir.display()),
                fix_applied: false,
            }
        } else {
            DiagnosticItem {
                category: "Shell IPC".into(),
                name: "Runtime Directory".into(),
                status: DiagnosticStatus::Pass,
                message: format!(
                    "Will be created automatically on startup at {}",
                    shilpo_ipc_dir.display()
                ),
                fix_applied: false,
            }
        }
    }

    pub fn check_config_file(&self, auto_fix: bool) -> DiagnosticItem {
        let config_path = Self::default_config_path();
        if config_path.exists() {
            match shilpo_config::ShellConfig::load_or_create(&config_path) {
                Ok(_) => DiagnosticItem {
                    category: "Configuration".into(),
                    name: "config.toml".into(),
                    status: DiagnosticStatus::Pass,
                    message: format!("Valid configuration at {}", config_path.display()),
                    fix_applied: false,
                },
                Err(err) => DiagnosticItem {
                    category: "Configuration".into(),
                    name: "config.toml".into(),
                    status: DiagnosticStatus::Fail,
                    message: format!("Configuration syntax error: {}", err),
                    fix_applied: false,
                },
            }
        } else if auto_fix {
            let _ = shilpo_config::ShellConfig::load_or_create(&config_path);
            DiagnosticItem {
                category: "Configuration".into(),
                name: "config.toml".into(),
                status: DiagnosticStatus::Pass,
                message: format!("Created default config file at {}", config_path.display()),
                fix_applied: true,
            }
        } else {
            DiagnosticItem {
                category: "Configuration".into(),
                name: "config.toml".into(),
                status: DiagnosticStatus::Warn,
                message: format!(
                    "File missing at {}; defaults will be used",
                    config_path.display()
                ),
                fix_applied: false,
            }
        }
    }

    pub fn check_wallpaper_directory(&self, auto_fix: bool) -> DiagnosticItem {
        let config_path = Self::default_config_path();
        let wallpaper_dir = shilpo_config::ShellConfig::load(&config_path)
            .unwrap_or_default()
            .desktop
            .wallpaper_dir;
        let wallpaper_dir = expand_home_path(wallpaper_dir);
        if wallpaper_dir.exists() {
            let count = std::fs::read_dir(&wallpaper_dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter(|entry| {
                            let path = entry.path();
                            path.is_file()
                                && path.extension().and_then(|ext| ext.to_str()).is_some_and(
                                    |ext| {
                                        matches!(
                                            ext.to_ascii_lowercase().as_str(),
                                            "png" | "jpg" | "jpeg" | "webp"
                                        )
                                    },
                                )
                        })
                        .count()
                })
                .unwrap_or_default();
            DiagnosticItem {
                category: "Wallpaper".into(),
                name: "Wallpapers Directory".into(),
                status: DiagnosticStatus::Pass,
                message: format!(
                    "Found {} wallpaper(s) in {}",
                    count,
                    wallpaper_dir.display()
                ),
                fix_applied: false,
            }
        } else if auto_fix {
            let _ = std::fs::create_dir_all(&wallpaper_dir);
            DiagnosticItem {
                category: "Wallpaper".into(),
                name: "Wallpapers Directory".into(),
                status: DiagnosticStatus::Pass,
                message: format!("Created wallpaper directory at {}", wallpaper_dir.display()),
                fix_applied: true,
            }
        } else {
            DiagnosticItem {
                category: "Wallpaper".into(),
                name: "Wallpapers Directory".into(),
                status: DiagnosticStatus::Warn,
                message: format!("Directory does not exist at {}", wallpaper_dir.display()),
                fix_applied: false,
            }
        }
    }

    pub fn check_awww_backend(&self) -> DiagnosticItem {
        let is_installed = std::process::Command::new("awww")
            .arg("--version")
            .output()
            .is_ok();
        if is_installed {
            DiagnosticItem {
                category: "Wallpaper".into(),
                name: "awww Daemon Backend".into(),
                status: DiagnosticStatus::Pass,
                message: "awww wallpaper daemon CLI client detected on $PATH".into(),
                fix_applied: false,
            }
        } else {
            DiagnosticItem {
                category: "Wallpaper".into(),
                name: "awww Daemon Backend".into(),
                status: DiagnosticStatus::Warn,
                message: "awww binary not found on $PATH; wallpaper switching will fail until the backend is installed".into(),
                fix_applied: false,
            }
        }
    }

    pub fn format_report(&self, items: &[DiagnosticItem]) -> String {
        let mut out = String::from("\n=== Shilpo Doctor Diagnostics ===\n\n");
        for item in items {
            let badge = match item.status {
                DiagnosticStatus::Pass => "[✓ PASS]",
                DiagnosticStatus::Warn => "[⚠ WARN]",
                DiagnosticStatus::Fail => "[✗ FAIL]",
            };
            let fix_note = if item.fix_applied {
                " (auto-fixed)"
            } else {
                ""
            };
            out.push_str(&format!(
                "{:10} {:22} {} {}{}\n",
                badge,
                format!("[{}]", item.category),
                item.name,
                item.message,
                fix_note
            ));
        }
        out
    }
}

fn expand_home_path(path: PathBuf) -> PathBuf {
    if let Some(rest) = path.to_str().and_then(|path| path.strip_prefix("~/")) {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(rest)
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::expand_home_path;
    use std::path::PathBuf;

    #[test]
    fn wallpaper_path_expands_home_prefix() {
        let expanded = expand_home_path(PathBuf::from("~/Pictures/Wallpapers"));
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        assert_eq!(expanded, home.join("Pictures/Wallpapers"));
    }

    #[test]
    fn absolute_wallpaper_path_is_preserved() {
        let path = PathBuf::from("/srv/wallpapers");
        assert_eq!(expand_home_path(path.clone()), path);
    }
}
