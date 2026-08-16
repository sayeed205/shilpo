use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use super::types::{ProviderId, SearchCandidate};

/// Configuration limits for a [`SearchSink`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkConfig {
    /// Maximum candidates accepted from a single provider (safety bound).
    pub max_per_provider: usize,
    /// Maximum total candidates accepted across all providers (safety bound before ranking).
    pub max_total: usize,
}

impl Default for SinkConfig {
    fn default() -> Self {
        Self {
            max_per_provider: 64,
            max_total: 256,
        }
    }
}

struct SinkInner {
    generation: u64,
    config: SinkConfig,
    seen_identities: HashSet<String>,
    provider_counts: HashMap<ProviderId, usize>,
    candidates: Vec<SearchCandidate>,
}

/// Thread-safe sink that collects search candidates from multiple providers.
///
/// Owns deduplication by canonical identity, per-provider quotas, total bounds,
/// and generation-based cancellation.
#[derive(Clone)]
pub struct SearchSink {
    inner: Arc<Mutex<SinkInner>>,
}

impl SearchSink {
    /// Creates a new sink for the given query generation and configuration.
    pub fn new(generation: u64, config: SinkConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SinkInner {
                generation,
                config,
                seen_identities: HashSet::new(),
                provider_counts: HashMap::new(),
                candidates: Vec::new(),
            })),
        }
    }

    /// Creates a sink with default configuration limits for the given query generation.
    pub fn with_default_config(generation: u64) -> Self {
        Self::new(generation, SinkConfig::default())
    }

    /// Creates a default sink for testing with the given generation.
    pub fn for_test(generation: u64) -> Self {
        Self::with_default_config(generation)
    }

    /// Returns the active query generation accepted by this sink.
    pub fn generation(&self) -> u64 {
        self.inner.lock().unwrap().generation
    }

    /// Returns `true` if the given generation matches the sink's active generation.
    pub fn is_current(&self, generation: u64) -> bool {
        self.inner.lock().unwrap().generation == generation
    }

    /// Cancels this sink by advancing its accepted generation to a terminal tombstone.
    ///
    /// Subsequent candidate deliveries with the previous generation will be dropped.
    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.generation = u64::MAX;
    }

    /// Pushes a search candidate into the sink.
    ///
    /// Returns `true` if the candidate was accepted and buffered; `false` if it
    /// was dropped due to stale generation, duplicate identity, provider quota,
    /// or total bounds.
    pub fn push(&self, candidate: SearchCandidate) -> bool {
        let mut inner = self.inner.lock().unwrap();

        // 1. Generation staleness check: only the active generation is accepted.
        if candidate.generation != inner.generation {
            return false;
        }

        // 2. Deduplication check: canonical identity must be unique across all providers.
        if inner.seen_identities.contains(&candidate.canonical_id) {
            return false;
        }

        // 3. Total bounds check: cannot exceed max_total.
        if inner.candidates.len() >= inner.config.max_total {
            return false;
        }

        // 4. Per-provider quota check: cannot exceed max_per_provider.
        let provider_count = inner
            .provider_counts
            .get(&candidate.provider_id)
            .copied()
            .unwrap_or(0);
        if provider_count >= inner.config.max_per_provider {
            return false;
        }

        // Accept and record candidate.
        inner.seen_identities.insert(candidate.canonical_id.clone());
        inner
            .provider_counts
            .insert(candidate.provider_id.clone(), provider_count + 1);
        inner.candidates.push(candidate);

        true
    }

    /// Returns an immutable snapshot of all candidates accepted so far.
    pub fn snapshot(&self) -> Vec<SearchCandidate> {
        self.inner.lock().unwrap().candidates.clone()
    }

    /// Returns the number of candidates currently in the sink.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().candidates.len()
    }

    /// Returns `true` if the sink has accepted no candidates.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().candidates.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::shell::overview_search::types::{
        ActionResult, ResultCategory, SearchActivation, SearchError, SearchProvider, SearchRequest,
        SearchResultIcon,
    };

    fn make_test_candidate(
        provider: &str,
        id: &str,
        title: &str,
        generation: u64,
    ) -> SearchCandidate {
        SearchCandidate::new(
            ProviderId::new(provider),
            id,
            generation,
            title,
            None,
            ResultCategory::Custom,
            SearchResultIcon::Initial(title.chars().next().unwrap_or('?')),
            "Open",
            SearchActivation::new(id),
        )
    }

    #[test]
    fn test_stale_generation_candidate_is_rejected() {
        let sink = SearchSink::for_test(2);

        // Stale candidate with generation 1
        let stale = make_test_candidate("p1", "item-1", "Stale Item", 1);
        assert!(!sink.push(stale));
        assert!(sink.is_empty());

        // Current candidate with generation 2
        let current = make_test_candidate("p1", "item-2", "Current Item", 2);
        assert!(sink.push(current));
        assert_eq!(sink.len(), 1);

        // Future/mismatched candidate with generation 3
        let future = make_test_candidate("p1", "item-3", "Future Item", 3);
        assert!(!sink.push(future));
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn test_provider_quota_enforcement_and_isolation() {
        let config = SinkConfig {
            max_per_provider: 2,
            max_total: 10,
        };
        let sink = SearchSink::new(1, config);

        let p1 = "provider-1";
        let p2 = "provider-2";

        // Provider 1 delivers 2 candidates (hits quota)
        assert!(sink.push(make_test_candidate(p1, "p1-1", "P1 Item 1", 1)));
        assert!(sink.push(make_test_candidate(p1, "p1-2", "P1 Item 2", 1)));
        // Provider 1 surplus is dropped
        assert!(!sink.push(make_test_candidate(p1, "p1-3", "P1 Item 3", 1)));

        // Provider 2 still delivers successfully
        assert!(sink.push(make_test_candidate(p2, "p2-1", "P2 Item 1", 1)));
        assert!(sink.push(make_test_candidate(p2, "p2-2", "P2 Item 2", 1)));
        // Provider 2 surplus is dropped
        assert!(!sink.push(make_test_candidate(p2, "p2-3", "P2 Item 3", 1)));

        assert_eq!(sink.len(), 4);
    }

    #[test]
    fn test_duplicate_canonical_identities_collapse() {
        let sink = SearchSink::for_test(1);

        let cand1 = make_test_candidate("provider-a", "canonical-app-1", "App From A", 1);
        let cand2 = make_test_candidate("provider-b", "canonical-app-1", "App From B", 1);

        assert!(sink.push(cand1));
        assert!(!sink.push(cand2), "Duplicate canonical ID must be rejected");

        assert_eq!(sink.len(), 1);
        assert_eq!(sink.snapshot()[0].title, "App From A");
    }

    #[test]
    fn test_total_bounds_enforcement() {
        let config = SinkConfig {
            max_per_provider: 10,
            max_total: 3,
        };
        let sink = SearchSink::new(1, config);

        assert!(sink.push(make_test_candidate("p1", "id-1", "Item 1", 1)));
        assert!(sink.push(make_test_candidate("p1", "id-2", "Item 2", 1)));
        assert!(sink.push(make_test_candidate("p1", "id-3", "Item 3", 1)));
        // Total bound reached
        assert!(!sink.push(make_test_candidate("p2", "id-4", "Item 4", 1)));

        assert_eq!(sink.len(), 3);
    }

    #[test]
    fn test_cancellation_invalidates_future_pushes() {
        let sink = SearchSink::for_test(1);

        assert!(sink.push(make_test_candidate("p1", "id-1", "Pre-cancel", 1)));
        assert_eq!(sink.len(), 1);

        // Cancel the sink
        sink.cancel();
        assert!(!sink.is_current(1));

        // Any candidate with old generation is dropped
        assert!(!sink.push(make_test_candidate("p1", "id-2", "Post-cancel", 1)));
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.snapshot()[0].title, "Pre-cancel");
    }

    struct FastProvider;
    impl SearchProvider for FastProvider {
        fn id(&self) -> ProviderId {
            ProviderId::from_static("fast-provider")
        }
        fn search(&self, request: SearchRequest, sink: SearchSink) {
            sink.push(make_test_candidate(
                "fast-provider",
                "fast-1",
                "Fast Result",
                request.generation,
            ));
        }
        fn activate(&self, _activation: SearchActivation) -> Result<ActionResult, SearchError> {
            Ok(ActionResult::Handled {
                close_overview: true,
            })
        }
    }

    struct StalledProvider {
        invoked: Arc<AtomicBool>,
    }
    impl SearchProvider for StalledProvider {
        fn id(&self) -> ProviderId {
            ProviderId::from_static("stalled-provider")
        }
        fn search(&self, _request: SearchRequest, _sink: SearchSink) {
            self.invoked.store(true, Ordering::SeqCst);
            // Stalled: never delivers candidates into sink
        }
        fn activate(&self, _activation: SearchActivation) -> Result<ActionResult, SearchError> {
            Err(SearchError::ActivationFailed("stalled".into()))
        }
    }

    #[test]
    fn test_stalled_provider_invocation_does_not_prevent_fast_provider_from_pushing() {
        // Covers the sink's own contract in isolation: a provider that never
        // pushes a candidate must not corrupt or block subsequent pushes
        // from another provider sharing the same sink. Real cross-provider
        // concurrency (a provider whose `search` call blocks the calling
        // thread) is covered at the coordinator level, where dispatch is
        // actually parallelized: see
        // coordinator::tests::test_slow_provider_does_not_delay_dispatch_beyond_the_slowest_provider.
        let sink = SearchSink::for_test(1);
        let request = SearchRequest::new(
            "query",
            crate::shell::overview_search::parser::SearchMode::Default,
            "query",
            1,
        );

        let stalled_invoked = Arc::new(AtomicBool::new(false));
        let stalled = StalledProvider {
            invoked: stalled_invoked.clone(),
        };
        let fast = FastProvider;

        stalled.search(request.clone(), sink.clone());
        fast.search(request, sink.clone());

        assert!(stalled_invoked.load(Ordering::SeqCst));
        assert_eq!(sink.len(), 1);
        assert_eq!(sink.snapshot()[0].title, "Fast Result");
    }

    #[test]
    fn test_concurrent_pushes_from_multiple_threads_are_race_free() {
        use std::thread;

        let config = SinkConfig {
            max_per_provider: 100,
            max_total: 1000,
        };
        let sink = SearchSink::new(1, config);

        let thread_count = 8;
        let per_thread = 20;

        thread::scope(|scope| {
            for t in 0..thread_count {
                let sink = sink.clone();
                scope.spawn(move || {
                    for i in 0..per_thread {
                        sink.push(make_test_candidate(
                            &format!("provider-{t}"),
                            &format!("provider-{t}:item-{i}"),
                            "Item",
                            1,
                        ));
                    }
                });
            }
        });

        // Every push had a unique canonical id and a distinct provider quota,
        // so every one of them must have been accepted with no data loss or
        // corruption from concurrent access.
        assert_eq!(sink.len(), thread_count * per_thread);
    }
}
