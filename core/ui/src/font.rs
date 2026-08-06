use gpui::{App, Global, ReadGlobal, SharedString};
use std::sync::{Arc, RwLock};
use std::time::Instant;

#[derive(Default)]
struct FontFamilyCacheState {
    loaded_at: Option<Instant>,
    font_families: Vec<SharedString>,
}

/// Thread-safe global cache for operating system font families.
#[derive(Default)]
pub struct FontFamilyCache {
    state: Arc<RwLock<FontFamilyCacheState>>,
}

#[derive(Default)]
struct GlobalFontFamilyCache(Arc<FontFamilyCache>);

impl Global for GlobalFontFamilyCache {}

impl FontFamilyCache {
    /// Initializes the global font family cache.
    pub fn init_global(cx: &mut App) {
        if !cx.has_global::<GlobalFontFamilyCache>() {
            cx.default_global::<GlobalFontFamilyCache>();
        }
    }

    /// Returns the global font family cache handle.
    pub fn global(cx: &App) -> Arc<Self> {
        if cx.has_global::<GlobalFontFamilyCache>() {
            GlobalFontFamilyCache::global(cx).0.clone()
        } else {
            Arc::new(Self::default())
        }
    }

    /// Returns the list of font family names available on the operating system.
    pub fn list_font_families(&self, cx: &App) -> Vec<SharedString> {
        if let Ok(read) = self.state.read()
            && read.loaded_at.is_some()
        {
            return read.font_families.clone();
        }

        if let Ok(mut lock) = self.state.write() {
            lock.font_families = cx
                .text_system()
                .all_font_names()
                .into_iter()
                .map(SharedString::from)
                .collect();
            lock.loaded_at = Some(Instant::now());
            lock.font_families.clone()
        } else {
            Vec::new()
        }
    }
}
