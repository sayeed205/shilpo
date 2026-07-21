use std::path::{Path, PathBuf};

const SIZE_DIRS: &[&str] = &[
    "scalable", "48x48", "64x64", "128x128", "256x256", "32x32", "24x24", "22x22", "16x16",
];

const EXTENSIONS: &[&str] = &["svg", "png"];

/// Resolves an icon name (e.g. "firefox") to an actual filesystem path.
pub fn lookup_icon(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }

    let path = Path::new(name);
    if path.is_absolute() {
        return path.exists().then(|| path.to_path_buf());
    }

    // Search hicolor and pixmaps
    for base in icon_theme_base_dirs() {
        let hicolor = base.join("hicolor");
        if let Some(found) = search_theme_dir(&hicolor, name) {
            return Some(found);
        }
    }

    // Fallback to /usr/share/pixmaps
    for ext in EXTENSIONS {
        let pixmap = PathBuf::from(format!("/usr/share/pixmaps/{}.{}", name, ext));
        if pixmap.exists() {
            return Some(pixmap);
        }
    }

    None
}

fn search_theme_dir(theme_dir: &Path, name: &str) -> Option<PathBuf> {
    for size in SIZE_DIRS {
        let apps_dir = theme_dir.join(size).join("apps");
        for ext in EXTENSIONS {
            let candidate = apps_dir.join(format!("{}.{}", name, ext));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn icon_theme_base_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".icons"));
        dirs.push(home.join(".local/share/icons"));
    }

    if let Some(data_dirs) = std::env::var_os("XDG_DATA_DIRS") {
        for dir in std::env::split_paths(&data_dirs) {
            dirs.push(dir.join("icons"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/local/share/icons"));
        dirs.push(PathBuf::from("/usr/share/icons"));
    }

    dirs
}
