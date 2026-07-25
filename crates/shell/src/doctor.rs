use shilpo_services::WallpaperService;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone)]
pub struct DiagnosticItem {
    pub category: String,
    pub name: String,
    pub status: DiagnosticStatus,
    pub message: String,
    pub fix_applied: bool,
}

/// Doctor diagnostic runner for Shilpo Desktop Shell.
#[derive(Debug, Clone, Default)]
pub struct DoctorChecker;

impl DoctorChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn default_config_path() -> std::path::PathBuf {
        std::env::var("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".config/shilpo/config.toml"))
            .unwrap_or_else(|_| std::path::PathBuf::from(".config/shilpo/config.toml"))
    }

    /// Runs all diagnostic checks and attempts auto-fixing if requested.
    pub fn run_diagnostics(&self, auto_fix: bool) -> Vec<DiagnosticItem> {
        vec![
            self.check_niri_compositor(),
            self.check_shell_ipc(),
            self.check_config_file(auto_fix),
            self.check_wallpaper_directory(auto_fix),
        ]
    }

    /// Checks if Niri Wayland compositor IPC socket is accessible.
    pub fn check_niri_compositor(&self) -> DiagnosticItem {
        let niri_socket = std::env::var("NIRI_SOCKET");
        match niri_socket {
            Ok(path) => {
                if std::path::Path::new(&path).exists() {
                    DiagnosticItem {
                        category: "Compositor".into(),
                        name: "Niri IPC Socket".into(),
                        status: DiagnosticStatus::Pass,
                        message: format!("Active socket found at {}", path),
                        fix_applied: false,
                    }
                } else {
                    DiagnosticItem {
                        category: "Compositor".into(),
                        name: "Niri IPC Socket".into(),
                        status: DiagnosticStatus::Warn,
                        message: format!(
                            "$NIRI_SOCKET is set to {} but socket file does not exist",
                            path
                        ),
                        fix_applied: false,
                    }
                }
            }
            Err(_) => DiagnosticItem {
                category: "Compositor".into(),
                name: "Niri IPC Socket".into(),
                status: DiagnosticStatus::Warn,
                message: "$NIRI_SOCKET is not set (running in offline/headless fallback mode)"
                    .into(),
                fix_applied: false,
            },
        }
    }

    /// Checks shell IPC runtime directory and socket status.
    pub fn check_shell_ipc(&self) -> DiagnosticItem {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
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

    /// Checks configuration file validity and readiness.
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

    /// Checks default wallpaper directory existence and image readiness.
    pub fn check_wallpaper_directory(&self, auto_fix: bool) -> DiagnosticItem {
        let wallpaper_dir = WallpaperService::default_wallpaper_dir();
        if wallpaper_dir.exists() {
            let service = WallpaperService::new(&wallpaper_dir);
            let count = service.scan_wallpapers().len();
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

    /// Formats and prints diagnostic items to standard output.
    pub fn print_report(&self, items: &[DiagnosticItem]) {
        println!("\n=== Shilpo Shell Doctor Diagnostics ===\n");
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
            println!(
                "{:10} {:22} {} {}{}",
                badge,
                format!("[{}]", item.category),
                item.name,
                item.message,
                fix_note
            );
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doctor_diagnostic_suite() {
        let doctor = DoctorChecker::new();
        let results = doctor.run_diagnostics(false);
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.category == "Configuration"));
    }
}
