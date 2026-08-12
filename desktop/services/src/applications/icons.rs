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
    lookup_icon_internal(name, 0)
}

fn lookup_icon_internal(name: &str, depth: usize) -> Option<PathBuf> {
    if depth > 4 {
        return None;
    }
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let clean_name = name.strip_prefix("file://").unwrap_or(name);
    let path = Path::new(clean_name);
    if path.is_absolute() {
        return path.exists().then(|| path.to_path_buf());
    }

    if let Ok(mut cache) = ICON_CACHE.lock() {
        if let Some(cached) = cache.get(clean_name) {
            return cached.clone();
        }
        // Insert transient entry to break recursion cycles
        cache.insert(clean_name.to_string(), None);
    }

    let mut search_names = vec![
        clean_name.to_string(),
        clean_name.to_lowercase(),
        clean_name.to_lowercase().replace(' ', "-"),
        format!("{}-stable", clean_name.to_lowercase().replace(' ', "-")),
    ];
    if clean_name.to_lowercase().contains("chrome") {
        search_names.push("google-chrome".to_string());
        search_names.push("google-chrome-stable".to_string());
    }
    if clean_name.to_lowercase().contains("niri") {
        search_names.push("niri".to_string());
        search_names.push("org.freedesktop.impl.portal.desktop.niri".to_string());
    }

    let theme = freedesktop_icons::default_theme_gtk();
    let mut resolved = None;

    for s_name in &search_names {
        resolved = theme
            .as_deref()
            .and_then(|t| {
                freedesktop_icons::lookup(s_name)
                    .with_theme(t)
                    .with_size(APP_ICON_SOURCE_SIZE)
                    .force_svg()
                    .with_cache()
                    .find()
                    .or_else(|| {
                        freedesktop_icons::lookup(s_name)
                            .with_theme(t)
                            .with_size(APP_ICON_SOURCE_SIZE)
                            .with_cache()
                            .find()
                    })
            })
            .or_else(|| {
                freedesktop_icons::lookup(s_name)
                    .with_size(APP_ICON_SOURCE_SIZE)
                    .with_cache()
                    .find()
            });

        if resolved.is_some() {
            break;
        }
    }

    if resolved.is_none()
        && depth == 0
        && let Ok(apps) = super::list_applications()
    {
        let search_clean = clean_name.to_lowercase().replace(' ', "-");
        for app in apps {
            let name_clean = app.name.to_lowercase().replace(' ', "-");
            let stem_clean = app
                .desktop_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase()
                .replace(' ', "-");
            let icon_hint = app.icon.as_deref().unwrap_or("").to_lowercase();

            if name_clean == search_clean || stem_clean == search_clean || icon_hint == search_clean
            {
                if let Some(path) = app.icon_path {
                    resolved = Some(path);
                    break;
                }
                if let Some(ref icon) = app.icon
                    && icon.to_lowercase() != search_clean
                    && let Some(path) = lookup_icon_internal(icon, depth + 1)
                {
                    resolved = Some(path);
                    break;
                }
            }
        }
    }

    if let Ok(mut cache) = ICON_CACHE.lock() {
        cache.insert(clean_name.to_string(), resolved.clone());
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
