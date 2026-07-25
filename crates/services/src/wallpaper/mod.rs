use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Desktop Wallpaper Service managing directory scanning, active wallpaper selection, and random picking.
#[derive(Debug, Clone)]
pub struct WallpaperService {
    wallpaper_dir: PathBuf,
    active_wallpaper: Arc<Mutex<Option<PathBuf>>>,
}

impl WallpaperService {
    /// Creates a new WallpaperService for the specified directory.
    pub fn new(wallpaper_dir: impl Into<PathBuf>) -> Self {
        Self {
            wallpaper_dir: wallpaper_dir.into(),
            active_wallpaper: Arc::new(Mutex::new(None)),
        }
    }

    /// Returns the default wallpaper directory path (`~/Pictures/Wallpapers`).
    pub fn default_wallpaper_dir() -> PathBuf {
        if let Ok(home_str) = std::env::var("HOME") {
            let home = PathBuf::from(home_str);
            let pictures_dir = home.join("Pictures").join("Wallpapers");
            if pictures_dir.exists() {
                return pictures_dir;
            }
            let config_dir = home.join(".config").join("shilpo").join("wallpapers");
            if config_dir.exists() {
                return config_dir;
            }
            pictures_dir
        } else {
            PathBuf::from("/usr/share/backgrounds")
        }
    }

    /// Scans the wallpaper directory for supported image files (.png, .jpg, .jpeg, .webp).
    pub fn scan_wallpapers(&self) -> Vec<PathBuf> {
        let mut wallpapers = Vec::new();
        if !self.wallpaper_dir.exists() {
            return wallpapers;
        }

        if let Ok(entries) = std::fs::read_dir(&self.wallpaper_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && self.is_supported_image(&path) {
                    wallpapers.push(path);
                }
            }
        }

        wallpapers.sort();
        wallpapers
    }

    /// Checks if a file has a supported image extension.
    fn is_supported_image(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp")
        } else {
            false
        }
    }

    /// Returns the currently active wallpaper path, if any.
    pub fn active_wallpaper(&self) -> Option<PathBuf> {
        self.active_wallpaper.lock().unwrap().clone()
    }

    /// Sets the active wallpaper to the specified path.
    pub fn set_wallpaper(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if !path.exists() {
            bail!("Wallpaper file does not exist: {}", path.display());
        }

        if !self.is_supported_image(path) {
            bail!("Unsupported image format for wallpaper: {}", path.display());
        }

        let mut active = self.active_wallpaper.lock().unwrap();
        *active = Some(path.to_path_buf());
        tracing::info!(path = %path.display(), "Active wallpaper updated");
        Ok(())
    }

    /// Selects and sets a random wallpaper from the scanned directory.
    pub fn set_random_wallpaper(&self) -> Result<PathBuf> {
        let wallpapers = self.scan_wallpapers();
        if wallpapers.is_empty() {
            bail!(
                "No wallpapers found in directory: {}",
                self.wallpaper_dir.display()
            );
        }

        let rng_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as usize;

        let index = rng_seed % wallpapers.len();
        let selected = wallpapers[index].clone();
        self.set_wallpaper(&selected)?;
        Ok(selected)
    }
}

impl Default for WallpaperService {
    fn default() -> Self {
        Self::new(Self::default_wallpaper_dir())
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
    fn test_wallpaper_service_random_selection() {
        let temp_dir =
            std::env::temp_dir().join(format!("wallpapers-random-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let img1 = temp_dir.join("wallpaper.webp");
        std::fs::write(&img1, b"fake webp").unwrap();

        let service = WallpaperService::new(&temp_dir);
        let selected = service.set_random_wallpaper().unwrap();
        assert_eq!(selected, img1);
        assert_eq!(service.active_wallpaper(), Some(img1));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
