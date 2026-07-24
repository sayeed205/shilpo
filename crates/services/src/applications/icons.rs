use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

static ICON_CACHE: std::sync::LazyLock<Mutex<HashMap<String, Option<PathBuf>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

const THEMES: &[&str] = &[
    "hicolor",
    "Papirus",
    "Papirus-Dark",
    "Adwaita",
    "breeze",
    "breeze-dark",
    "Yaru",
    "gnome",
];

const SIZE_DIRS: &[&str] = &[
    "scalable", "48x48", "64x64", "128x128", "256x256", "32x32", "24x24", "22x22", "16x16",
];

const EXTENSIONS: &[&str] = &["svg", "png"];

/// Clears the in-memory icon resolution cache.
pub fn clear_icon_cache() {
    if let Ok(mut cache) = ICON_CACHE.lock() {
        cache.clear();
    }
}

/// Resolves an icon name (e.g. "firefox") to an actual filesystem path, cached in memory.
pub fn lookup_icon(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }

    let path = Path::new(name);
    if path.is_absolute() {
        return path.exists().then(|| path.to_path_buf());
    }

    if let Ok(cache) = ICON_CACHE.lock()
        && let Some(cached) = cache.get(name)
    {
        return cached.clone();
    }

    let resolved = resolve_icon_uncached(name);

    if let Ok(mut cache) = ICON_CACHE.lock() {
        cache.insert(name.to_string(), resolved.clone());
    }

    resolved
}

fn resolve_icon_uncached(name: &str) -> Option<PathBuf> {
    for base in icon_theme_base_dirs() {
        for theme in THEMES {
            let theme_dir = base.join(theme);
            if let Some(found) = search_theme_dir(&theme_dir, name) {
                return Some(found);
            }
        }
    }

    // Fallback to /usr/share/pixmaps and Flatpak export paths
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
        dirs.push(home.join(".local/share/flatpak/exports/share/icons"));
    }

    if let Some(data_dirs) = std::env::var_os("XDG_DATA_DIRS") {
        for dir in std::env::split_paths(&data_dirs) {
            dirs.push(dir.join("icons"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/local/share/icons"));
        dirs.push(PathBuf::from("/usr/share/icons"));
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/icons"));
    }

    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_icon_caching_and_clearing() {
        clear_icon_cache();
        let res1 = lookup_icon("nonexistent_icon_shilpo_999");
        assert!(res1.is_none());
        let res2 = lookup_icon("nonexistent_icon_shilpo_999");
        assert!(res2.is_none());

        clear_icon_cache();
        let empty_res = lookup_icon("");
        assert!(empty_res.is_none());
    }
}
