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
    if Path::new(name).components().count() != 1 {
        return None;
    }

    for base in icon_theme_base_dirs() {
        if let Some(found) = search_icon_base(&base, name) {
            return Some(found);
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

fn search_icon_base(base: &Path, name: &str) -> Option<PathBuf> {
    for ext in EXTENSIONS {
        let candidate = base.join(format!("{name}.{ext}"));
        if candidate.exists() {
            return Some(candidate);
        }
    }

    for theme in THEMES {
        let theme_dir = base.join(theme);
        if let Some(found) = search_theme_dir(&theme_dir, name) {
            return Some(found);
        }
    }

    if let Ok(theme_dirs) = std::fs::read_dir(base) {
        for theme_dir in theme_dirs.flatten() {
            let theme_dir = theme_dir.path();
            if theme_dir.is_dir()
                && let Some(found) = search_theme_dir(&theme_dir, name)
            {
                return Some(found);
            }
        }
    }

    None
}

fn search_theme_dir(theme_dir: &Path, name: &str) -> Option<PathBuf> {
    for size in SIZE_DIRS {
        let app_dirs = [
            theme_dir.join(size).join("apps"),
            theme_dir.join("apps").join(size),
        ];
        for apps_dir in app_dirs {
            for ext in EXTENSIONS {
                let candidate = apps_dir.join(format!("{name}.{ext}"));
                if candidate.exists() {
                    return Some(candidate);
                }
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_icon_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("shilpo-icon-test-{}-{nonce}", std::process::id()))
    }

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

    #[test]
    fn searches_direct_icon_roots_and_alternate_theme_layouts() {
        let root = temporary_icon_root();
        std::fs::create_dir_all(&root).unwrap();

        let direct_icon = root.join("direct-app.svg");
        std::fs::write(&direct_icon, "<svg/>").unwrap();
        assert_eq!(
            search_icon_base(&root, "direct-app"),
            Some(direct_icon.clone())
        );
        std::fs::remove_file(direct_icon).unwrap();

        let themed_icon = root
            .join("CustomTheme")
            .join("apps")
            .join("scalable")
            .join("themed-app.svg");
        std::fs::create_dir_all(themed_icon.parent().unwrap()).unwrap();
        std::fs::write(&themed_icon, "<svg/>").unwrap();
        assert_eq!(search_icon_base(&root, "themed-app"), Some(themed_icon));

        std::fs::remove_dir_all(root).unwrap();
    }
}
