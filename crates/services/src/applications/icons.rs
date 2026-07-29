use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

static ICON_CACHE: std::sync::LazyLock<Mutex<std::collections::HashMap<String, Option<PathBuf>>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

// Prefer a vector source. If a theme only provides bitmaps, start with enough
// pixels to downsample cleanly on common HiDPI output scales.
const APP_ICON_SOURCE_SIZE: u16 = 48;

/// Clears the in-memory icon-name cache.
pub fn clear_icon_cache() {
    if let Ok(mut cache) = ICON_CACHE.lock() {
        cache.clear();
    }
}

/// Resolve an application icon using the freedesktop icon-theme specification.
///
/// Theme inheritance, `index.theme` directory sizing, PNG/SVG preference,
/// XDG data paths, hicolor fallback, and pixmaps fallback are delegated to
/// `freedesktop-icons`, matching the behavior used by desktop environments.
pub fn lookup_icon(name: &str) -> Option<PathBuf> {
    let name = name.trim();
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

    let theme = freedesktop_icons::default_theme_gtk();
    let resolved = theme
        .as_deref()
        .and_then(|theme| {
            freedesktop_icons::lookup(name)
                .with_theme(theme)
                .with_size(APP_ICON_SOURCE_SIZE)
                .force_svg()
                .with_cache()
                .find()
        })
        .or_else(|| {
            freedesktop_icons::lookup(name)
                .with_size(APP_ICON_SOURCE_SIZE)
                .force_svg()
                .with_cache()
                .find()
        });

    if let Ok(mut cache) = ICON_CACHE.lock() {
        cache.insert(name.to_string(), resolved.clone());
    }
    resolved
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
        assert_eq!(lookup_icon(""), None);
    }

    #[test]
    fn preserves_existing_absolute_icon_paths() {
        assert_eq!(lookup_icon("/etc/hosts"), Some(PathBuf::from("/etc/hosts")));
    }
}
