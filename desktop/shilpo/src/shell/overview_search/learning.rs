//! Bounded, persisted frequency and recency search ranking learning.
//!
//! This module tracks result activations keyed by stable canonical result identity
//! (`SearchCandidate::canonical_id`), allowing frequently and recently activated
//! results to receive a learned score boost.
//!
//! ### Model Invariants & Bounds
//! - **Influence Cap**: Learning contributes at most `+40` ([`MAX_INFLUENCE_BOOST`]) to a
//!   candidate's score. This ensures high-level launcher category boundaries (Window `250` ->
//!   App `200` -> Action `150`, with `50` point gaps) cannot be usurped by heavily trained
//!   lower-category results.
//! - **Capacity & LRU Eviction**: At most `512` ([`MAX_LEARNING_ENTRIES`]) unique canonical
//!   identities are tracked simultaneously. When full, the least-recently-activated entry is evicted.
//! - **Exponential Time Decay**: Activation contributions decay exponentially with a ~14-day
//!   ([`DEFAULT_HALF_LIFE_SECS`]) half-life, calculated on read relative to an injectable clock.
//! - **Graceful Degradation**: Persistence failures in the underlying LMDB store degrade to
//!   in-memory learning and record a last-error string; queries and activations never fail or panic.
//! - **Explicit Reset**: Per-item and global resets take effect immediately in both persistent
//!   and in-memory stores.

use shilpo_services::{HeedSessionStore, SearchLearningRecord};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

/// Maximum score boost that learning can contribute to a candidate (+40).
pub const MAX_INFLUENCE_BOOST: i64 = 40;

/// Maximum number of distinct search learning records retained.
pub const MAX_LEARNING_ENTRIES: usize = 512;

/// Default half-life for time decay in seconds (14 days = 1,209,600s).
pub const DEFAULT_HALF_LIFE_SECS: u64 = 14 * 24 * 3600;

/// Time source abstraction for deterministic decay calculation and testing.
pub trait LearningClock: Send + Sync {
    fn now_secs(&self) -> u64;
}

/// System clock reading real wall-clock seconds since Unix epoch.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemLearningClock;

impl LearningClock for SystemLearningClock {
    fn now_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Injectable test clock allowing explicit time control without wall-clock sleeps.
#[derive(Debug)]
pub struct TestLearningClock {
    now: Mutex<u64>,
}

impl TestLearningClock {
    pub fn new(initial_secs: u64) -> Self {
        Self {
            now: Mutex::new(initial_secs),
        }
    }

    pub fn advance_secs(&self, delta: u64) {
        *self.now.lock().unwrap() += delta;
    }

    pub fn set_secs(&self, secs: u64) {
        *self.now.lock().unwrap() = secs;
    }
}

impl LearningClock for TestLearningClock {
    fn now_secs(&self) -> u64 {
        *self.now.lock().unwrap()
    }
}

/// Trait for querying and updating learned search rankings.
pub trait SearchLearningStore: Send + Sync {
    /// Computes the learned score boost for a given canonical ID at current clock time.
    /// Returns a boost clamped to `[0, MAX_INFLUENCE_BOOST]`.
    fn score_boost(&self, canonical_id: &str) -> i64;

    /// Records an activation of the specified canonical ID.
    fn record_activation(&self, canonical_id: &str);

    /// Forgets the learned ranking for a single canonical ID.
    fn forget_result(&self, canonical_id: &str) -> bool;

    /// Clears all learned rankings globally.
    fn clear_all(&self);

    /// Surfaces the last persistence error message, if any occurred.
    fn last_error(&self) -> Option<String>;
}

/// Production implementation of [`SearchLearningStore`] backed by LMDB with in-memory fallback.
pub struct HeedSearchLearningStore {
    heed_store: Option<Arc<HeedSessionStore>>,
    fallback_memory: RwLock<HashMap<String, SearchLearningRecord>>,
    clock: Arc<dyn LearningClock>,
    last_error: Mutex<Option<String>>,
    max_entries: usize,
    half_life_secs: u64,
}

impl HeedSearchLearningStore {
    pub fn new(heed_store: Option<Arc<HeedSessionStore>>) -> Self {
        Self::with_clock(heed_store, Arc::new(SystemLearningClock))
    }

    pub fn with_clock(
        heed_store: Option<Arc<HeedSessionStore>>,
        clock: Arc<dyn LearningClock>,
    ) -> Self {
        Self::with_config(
            heed_store,
            clock,
            MAX_LEARNING_ENTRIES,
            DEFAULT_HALF_LIFE_SECS,
        )
    }

    pub fn with_config(
        heed_store: Option<Arc<HeedSessionStore>>,
        clock: Arc<dyn LearningClock>,
        max_entries: usize,
        half_life_secs: u64,
    ) -> Self {
        Self {
            heed_store,
            fallback_memory: RwLock::new(HashMap::new()),
            clock,
            last_error: Mutex::new(None),
            max_entries,
            half_life_secs,
        }
    }

    fn calculate_boost(record: &SearchLearningRecord, now_secs: u64, half_life_secs: u64) -> i64 {
        let elapsed = now_secs.saturating_sub(record.last_activated_at_secs);
        let decay_factor =
            (-(elapsed as f64) * (std::f64::consts::LN_2 / (half_life_secs.max(1) as f64))).exp();
        let decayed_activations = record.decayed_score * decay_factor;
        let boost = (decayed_activations * 10.0)
            .min(MAX_INFLUENCE_BOOST as f64)
            .round() as i64;
        boost.clamp(0, MAX_INFLUENCE_BOOST)
    }

    fn record_in_memory(&self, canonical_id: &str, now_secs: u64) {
        let mut memory = self.fallback_memory.write().unwrap();
        if let Some(record) = memory.get_mut(canonical_id) {
            let elapsed = now_secs.saturating_sub(record.last_activated_at_secs);
            let decay_factor = (-(elapsed as f64)
                * (std::f64::consts::LN_2 / (self.half_life_secs.max(1) as f64)))
                .exp();
            record.decayed_score = record.decayed_score * decay_factor + 1.0;
            record.activation_count = record.activation_count.saturating_add(1);
            record.last_activated_at_secs = now_secs;
        } else {
            if memory.len() >= self.max_entries {
                let oldest = memory
                    .iter()
                    .min_by_key(|(_, r)| r.last_activated_at_secs)
                    .map(|(k, _)| k.clone());
                if let Some(oldest_key) = oldest {
                    memory.remove(&oldest_key);
                }
            }
            memory.insert(
                canonical_id.to_string(),
                SearchLearningRecord::new_initial(now_secs),
            );
        }
    }
}

impl SearchLearningStore for HeedSearchLearningStore {
    fn score_boost(&self, canonical_id: &str) -> i64 {
        let now_secs = self.clock.now_secs();
        if let Some(heed) = &self.heed_store {
            match heed.get_search_learning_record(canonical_id) {
                Ok(Some(rec)) => {
                    return Self::calculate_boost(&rec, now_secs, self.half_life_secs);
                }
                Ok(None) => {}
                Err(err) => {
                    *self.last_error.lock().unwrap() = Some(err.to_string());
                }
            }
        }

        let memory = self.fallback_memory.read().unwrap();
        if let Some(rec) = memory.get(canonical_id) {
            Self::calculate_boost(rec, now_secs, self.half_life_secs)
        } else {
            0
        }
    }

    fn record_activation(&self, canonical_id: &str) {
        let now_secs = self.clock.now_secs();
        if let Some(heed) = &self.heed_store {
            match heed.record_search_activation(
                canonical_id,
                now_secs,
                self.max_entries,
                self.half_life_secs,
            ) {
                Ok(()) => {
                    self.fallback_memory.write().unwrap().remove(canonical_id);
                    return;
                }
                Err(err) => {
                    *self.last_error.lock().unwrap() = Some(err.to_string());
                }
            }
        }

        self.record_in_memory(canonical_id, now_secs);
    }

    fn forget_result(&self, canonical_id: &str) -> bool {
        let mut deleted = false;
        if let Some(heed) = &self.heed_store {
            match heed.forget_search_result(canonical_id) {
                Ok(d) => deleted |= d,
                Err(err) => {
                    *self.last_error.lock().unwrap() = Some(err.to_string());
                }
            }
        }
        deleted |= self
            .fallback_memory
            .write()
            .unwrap()
            .remove(canonical_id)
            .is_some();
        deleted
    }

    fn clear_all(&self) {
        if let Some(heed) = &self.heed_store
            && let Err(err) = heed.clear_search_learning()
        {
            *self.last_error.lock().unwrap() = Some(err.to_string());
        }
        self.fallback_memory.write().unwrap().clear();
    }

    fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }
}

/// No-op implementation of [`SearchLearningStore`] that contributes 0 boost and ignores writes.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopSearchLearningStore;

impl SearchLearningStore for NoopSearchLearningStore {
    fn score_boost(&self, _canonical_id: &str) -> i64 {
        0
    }

    fn record_activation(&self, _canonical_id: &str) {}

    fn forget_result(&self, _canonical_id: &str) -> bool {
        false
    }

    fn clear_all(&self) {}

    fn last_error(&self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_fallback_activation_and_decay() {
        let clock = Arc::new(TestLearningClock::new(100_000));
        let store =
            HeedSearchLearningStore::with_config(None, clock.clone(), 512, DEFAULT_HALF_LIFE_SECS);

        assert_eq!(store.score_boost("app:terminal"), 0);

        // 1st activation -> boost = 10
        store.record_activation("app:terminal");
        assert_eq!(store.score_boost("app:terminal"), 10);

        // 2nd activation -> boost = 20
        store.record_activation("app:terminal");
        assert_eq!(store.score_boost("app:terminal"), 20);

        // 4 activations total -> boost = 40 (saturates at +40)
        store.record_activation("app:terminal");
        store.record_activation("app:terminal");
        assert_eq!(store.score_boost("app:terminal"), 40);

        // 5th activation -> still capped at +40
        store.record_activation("app:terminal");
        assert_eq!(store.score_boost("app:terminal"), 40);

        // Advance clock by 14 days (half-life): score decays from ~5.0 to ~2.5 -> boost is 25
        clock.advance_secs(DEFAULT_HALF_LIFE_SECS);
        assert_eq!(store.score_boost("app:terminal"), 25);

        // Forget result removes it immediately
        assert!(store.forget_result("app:terminal"));
        assert_eq!(store.score_boost("app:terminal"), 0);
    }

    #[test]
    fn test_in_memory_fallback_lru_eviction() {
        let clock = Arc::new(TestLearningClock::new(10));
        let store = HeedSearchLearningStore::with_config(
            None,
            clock.clone(),
            2, // Max 2 entries
            DEFAULT_HALF_LIFE_SECS,
        );

        clock.set_secs(10);
        store.record_activation("item:1");

        clock.set_secs(20);
        store.record_activation("item:2");

        assert_eq!(store.score_boost("item:1"), 10);
        assert_eq!(store.score_boost("item:2"), 10);

        clock.set_secs(30);
        store.record_activation("item:3"); // Should evict item:1

        assert_eq!(store.score_boost("item:1"), 0);
        assert_eq!(store.score_boost("item:2"), 10);
        assert_eq!(store.score_boost("item:3"), 10);
    }

    #[test]
    fn test_clear_all_resets_learning() {
        let clock = Arc::new(TestLearningClock::new(10));
        let store =
            HeedSearchLearningStore::with_config(None, clock.clone(), 512, DEFAULT_HALF_LIFE_SECS);

        store.record_activation("item:1");
        store.record_activation("item:2");
        assert_eq!(store.score_boost("item:1"), 10);
        assert_eq!(store.score_boost("item:2"), 10);

        store.clear_all();
        assert_eq!(store.score_boost("item:1"), 0);
        assert_eq!(store.score_boost("item:2"), 0);
    }
}
