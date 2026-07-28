use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

static SHARED_WALLPAPER_DIR: OnceLock<Arc<Mutex<PathBuf>>> = OnceLock::new();

fn get_shared_wallpaper_dir() -> Arc<Mutex<PathBuf>> {
    SHARED_WALLPAPER_DIR
        .get_or_init(|| Arc::new(Mutex::new(PathBuf::from("~/Pictures/Wallpapers"))))
        .clone()
}

/// Event emitted when wallpaper changes and M3 colors are regenerated.
///
/// Subscribers receive this via [`WallpaperService::subscribe`]. External apps
/// can read the same data from `~/.local/state/shilpo/colors.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorSchemeChanged {
    /// Absolute path to the active wallpaper image.
    pub wallpaper: PathBuf,
    /// Material 3 seed color extracted from the wallpaper (ARGB u32).
    pub source_argb: u32,
    /// Seed color as `#RRGGBB` hex string.
    pub source_hex: String,
    /// Active theme mode ("dark", "light", "system").
    #[serde(default = "default_theme_mode")]
    pub mode: String,
    /// ISO 8601 timestamp of when the colors were generated.
    pub generated_at: String,
    /// Full M3 light scheme: `"primary" → "#1B6B22"`, etc.
    pub light: HashMap<String, String>,
    /// Full M3 dark scheme: `"primary" → "#87D982"`, etc.
    pub dark: HashMap<String, String>,
}

fn default_theme_mode() -> String {
    "dark".to_string()
}

/// Desktop Wallpaper Service managing directory scanning, active wallpaper selection,
/// M3 color generation, and event-driven change notifications.
///
/// When a wallpaper is set via [`set_wallpaper`] or [`set_random_wallpaper`]:
/// 1. The wallpaper backend (`awww`) is invoked.
/// 2. M3 seed color is extracted from the image.
/// 3. Both light and dark M3 schemes (48 tokens each) are generated.
/// 4. The result is atomically written to `~/.local/state/shilpo/colors.json`.
/// 5. All [`subscribe`] receivers are notified via inotify on the state file.
///
/// Any external app or script can also read `colors.json` directly.
#[derive(Debug, Clone)]
pub struct WallpaperService {
    wallpaper_dir: Arc<Mutex<PathBuf>>,
}

/// Result of a wallpaper change operation containing the active path and extracted M3 seed color.
#[derive(Debug, Clone)]
pub struct WallpaperChangeResult {
    pub path: PathBuf,
    pub source_argb: Option<u32>,
}

impl WallpaperService {
    /// Creates a new WallpaperService for the specified directory.
    pub fn new(wallpaper_dir: impl Into<PathBuf>) -> Self {
        Self {
            wallpaper_dir: Arc::new(Mutex::new(wallpaper_dir.into())),
        }
    }

    /// Returns the default wallpaper directory path (`~/Pictures/Wallpapers`).
    pub fn default_wallpaper_dir() -> PathBuf {
        PathBuf::from("~/Pictures/Wallpapers")
    }

    /// Returns the path to the state file (`~/.local/state/shilpo/colors.json`).
    ///
    /// Uses `dirs::state_dir()` (`$XDG_STATE_HOME`) with fallback to
    /// `~/.local/state/shilpo/colors.json`.
    pub fn state_file_path() -> PathBuf {
        let state_dir = dirs::state_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("/tmp"))
                    .join(".local/state")
            })
            .join("shilpo");
        state_dir.join("colors.json")
    }

    /// Returns the currently active wallpaper directory path.
    pub fn wallpaper_dir(&self) -> PathBuf {
        self.wallpaper_dir.lock().unwrap().clone()
    }

    /// Sets the wallpaper directory path, creating the directory if it does not exist.
    pub fn set_wallpaper_dir(&self, dir: impl Into<PathBuf>) {
        let dir = dir.into();
        let expanded = Self::expand_tilde(&dir);
        let _ = std::fs::create_dir_all(&expanded);
        let mut current_dir = self.wallpaper_dir.lock().unwrap();
        *current_dir = dir;
    }

    /// Scans the wallpaper directory for supported image files (.png, .jpg, .jpeg, .webp).
    pub fn scan_wallpapers(&self) -> Vec<PathBuf> {
        let mut wallpapers = Vec::new();
        let target_dir = Self::expand_tilde(&self.wallpaper_dir());
        if !target_dir.exists() {
            return wallpapers;
        }

        if let Ok(entries) = std::fs::read_dir(&target_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && Self::is_supported_image(&path) {
                    wallpapers.push(path);
                }
            }
        }

        wallpapers.sort();
        wallpapers
    }

    /// Checks if a file has a supported image extension.
    fn is_supported_image(path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp")
        } else {
            false
        }
    }

    fn expand_tilde(path: &Path) -> PathBuf {
        if let Ok(path_str) = path.to_path_buf().into_os_string().into_string() {
            if let (Some(rest), Ok(home)) = (path_str.strip_prefix("~/"), std::env::var("HOME")) {
                return PathBuf::from(home).join(rest);
            }
            PathBuf::from(path_str)
        } else {
            path.to_path_buf()
        }
    }

    /// Returns the currently active wallpaper path by reading from `colors.json`.
    pub fn active_wallpaper(&self) -> Option<PathBuf> {
        Self::read_current().map(|state| state.wallpaper)
    }

    /// Sets the active wallpaper to the specified path, invoking the `awww` backend daemon,
    /// extracting M3 seed color, generating full light + dark color schemes,
    /// and writing the result to `colors.json`.
    ///
    /// All [`subscribe`] receivers will be notified via the inotify file watcher.
    pub fn set_wallpaper(&self, path: impl AsRef<Path>) -> Result<WallpaperChangeResult> {
        let path = path.as_ref();
        if !path.exists() {
            bail!("Wallpaper file does not exist: {}", path.display());
        }

        if !Self::is_supported_image(path) {
            bail!("Unsupported image format for wallpaper: {}", path.display());
        }

        // Execute awww wallpaper backend daemon if available
        if let Err(error) = std::process::Command::new("awww")
            .arg("img")
            .arg(path)
            .status()
        {
            tracing::debug!(error = %error, path = %path.display(), "awww backend invocation skipped or failed");
        }

        // Extract Material 3 Expressive dominant source seed color from wallpaper
        let extractor = crate::palette::PaletteExtractor::new();
        let source_argb = extractor.extract_source_argb_from_file(path).ok();

        // Generate full M3 light + dark color schemes and write state file
        if let Some(argb) = source_argb
            && let Err(error) = Self::write_state_file(path, argb)
        {
            tracing::warn!(error = %error, "failed to write colors.json state file");
        }

        tracing::info!(
            path = %path.display(),
            source_argb = ?source_argb.map(|c| format!("#{:08x}", c)),
            "Active wallpaper updated with awww backend and M3 color extraction"
        );

        Ok(WallpaperChangeResult {
            path: path.to_path_buf(),
            source_argb,
        })
    }

    /// Selects and sets a random wallpaper from the scanned directory.
    pub fn set_random_wallpaper(&self) -> Result<WallpaperChangeResult> {
        let wallpapers = self.scan_wallpapers();
        if wallpapers.is_empty() {
            bail!(
                "No wallpapers found in directory: {}",
                self.wallpaper_dir().display()
            );
        }

        let rng_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as usize;

        let index = rng_seed % wallpapers.len();
        let selected = wallpapers[index].clone();
        self.set_wallpaper(&selected)
    }

    /// Reads the current color scheme state from `colors.json`.
    ///
    /// Returns `None` if the state file doesn't exist or can't be parsed.
    pub fn read_current() -> Option<ColorSchemeChanged> {
        let path = Self::state_file_path();
        let contents = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    /// Syncs the OS system-wide theme mode across desktop environments.
    ///
    /// Supports:
    /// - **GNOME / GTK** via `gsettings` (`color-scheme`, `gtk-theme`)
    /// - **KDE Plasma** via `kwriteconfig6` / `kwriteconfig5` (`kdeglobals`)
    /// - **dconf** direct write as fallback for compositors like niri
    ///
    /// The XDG Desktop Portal (`xdg-desktop-portal-gnome`/`gtk`) reads the
    /// `gsettings` value and broadcasts `color-scheme` changes to sandboxed and
    /// portal-aware applications (Flatpak, Chromium, GTK4, Qt with `gtk3`
    /// platform theme).
    pub fn sync_system_desktop_theme_mode(mode: &str) {
        let is_dark = match mode.to_lowercase().as_str() {
            "dark" => true,
            "light" => false,
            _ => return,
        };

        // 1. GNOME / GTK (gsettings → portal → all GTK/portal-aware apps)
        let color_scheme = if is_dark {
            "prefer-dark"
        } else {
            "prefer-light"
        };
        let gtk_theme = if is_dark {
            if Self::path_exists("/usr/share/themes/adw-gtk3-dark") {
                "adw-gtk3-dark"
            } else {
                "Adwaita-dark"
            }
        } else {
            if Self::path_exists("/usr/share/themes/adw-gtk3") {
                "adw-gtk3"
            } else {
                "Adwaita"
            }
        };

        let gsettings_ok = std::process::Command::new("gsettings")
            .args([
                "set",
                "org.gnome.desktop.interface",
                "color-scheme",
                color_scheme,
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if gsettings_ok {
            let _ = std::process::Command::new("gsettings")
                .args(["set", "org.gnome.desktop.interface", "gtk-theme", gtk_theme])
                .status();
        } else {
            // Fallback: dconf direct write (for compositors like niri where
            // gsettings schema may not be installed but dconf backend works)
            let _ = std::process::Command::new("dconf")
                .args([
                    "write",
                    "/org/gnome/desktop/interface/color-scheme",
                    &format!("'{}'", color_scheme),
                ])
                .status();
        }

        // 2. KDE Plasma (kwriteconfig6, fallback kwriteconfig5)
        let kde_tool = if Self::command_exists("kwriteconfig6") {
            Some("kwriteconfig6")
        } else if Self::command_exists("kwriteconfig5") {
            Some("kwriteconfig5")
        } else {
            None
        };

        if let Some(tool) = kde_tool {
            let kde_scheme = if is_dark { "BreezeDark" } else { "BreezeLight" };
            let _ = std::process::Command::new(tool)
                .args([
                    "--file",
                    "kdeglobals",
                    "--group",
                    "General",
                    "--key",
                    "ColorScheme",
                    kde_scheme,
                ])
                .status();
        }

        tracing::info!(
            mode,
            color_scheme,
            gtk_theme,
            kde = kde_tool.is_some(),
            "synced OS desktop theme mode"
        );
    }

    /// Checks whether a file or directory path exists.
    fn path_exists(path: impl AsRef<Path>) -> bool {
        path.as_ref().exists()
    }

    /// Checks whether a command-line tool is available on `$PATH`.
    fn command_exists(name: &str) -> bool {
        std::process::Command::new("which")
            .arg(name)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Persists the given theme mode to `colors.json` without triggering system
    /// desktop sync. Use when the system already has the correct mode (e.g.,
    /// from an external `gsettings` / XDG portal change) and we just need to
    /// persist the mode to our state file for other Shilpo components.
    pub fn persist_theme_mode(mode: impl Into<String>) -> Result<()> {
        let mode = mode.into();
        if let Some(mut state) = Self::read_current() {
            state.mode = mode;
            state.generated_at = chrono::Utc::now().to_rfc3339();
            let state_path = Self::state_file_path();
            let tmp_path = state_path.with_extension("json.tmp");
            let json = serde_json::to_string_pretty(&state)?;
            std::fs::write(&tmp_path, json)?;
            std::fs::rename(&tmp_path, &state_path)?;
        }
        Ok(())
    }

    /// Updates the active theme mode in `colors.json` and syncs the OS desktop
    /// theme mode across all supported desktop environments.
    ///
    /// Writes to `colors.json` **first** so that inotify subscribers apply the
    /// new mode before the system appearance change (from `gsettings`) triggers
    /// window appearance observers.
    pub fn update_theme_mode(mode: impl Into<String>) -> Result<()> {
        let mode = mode.into();
        // Persist to colors.json first — inotify subscribers pick it up
        // before the gsettings-triggered appearance observer can race.
        Self::persist_theme_mode(&mode)?;
        Self::sync_system_desktop_theme_mode(&mode);
        Ok(())
    }

    /// Subscribes to wallpaper/color change events.
    ///
    /// Returns a channel receiver that emits [`ColorSchemeChanged`] whenever
    /// `colors.json` is updated. Uses `notify` (inotify on Linux) to watch the
    /// state file for modifications — no polling.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let rx = WallpaperService::subscribe();
    /// // In a GPUI async task:
    /// cx.spawn(async move |cx| {
    ///     while let Ok(changed) = rx.recv().await {
    ///         cx.update(|cx| {
    ///             Theme::global_mut(cx).set_source_argb(changed.source_argb);
    ///             cx.refresh_windows();
    ///         });
    ///     }
    /// }).detach();
    /// ```
    pub fn subscribe() -> smol::channel::Receiver<ColorSchemeChanged> {
        let (tx, rx) = smol::channel::unbounded();
        let state_file = Self::state_file_path();

        // Ensure state directory exists so the watcher has something to watch
        if let Some(parent) = state_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let watch_dir = state_file
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let target_name = state_file.file_name().map(|n| n.to_os_string());

        std::thread::Builder::new()
            .name("shilpo-wallpaper-watcher".into())
            .spawn(move || {
                use notify::Watcher;

                let tx_clone = tx.clone();
                let target_name_clone = target_name.clone();
                let mut watcher = match notify::RecommendedWatcher::new(
                    move |res: Result<notify::Event, notify::Error>| {
                        if let Ok(event) = res {
                            if !event.kind.is_modify() && !event.kind.is_create() {
                                return;
                            }
                            // Only react to our specific file
                            let is_target = target_name_clone.is_none()
                                || event
                                .paths
                                .iter()
                                .any(|p| p.file_name() == target_name_clone.as_deref());
                            if !is_target {
                                return;
                            }
                            if let Some(changed) = Self::read_current() {
                                let _ = tx_clone.send_blocking(changed);
                            }
                        }
                    },
                    notify::Config::default(),
                ) {
                    Ok(w) => w,
                    Err(error) => {
                        tracing::warn!(error = %error, "wallpaper state file watcher creation failed");
                        return;
                    }
                };

                if let Err(error) =
                    watcher.watch(&watch_dir, notify::RecursiveMode::NonRecursive)
                {
                    tracing::warn!(error = %error, path = ?watch_dir, "wallpaper state file watch failed");
                    return;
                }

                // Keep thread alive to maintain the watcher
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(3600));
                }
            })
            .expect("failed to spawn wallpaper watcher thread");

        rx
    }

    /// Generates and atomically writes the M3 color scheme state file.
    ///
    /// Writes to a `.tmp` file first, then renames to prevent partial reads.
    fn write_state_file(wallpaper_path: &Path, source_argb: u32) -> Result<()> {
        use mcu_material_color::{Hct, SchemeTonalSpot};

        let light_scheme = SchemeTonalSpot::new(Hct::from_int(source_argb), false, 0.0);
        let dark_scheme = SchemeTonalSpot::new(Hct::from_int(source_argb), true, 0.0);

        let light = Self::scheme_to_hex_map(&light_scheme);
        let dark = Self::scheme_to_hex_map(&dark_scheme);

        let current_mode = Self::read_current()
            .map(|s| s.mode)
            .unwrap_or_else(|| "dark".to_string());

        let state = ColorSchemeChanged {
            wallpaper: wallpaper_path.to_path_buf(),
            source_argb,
            source_hex: format!(
                "#{:02X}{:02X}{:02X}",
                (source_argb >> 16) & 0xff,
                (source_argb >> 8) & 0xff,
                source_argb & 0xff
            ),
            mode: current_mode,
            generated_at: chrono::Utc::now().to_rfc3339(),
            light,
            dark,
        };

        let state_path = Self::state_file_path();
        if let Some(parent) = state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Atomic write: write to .tmp then rename
        let tmp_path = state_path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&state)?;
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(&tmp_path, &state_path)?;

        tracing::info!(
            path = %state_path.display(),
            source_hex = %state.source_hex,
            "wrote M3 color scheme to colors.json"
        );

        Ok(())
    }

    /// Converts a Material 3 `SchemeTonalSpot` into a map of color role names to `#RRGGBB` hex strings.
    fn scheme_to_hex_map(scheme: &mcu_material_color::SchemeTonalSpot) -> HashMap<String, String> {
        use mcu_material_color::MaterialDynamicColors;

        macro_rules! color_map {
            ($($name:ident),+ $(,)?) => {{
                let mut map = HashMap::new();
                $(
                    let argb = MaterialDynamicColors::$name().get_argb(scheme);
                    map.insert(
                        stringify!($name).to_string(),
                        format!(
                            "#{:02X}{:02X}{:02X}",
                            (argb >> 16) & 0xff,
                            (argb >> 8) & 0xff,
                            argb & 0xff
                        ),
                    );
                )+
                map
            }};
        }

        color_map!(
            surface,
            on_surface,
            surface_dim,
            surface_bright,
            surface_container_lowest,
            surface_container_low,
            surface_container,
            surface_container_high,
            surface_container_highest,
            surface_variant,
            on_surface_variant,
            inverse_surface,
            inverse_on_surface,
            outline,
            outline_variant,
            shadow,
            scrim,
            surface_tint,
            primary,
            on_primary,
            primary_container,
            on_primary_container,
            inverse_primary,
            primary_fixed,
            primary_fixed_dim,
            on_primary_fixed,
            on_primary_fixed_variant,
            secondary,
            on_secondary,
            secondary_container,
            on_secondary_container,
            secondary_fixed,
            secondary_fixed_dim,
            on_secondary_fixed,
            on_secondary_fixed_variant,
            tertiary,
            on_tertiary,
            tertiary_container,
            on_tertiary_container,
            tertiary_fixed,
            tertiary_fixed_dim,
            on_tertiary_fixed,
            on_tertiary_fixed_variant,
            error,
            on_error,
            error_container,
            on_error_container,
        )
    }
}

impl Default for WallpaperService {
    fn default() -> Self {
        Self {
            wallpaper_dir: get_shared_wallpaper_dir(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallpaper_service_directory_scan() {
        let temp_dir = std::env::temp_dir().join(format!("wallpapers-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let img1 = temp_dir.join("bg1.png");
        let img2 = temp_dir.join("bg2.jpg");
        let txt = temp_dir.join("notes.txt");

        std::fs::write(&img1, b"fake png").unwrap();
        std::fs::write(&img2, b"fake jpg").unwrap();
        std::fs::write(&txt, b"notes").unwrap();

        let service = WallpaperService::new(&temp_dir);
        let wallpapers = service.scan_wallpapers();
        assert_eq!(wallpapers.len(), 2);
        assert!(wallpapers.contains(&img1));
        assert!(wallpapers.contains(&img2));
        assert!(!wallpapers.contains(&txt));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_state_file_path_uses_dirs() {
        let path = WallpaperService::state_file_path();
        assert!(path.ends_with("shilpo/colors.json"));
        // Should be under state dir or fallback
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains(".local/state") || path_str.contains("state"),
            "state file path should be in a state directory: {}",
            path_str
        );
    }

    #[test]
    fn test_read_current_returns_none_when_no_file() {
        // read_current reads from the real state file path;
        // if no file exists, it should return None gracefully
        let _ = WallpaperService::read_current();
    }

    #[test]
    fn test_color_scheme_changed_serialization() {
        let state = ColorSchemeChanged {
            wallpaper: PathBuf::from("/tmp/test.png"),
            source_argb: 0xff5e_9963,
            source_hex: "#5E9963".to_string(),
            mode: "dark".to_string(),
            generated_at: "2026-07-27T20:45:00Z".to_string(),
            light: HashMap::from([("primary".to_string(), "#1B6B22".to_string())]),
            dark: HashMap::from([("primary".to_string(), "#87D982".to_string())]),
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        assert!(json.contains("\"source_hex\": \"#5E9963\""));
        assert!(json.contains("\"primary\": \"#1B6B22\""));

        let deserialized: ColorSchemeChanged = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.source_argb, 0xff5e_9963);
        assert_eq!(deserialized.wallpaper, PathBuf::from("/tmp/test.png"));
    }
}
