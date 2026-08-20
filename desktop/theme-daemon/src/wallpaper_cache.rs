//! In-memory LRU wallpaper analysis cache owned by shilpo-theme-daemon.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use shilpo_m3e::theme::SchemeVariant;

/// Cache key uniquely identifying an analyzed wallpaper image under a specific scheme variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WallpaperCacheKey {
    pub canonical_path: PathBuf,
    pub mtime: SystemTime,
    pub scheme_variant: SchemeVariant,
}

/// The expensive extracted analysis result of a wallpaper image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperAnalysis {
    pub seed: u32,
    pub detected_variant: SchemeVariant,
}

/// Bounded 32-entry in-memory LRU cache for wallpaper analysis results.
#[derive(Debug)]
pub struct WallpaperAnalysisCache {
    capacity: usize,
    entries: HashMap<WallpaperCacheKey, WallpaperAnalysis>,
    order: VecDeque<WallpaperCacheKey>,
}

impl WallpaperAnalysisCache {
    fn new() -> Self {
        Self {
            capacity: 32,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    #[cfg(test)]
    fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&mut self, key: &WallpaperCacheKey) -> Option<WallpaperAnalysis> {
        if let Some(analysis) = self.entries.get(key) {
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                let k = self.order.remove(pos).unwrap();
                self.order.push_back(k);
            }
            Some(analysis.clone())
        } else {
            None
        }
    }

    pub fn insert(&mut self, key: WallpaperCacheKey, analysis: WallpaperAnalysis) {
        if self.capacity == 0 {
            return;
        }
        if self.entries.contains_key(&key) {
            self.entries.insert(key.clone(), analysis);
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                let k = self.order.remove(pos).unwrap();
                self.order.push_back(k);
            }
        } else {
            if self.entries.len() >= self.capacity {
                while let Some(old_key) = self.order.pop_front() {
                    if self.entries.remove(&old_key).is_some() {
                        break;
                    }
                }
            }
            self.entries.insert(key.clone(), analysis);
            self.order.push_back(key);
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Default for WallpaperAnalysisCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Perform canonicalization, regular-file checks, and metadata extraction to form a [`WallpaperCacheKey`].
pub fn create_wallpaper_cache_key(
    path: &Path,
    active_scheme_variant: SchemeVariant,
) -> Result<(PathBuf, WallpaperCacheKey), String> {
    let canonical = fs::canonicalize(path).map_err(|e| {
        format!(
            "Failed to canonicalize wallpaper path {}: {e}",
            path.display()
        )
    })?;

    let metadata = fs::metadata(&canonical).map_err(|e| {
        format!(
            "Failed to read metadata for wallpaper {}: {e}",
            canonical.display()
        )
    })?;

    if !metadata.is_file() {
        return Err(format!(
            "Wallpaper path is not a regular file: {}",
            canonical.display()
        ));
    }

    let mtime = metadata.modified().map_err(|e| {
        format!(
            "Failed to read modification time for {}: {e}",
            canonical.display()
        )
    })?;

    let key = WallpaperCacheKey {
        canonical_path: canonical.clone(),
        mtime,
        scheme_variant: active_scheme_variant,
    };

    Ok((canonical, key))
}

/// Safely interact with a shared cache without panicking on lock poisoning.
pub fn with_cache<F, R>(cache: &Mutex<WallpaperAnalysisCache>, f: F) -> R
where
    F: FnOnce(&mut WallpaperAnalysisCache) -> R,
{
    match cache.lock() {
        Ok(mut guard) => f(&mut guard),
        Err(poisoned) => {
            tracing::error!("Wallpaper cache lock poisoned, accessing inner cache guard");
            let mut guard = poisoned.into_inner();
            f(&mut guard)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_1_exact_key_hit_returns_stored_analysis() {
        let mut cache = WallpaperAnalysisCache::with_capacity(32);
        let key = WallpaperCacheKey {
            canonical_path: PathBuf::from("/tmp/wallpaper.png"),
            mtime: SystemTime::UNIX_EPOCH,
            scheme_variant: SchemeVariant::Auto,
        };
        let analysis = WallpaperAnalysis {
            seed: 0xff123456,
            detected_variant: SchemeVariant::Expressive,
        };

        cache.insert(key.clone(), analysis.clone());
        let hit = cache.get(&key);
        assert_eq!(hit, Some(analysis));
    }

    #[test]
    fn test_2_hit_refreshes_lru_recency() {
        let mut cache = WallpaperAnalysisCache::with_capacity(2);

        let key1 = WallpaperCacheKey {
            canonical_path: PathBuf::from("/tmp/wp1.png"),
            mtime: SystemTime::UNIX_EPOCH,
            scheme_variant: SchemeVariant::Auto,
        };
        let key2 = WallpaperCacheKey {
            canonical_path: PathBuf::from("/tmp/wp2.png"),
            mtime: SystemTime::UNIX_EPOCH,
            scheme_variant: SchemeVariant::Auto,
        };
        let key3 = WallpaperCacheKey {
            canonical_path: PathBuf::from("/tmp/wp3.png"),
            mtime: SystemTime::UNIX_EPOCH,
            scheme_variant: SchemeVariant::Auto,
        };

        cache.insert(
            key1.clone(),
            WallpaperAnalysis {
                seed: 1,
                detected_variant: SchemeVariant::Auto,
            },
        );
        cache.insert(
            key2.clone(),
            WallpaperAnalysis {
                seed: 2,
                detected_variant: SchemeVariant::Auto,
            },
        );

        // Access key1 to make it most recent
        assert!(cache.get(&key1).is_some());

        // Insert key3 -> should evict key2 (since key1 was refreshed)
        cache.insert(
            key3.clone(),
            WallpaperAnalysis {
                seed: 3,
                detected_variant: SchemeVariant::Auto,
            },
        );

        assert!(cache.get(&key1).is_some(), "key1 should remain in cache");
        assert!(cache.get(&key2).is_none(), "key2 should have been evicted");
        assert!(cache.get(&key3).is_some(), "key3 should be in cache");
    }

    #[test]
    fn test_3_eviction_at_capacity_32() {
        let mut cache = WallpaperAnalysisCache::with_capacity(32);
        let mut keys = Vec::new();

        for i in 0..32 {
            let key = WallpaperCacheKey {
                canonical_path: PathBuf::from(format!("/tmp/wp_{i}.png")),
                mtime: SystemTime::UNIX_EPOCH,
                scheme_variant: SchemeVariant::Auto,
            };
            cache.insert(
                key.clone(),
                WallpaperAnalysis {
                    seed: i as u32,
                    detected_variant: SchemeVariant::Auto,
                },
            );
            keys.push(key);
        }

        assert_eq!(cache.len(), 32);

        // Insert 33rd key
        let key_33 = WallpaperCacheKey {
            canonical_path: PathBuf::from("/tmp/wp_32.png"),
            mtime: SystemTime::UNIX_EPOCH,
            scheme_variant: SchemeVariant::Auto,
        };
        cache.insert(
            key_33.clone(),
            WallpaperAnalysis {
                seed: 32,
                detected_variant: SchemeVariant::Auto,
            },
        );

        assert_eq!(cache.len(), 32);
        assert!(
            cache.get(&keys[0]).is_none(),
            "key 0 should be evicted as LRU"
        );
        assert!(cache.get(&keys[1]).is_some(), "key 1 should remain");
        assert!(cache.get(&key_33).is_some(), "key 33 should be present");
    }

    #[test]
    fn test_4_variant_miss() {
        let mut cache = WallpaperAnalysisCache::with_capacity(32);
        let path = PathBuf::from("/tmp/wp.png");

        let key1 = WallpaperCacheKey {
            canonical_path: path.clone(),
            mtime: SystemTime::UNIX_EPOCH,
            scheme_variant: SchemeVariant::Auto,
        };
        let key2 = WallpaperCacheKey {
            canonical_path: path,
            mtime: SystemTime::UNIX_EPOCH,
            scheme_variant: SchemeVariant::Expressive,
        };

        cache.insert(
            key1.clone(),
            WallpaperAnalysis {
                seed: 100,
                detected_variant: SchemeVariant::Auto,
            },
        );

        assert!(cache.get(&key1).is_some());
        assert!(
            cache.get(&key2).is_none(),
            "different variant should be a miss"
        );
    }

    #[test]
    fn test_5_mtime_miss() {
        let mut cache = WallpaperAnalysisCache::with_capacity(32);
        let path = PathBuf::from("/tmp/wp.png");

        let time1 = SystemTime::UNIX_EPOCH;
        let time2 = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);

        let key1 = WallpaperCacheKey {
            canonical_path: path.clone(),
            mtime: time1,
            scheme_variant: SchemeVariant::Auto,
        };
        let key2 = WallpaperCacheKey {
            canonical_path: path,
            mtime: time2,
            scheme_variant: SchemeVariant::Auto,
        };

        cache.insert(
            key1.clone(),
            WallpaperAnalysis {
                seed: 100,
                detected_variant: SchemeVariant::Auto,
            },
        );

        assert!(cache.get(&key1).is_some());
        assert!(cache.get(&key2).is_none(), "changed mtime should be a miss");
    }

    #[test]
    fn test_6_path_alias_canonicalization() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let (canonical1, key1) = create_wallpaper_cache_key(path, SchemeVariant::Auto).unwrap();

        // Create an alias path (e.g. adding ./ component)
        let alias_path = path
            .parent()
            .unwrap()
            .join(".")
            .join(path.file_name().unwrap());
        let (canonical2, key2) =
            create_wallpaper_cache_key(&alias_path, SchemeVariant::Auto).unwrap();

        assert_eq!(canonical1, canonical2);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_7_non_regular_and_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let res_dir = create_wallpaper_cache_key(dir.path(), SchemeVariant::Auto);
        assert!(res_dir.is_err());
        assert!(res_dir.unwrap_err().contains("not a regular file"));

        let missing = dir.path().join("does_not_exist.png");
        let res_missing = create_wallpaper_cache_key(&missing, SchemeVariant::Auto);
        assert!(res_missing.is_err());
    }

    #[test]
    fn test_8_poison_recovery_no_panic() {
        let cache_mutex = Arc::new(Mutex::new(WallpaperAnalysisCache::default()));

        // Poison the lock intentionally in a thread panic
        let c = cache_mutex.clone();
        let _ = std::thread::spawn(move || {
            let _guard = c.lock().unwrap();
            panic!("poisoning lock");
        })
        .join();

        assert!(cache_mutex.is_poisoned());

        // with_cache handles poison gracefully without panic
        let res1 = with_cache(&cache_mutex, |cache| cache.len());
        assert_eq!(res1, 0);

        // Subsequent calls continue to recover safely without panic
        let key = WallpaperCacheKey {
            canonical_path: PathBuf::from("/tmp/wp.png"),
            mtime: SystemTime::UNIX_EPOCH,
            scheme_variant: SchemeVariant::Auto,
        };
        with_cache(&cache_mutex, |cache| {
            cache.insert(
                key.clone(),
                WallpaperAnalysis {
                    seed: 123,
                    detected_variant: SchemeVariant::Auto,
                },
            );
        });

        let res2 = with_cache(&cache_mutex, |cache| cache.get(&key));
        assert_eq!(
            res2,
            Some(WallpaperAnalysis {
                seed: 123,
                detected_variant: SchemeVariant::Auto,
            })
        );
    }
}
