use std::borrow::Cow;

use anyhow::anyhow;
use gpui::{AssetSource, Result, SharedString};

#[cfg(not(target_family = "wasm"))]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../core/assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

#[cfg(not(target_family = "wasm"))]
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        Assets::get(path)
            .map(|file| Some(file.data))
            .ok_or_else(|| anyhow!("could not find asset at path \"{path}\""))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Assets::iter()
            .filter_map(|entry| entry.starts_with(path).then(|| entry.into()))
            .collect())
    }
}

#[cfg(target_family = "wasm")]
mod wasm {
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    use wasm_bindgen_futures::spawn_local;

    use super::*;

    pub struct Assets {
        endpoint: SharedString,
        cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
        pending: Arc<RwLock<HashMap<String, bool>>>,
    }

    impl Assets {
        pub fn new(endpoint: impl Into<SharedString>) -> Self {
            Self {
                endpoint: endpoint.into(),
                cache: Arc::new(RwLock::new(HashMap::new())),
                pending: Arc::new(RwLock::new(HashMap::new())),
            }
        }
    }

    impl Default for Assets {
        fn default() -> Self {
            Self::new("")
        }
    }

    impl AssetSource for Assets {
        fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
            if path.is_empty() {
                return Ok(None);
            }
            if !path.starts_with("icons/") || !path.ends_with(".svg") {
                return Ok(None);
            }
            if let Ok(cache) = self.cache.read()
                && let Some(data) = cache.get(path)
            {
                return Ok(Some(Cow::Owned(data.clone())));
            }
            if self
                .pending
                .read()
                .map(|pending| pending.contains_key(path))
                .unwrap_or(false)
            {
                return Err(anyhow!("Wasm asset is still loading"));
            }

            if let Ok(mut pending) = self.pending.write() {
                pending.insert(path.to_string(), true);
            }
            let url = format!("{}/assets/{path}", self.endpoint);
            let path = path.to_string();
            let cache = Arc::clone(&self.cache);
            let pending = Arc::clone(&self.pending);
            spawn_local(async move {
                if let Ok(response) = reqwest::get(&url).await
                    && response.status().is_success()
                    && let Ok(bytes) = response.bytes().await
                    && let Ok(mut cache) = cache.write()
                {
                    cache.insert(path.clone(), bytes.to_vec());
                }
                if let Ok(mut pending) = pending.write() {
                    pending.remove(&path);
                }
            });
            Err(anyhow!("Wasm asset is loading"))
        }

        fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
            Ok(Vec::new())
        }
    }
}

#[cfg(target_family = "wasm")]
pub use wasm::Assets;
