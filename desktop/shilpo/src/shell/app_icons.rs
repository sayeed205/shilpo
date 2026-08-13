use std::{
    collections::HashMap,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use gpui::{
    Image, ImageFormat, ImageSource, IntoElement, ObjectFit, ParentElement, Styled, StyledImage,
    div, img,
};
use image::{DynamicImage, GenericImageView as _, RgbaImage, imageops::FilterType};
use shilpo_services::Application;
use shilpo_ui::StyledExt;

pub(crate) fn app_icon(
    icon_path: Option<PathBuf>,
    fallback_label: &str,
    size: gpui::Pixels,
    scale_factor: f32,
    background: gpui::Hsla,
    foreground: gpui::Hsla,
) -> gpui::AnyElement {
    if let Some(icon_path) = icon_path {
        let target_size = icon_device_pixels(size.as_f32(), scale_factor);
        let image = rasterized_app_icon(&icon_path, target_size)
            .map(img)
            .unwrap_or_else(|| img(ImageSource::from(icon_path)));
        div()
            .w(size)
            .h(size)
            .flex_none()
            .items_center()
            .justify_center()
            .child(image.w(size).h(size).object_fit(ObjectFit::Contain))
            .into_any_element()
    } else {
        let initial = fallback_label
            .chars()
            .find(|character| character.is_alphanumeric())
            .map(|character| character.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string());
        div()
            .w(size)
            .h(size)
            .flex_none()
            .items_center()
            .justify_center()
            .rounded_xl()
            .bg(background)
            .text_color(foreground)
            .font_semibold()
            .shadow_md()
            .child(initial)
            .into_any_element()
    }
}

type RasterCache = HashMap<(PathBuf, u32), Option<Arc<Image>>>;

static RASTERIZED_ICON_CACHE: LazyLock<Mutex<RasterCache>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn normalize_app_key(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".desktop")
        .to_ascii_lowercase()
        .replace('_', "-")
}

pub(crate) fn build_app_icon_index(applications: Vec<Application>) -> HashMap<String, PathBuf> {
    let mut icons = HashMap::new();
    for app in applications {
        let icon_path = app.icon_path.clone().or_else(|| {
            app.icon
                .as_deref()
                .and_then(shilpo_services::applications::icons::lookup_icon)
        });
        let Some(icon_path) = icon_path else {
            continue;
        };
        let mut aliases = vec![normalize_app_key(&app.name)];
        if let Some(icon) = app.icon.as_deref() {
            aliases.push(normalize_app_key(icon));
        }
        if let Some(stem) = app.desktop_file.file_stem().and_then(|stem| stem.to_str()) {
            aliases.push(normalize_app_key(stem));
        }
        if let Some(program) = app.exec.split_whitespace().next()
            && let Some(program) = Path::new(program)
                .file_name()
                .and_then(|name| name.to_str())
        {
            aliases.push(normalize_app_key(program));
        }
        if let Some(startup_class) = read_startup_wm_class(&app.desktop_file) {
            aliases.push(normalize_app_key(&startup_class));
        }
        for alias in aliases {
            for key in alias_variants(&alias) {
                icons.entry(key).or_insert_with(|| icon_path.clone());
            }
        }
    }
    icons
}

fn read_startup_wm_class(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_desktop_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if in_desktop_entry && let Some(value) = line.strip_prefix("StartupWMClass=") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn alias_variants(value: &str) -> Vec<String> {
    let value = normalize_app_key(value);
    if value.is_empty() {
        return Vec::new();
    }
    let stripped = value.trim_start_matches('@');
    let segments: Vec<&str> = stripped
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let hyphen_segments: Vec<&str> = segments
        .iter()
        .flat_map(|segment| segment.split('-'))
        .filter(|segment| {
            !segment.is_empty() && !matches!(*segment, "desktop" | "app" | "bin" | "electron")
        })
        .collect();
    let joined = segments.join("-");
    let no_suffix = |value: &str| {
        value
            .trim_end_matches("-desktop")
            .trim_end_matches("-app")
            .trim_end_matches("-electron")
            .trim_end_matches("-bin")
            .to_string()
    };

    let mut variants = vec![value.clone(), stripped.to_string(), joined.clone()];
    variants.extend(segments.iter().map(|segment| (*segment).to_string()));
    variants.extend(hyphen_segments.iter().map(|segment| (*segment).to_string()));
    variants.push(no_suffix(&value));
    variants.push(no_suffix(&joined));
    if let Some(short) = value.rsplit('.').next() {
        variants.push(short.to_string());
        variants.push(no_suffix(short));
    }
    variants
        .into_iter()
        .map(|variant| variant.replace('_', "-"))
        .filter(|variant| !variant.is_empty())
        .collect()
}

pub(crate) fn resolve_app_icon_path(
    app_id: Option<&str>,
    app_icons: &HashMap<String, PathBuf>,
) -> Option<PathBuf> {
    resolve_app_icon_path_with_lookup(app_id, app_icons, |candidate| {
        shilpo_services::applications::icons::lookup_icon(candidate)
    })
}

fn theme_icon_candidates(value: &str) -> Vec<String> {
    let key = normalize_app_key(value).replace('_', "-");
    if key.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    if let Some(short) = key.strip_prefix("jetbrains-") {
        candidates.push(short.to_string());
    }
    if let Some(short) = key.rsplit('.').next()
        && short != key
    {
        candidates.push(short.to_string());
    }
    candidates.push(key.clone());
    candidates.extend(alias_variants(&key));
    candidates.dedup();
    candidates
}

fn resolve_app_icon_path_with_lookup(
    app_id: Option<&str>,
    app_icons: &HashMap<String, PathBuf>,
    lookup_icon: impl Fn(&str) -> Option<PathBuf>,
) -> Option<PathBuf> {
    let key = normalize_app_key(app_id?);
    if key.is_empty() {
        return None;
    }
    indexed_app_icon(&key, app_icons).or_else(|| {
        theme_icon_candidates(&key)
            .into_iter()
            .find_map(|candidate| {
                lookup_icon(&candidate).filter(|path| is_application_icon_path(path))
            })
    })
}

fn indexed_app_icon(key: &str, app_icons: &HashMap<String, PathBuf>) -> Option<PathBuf> {
    app_icons
        .get(key)
        .cloned()
        .or_else(|| {
            alias_variants(key)
                .into_iter()
                .find_map(|candidate| app_icons.get(&candidate).cloned())
        })
        .or_else(|| {
            app_icons
                .iter()
                .filter(|(alias, _)| {
                    alias.ends_with(key)
                        || key.ends_with(alias.as_str())
                        || alias.starts_with(&format!("{key}-"))
                })
                .max_by_key(|(alias, _)| alias.len())
                .map(|(_, path)| path.clone())
        })
}

fn is_application_icon_path(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|component| matches!(component, "apps" | "pixmaps"))
    })
}

pub(crate) fn icon_device_pixels(logical_size: f32, scale_factor: f32) -> u32 {
    (logical_size * scale_factor).round().max(1.0) as u32
}

pub(crate) fn rasterized_app_icon(path: &Path, target_size: u32) -> Option<Arc<Image>> {
    let cache_key = (path.to_path_buf(), target_size);
    if let Ok(cache) = RASTERIZED_ICON_CACHE.lock()
        && let Some(cached) = cache.get(&cache_key)
    {
        return cached.clone();
    }

    let rasterized = std::fs::read(path)
        .ok()
        .and_then(|bytes| rasterize_icon_bytes(path, &bytes, target_size))
        .map(|bytes| Arc::new(Image::from_bytes(ImageFormat::Png, bytes)));

    if let Ok(mut cache) = RASTERIZED_ICON_CACHE.lock() {
        cache.insert(cache_key, rasterized.clone());
    }
    rasterized
}

fn rasterize_icon_bytes(path: &Path, bytes: &[u8], target_size: u32) -> Option<Vec<u8>> {
    if target_size == 0 {
        return None;
    }

    let is_svg = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));

    if is_svg {
        rasterize_svg(bytes, target_size)
    } else {
        rasterize_bitmap(bytes, target_size)
    }
}

fn rasterize_svg(bytes: &[u8], target_size: u32) -> Option<Vec<u8>> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(bytes, &options).ok()?;
    let source_size = tree.size();
    let scale =
        (target_size as f32 / source_size.width()).min(target_size as f32 / source_size.height());
    let offset_x = (target_size as f32 - source_size.width() * scale) / 2.0;
    let offset_y = (target_size as f32 - source_size.height() * scale) / 2.0;
    let transform =
        resvg::tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, offset_x, offset_y);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(target_size, target_size)?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

fn rasterize_bitmap(bytes: &[u8], target_size: u32) -> Option<Vec<u8>> {
    let source = image::load_from_memory(bytes).ok()?;
    let (source_width, source_height) = source.dimensions();
    if source_width == 0 || source_height == 0 {
        return None;
    }

    let scale =
        (target_size as f32 / source_width as f32).min(target_size as f32 / source_height as f32);
    let width = (source_width as f32 * scale)
        .round()
        .clamp(1.0, target_size as f32) as u32;
    let height = (source_height as f32 * scale)
        .round()
        .clamp(1.0, target_size as f32) as u32;
    let resized = source.resize_exact(width, height, FilterType::Lanczos3);
    let mut canvas = RgbaImage::new(target_size, target_size);
    image::imageops::overlay(
        &mut canvas,
        &resized,
        i64::from((target_size - width) / 2),
        i64::from((target_size - height) / 2),
    );

    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(canvas)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .ok()?;
    Some(encoded.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_normalizes_desktop_ids_and_rejects_empty_ids() {
        let path = PathBuf::from("/tmp/example.svg");
        let icons = HashMap::from([(String::from("org.example.editor"), path.clone())]);
        assert_eq!(
            resolve_app_icon_path(Some("org.example.Editor.desktop"), &icons),
            Some(path)
        );
        assert_eq!(resolve_app_icon_path(Some(""), &icons), None);
    }

    #[test]
    fn resolver_handles_scoped_electron_and_suffix_app_ids() {
        let path = PathBuf::from("/tmp/example.svg");
        let icons = HashMap::from([(String::from("suite-desktop"), path.clone())]);
        assert_eq!(
            resolve_app_icon_path(Some("@vendor/suite-desktop"), &icons),
            Some(path)
        );
    }

    #[test]
    fn resolver_matches_short_startup_ids_to_hyphenated_desktop_icons() {
        let path = PathBuf::from("/tmp/rustrover.svg");
        let icons = HashMap::from([(String::from("jetbrains-rustrover"), path.clone())]);
        assert!(alias_variants("jetbrains-rustrover").contains(&"rustrover".to_string()));
        assert_eq!(
            resolve_app_icon_path_with_lookup(Some("rustrover"), &icons, |_| None),
            Some(path)
        );
    }

    #[test]
    fn themed_short_name_repairs_an_id_without_an_exact_desktop_entry() {
        let bundled = PathBuf::from("/icons/jetbrains-rustrover.svg");
        let themed = PathBuf::from("/themes/WhiteSur-dark/apps/scalable/rustrover.svg");
        let icons = HashMap::from([(String::from("jetbrains-rustrover"), bundled)]);

        let resolved = resolve_app_icon_path_with_lookup(
            Some("com.jetbrains.rustrover"),
            &icons,
            |candidate| (candidate == "rustrover").then(|| themed.clone()),
        );

        assert_eq!(resolved, Some(themed));
    }

    #[test]
    fn reverse_domain_app_id_preserves_the_desktop_entry_icon() {
        let bundled = PathBuf::from("/apps/zed.png");
        let themed = PathBuf::from("/themes/WhiteSur-dark/apps/scalable/zed.svg");
        let icons = HashMap::from([(String::from("dev.zed.zed"), bundled.clone())]);

        let resolved =
            resolve_app_icon_path_with_lookup(Some("dev.zed.Zed"), &icons, |candidate| {
                (candidate == "zed").then(|| themed.clone())
            });

        assert_eq!(resolved, Some(bundled));
    }

    #[test]
    fn underscore_app_id_keeps_its_distinct_desktop_entry_icon() {
        let app = PathBuf::from("/apps/antigravity.png");
        let tools = PathBuf::from("/apps/antigravity_tools.png");
        let icons = HashMap::from([
            (String::from("antigravity"), app.clone()),
            (String::from("antigravity-tools"), tools.clone()),
        ]);

        let resolved =
            resolve_app_icon_path_with_lookup(Some("Antigravity_tools"), &icons, |candidate| {
                (candidate == "antigravity").then(|| app.clone())
            });

        assert_eq!(resolved, Some(tools));
    }

    #[test]
    fn themed_icon_fills_a_no_display_desktop_entry_gap() {
        let themed = PathBuf::from("/themes/WhiteSur-dark/apps/scalable/org.quickshell.svg");

        let resolved = resolve_app_icon_path_with_lookup(
            Some("org.quickshell"),
            &HashMap::new(),
            |candidate| (candidate == "org.quickshell").then(|| themed.clone()),
        );

        assert_eq!(resolved, Some(themed));
    }

    #[test]
    fn icon_size_tracks_fractional_output_scale() {
        assert_eq!(icon_device_pixels(18.0, 1.0), 18);
        assert_eq!(icon_device_pixels(18.0, 1.5), 27);
        assert_eq!(icon_device_pixels(18.0, 2.0), 36);
    }

    #[test]
    fn svg_icons_are_rasterized_at_the_exact_device_size() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
            <circle cx="32" cy="32" r="28" fill="#ff0000"/>
        </svg>"##;
        let png = rasterize_icon_bytes(Path::new("test.svg"), svg, 27).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();

        assert_eq!(decoded.dimensions(), (27, 27));
    }

    #[test]
    fn non_square_bitmaps_keep_their_aspect_ratio() {
        let source =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(64, 32, image::Rgba([255, 0, 0, 255])));
        let mut encoded = Cursor::new(Vec::new());
        source
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();

        let png = rasterize_icon_bytes(Path::new("test.png"), &encoded.into_inner(), 27).unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(decoded.dimensions(), (27, 27));
        assert_eq!(decoded.get_pixel(13, 0).0[3], 0);
        assert_eq!(decoded.get_pixel(13, 13).0[3], 255);
        assert_eq!(decoded.get_pixel(13, 26).0[3], 0);
    }
}
