use std::sync::Arc;

use super::{
    learning::{NoopSearchLearningStore, SearchLearningStore},
    parser,
    ranker::{self, RankerConfig},
    sink::{SearchSink, SinkConfig},
    types::{
        ActionResult, ProviderId, SearchActivation, SearchError, SearchProvider, SearchRequest,
    },
};

/// Bound on candidates collected from a single provider before ranking.
///
/// Each scratch sink below is written to by exactly one provider, so
/// `max_per_provider` has no cross-provider-fairness role there — only
/// `max_total` matters, as a safety net against a runaway or hostile
/// provider. This value must comfortably exceed any realistic single
/// provider's output (hundreds of installed applications, dozens of
/// actions, clipboard history, keybindings) so a real candidate is never
/// dropped before the ranker gets to see it.
const SCRATCH_SINK_CAPACITY: usize = 4096;

/// Host search coordinator that manages registered providers and fans out requests.
#[derive(Clone)]
pub struct SearchCoordinator {
    providers: Vec<Arc<dyn SearchProvider>>,
    learning_store: Arc<dyn SearchLearningStore>,
    ranker_config: RankerConfig,
}

impl Default for SearchCoordinator {
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            learning_store: Arc::new(NoopSearchLearningStore),
            ranker_config: RankerConfig::default(),
        }
    }
}

impl SearchCoordinator {
    /// Creates a new coordinator with the given providers.
    pub fn new(providers: Vec<Arc<dyn SearchProvider>>) -> Self {
        Self {
            providers,
            learning_store: Arc::new(NoopSearchLearningStore),
            ranker_config: RankerConfig::default(),
        }
    }

    /// Builder helper to set the search learning store.
    pub fn with_learning_store(mut self, learning_store: Arc<dyn SearchLearningStore>) -> Self {
        self.learning_store = learning_store;
        self
    }

    /// Builder helper to set ranker configuration.
    pub fn with_ranker_config(mut self, ranker_config: RankerConfig) -> Self {
        self.ranker_config = ranker_config;
        self
    }

    /// Registers an additional provider.
    pub fn register(&mut self, provider: Arc<dyn SearchProvider>) {
        self.providers.push(provider);
    }

    /// Fans out a search query to all registered providers and ranks the merged results into the provided sink.
    ///
    /// Each provider's `search` call runs concurrently on its own thread so a slow or
    /// stalled provider cannot delay another provider's candidates from being collected.
    /// Each provider writes into a private scratch sink sized well above any realistic
    /// single-provider output (see [`SCRATCH_SINK_CAPACITY`]), so a provider that returns
    /// many legitimate candidates cannot have any of them silently dropped before ranking.
    /// Collected candidates are merged across all scratch sinks, scored, ordered by the
    /// host ranker, and truncated to top-k once into the caller's sink.
    pub fn search(&self, raw_query: &str, generation: u64, sink: &SearchSink) {
        let (mode, query) = parser::parse_query(raw_query);
        let request = SearchRequest::new(raw_query, mode, query, generation);

        let scratch_config = SinkConfig {
            max_per_provider: SCRATCH_SINK_CAPACITY,
            max_total: SCRATCH_SINK_CAPACITY,
        };
        let scratch_sinks: Vec<SearchSink> = self
            .providers
            .iter()
            .map(|_| SearchSink::new(generation, scratch_config.clone()))
            .collect();

        std::thread::scope(|scope| {
            for (provider, scratch) in self.providers.iter().zip(&scratch_sinks) {
                let request = request.clone();
                let scratch = scratch.clone();
                scope.spawn(move || provider.search(request, scratch));
            }
        });

        let mut all_candidates = Vec::new();
        for scratch in &scratch_sinks {
            all_candidates.extend(scratch.snapshot());
        }

        let ranked = ranker::rank(
            all_candidates,
            query,
            self.learning_store.as_ref(),
            &self.ranker_config,
        );

        for candidate in ranked {
            sink.push(candidate);
        }
    }

    /// Routes an activation request to the appropriate provider by ID and records the activation
    /// in the learning store upon success.
    pub fn activate(
        &self,
        provider_id: &ProviderId,
        canonical_id: &str,
        activation: SearchActivation,
    ) -> Result<ActionResult, SearchError> {
        let provider = self
            .providers
            .iter()
            .find(|p| p.id() == *provider_id)
            .ok_or_else(|| SearchError::NotFound(provider_id.to_string()))?;

        let result = provider.activate(activation)?;
        self.learning_store.record_activation(canonical_id);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::overview_search::types::{ResultCategory, SearchCandidate, SearchResultIcon};

    struct TestProvider {
        id: &'static str,
        prefix: &'static str,
    }

    impl SearchProvider for TestProvider {
        fn id(&self) -> ProviderId {
            ProviderId::from_static(self.id)
        }

        fn search(&self, request: SearchRequest, sink: SearchSink) {
            sink.push(SearchCandidate::new(
                self.id(),
                format!("{}:{}", self.id, request.query),
                request.generation,
                format!("{} - {}", self.prefix, request.query),
                None,
                ResultCategory::Custom,
                SearchResultIcon::Initial('T'),
                "Open",
                SearchActivation::new(format!("payload-{}", self.id)),
            ));
        }

        fn activate(&self, activation: SearchActivation) -> Result<ActionResult, SearchError> {
            if activation.payload == format!("payload-{}", self.id) {
                Ok(ActionResult::Handled {
                    close_overview: true,
                })
            } else {
                Err(SearchError::ActivationFailed("mismatched payload".into()))
            }
        }
    }

    #[test]
    fn test_coordinator_fanout_and_activation() {
        let p1 = Arc::new(TestProvider {
            id: "p1",
            prefix: "Provider One",
        });
        let p2 = Arc::new(TestProvider {
            id: "p2",
            prefix: "Provider Two",
        });

        let coordinator = SearchCoordinator::new(vec![p1, p2]);
        let sink = SearchSink::for_test(1);

        coordinator.search("hello", 1, &sink);

        let results = sink.snapshot();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Provider One - hello");
        assert_eq!(results[1].title, "Provider Two - hello");

        // Test activation routing
        let act1 = coordinator.activate(
            &ProviderId::from_static("p1"),
            &results[0].canonical_id,
            results[0].activation.clone(),
        );
        assert!(matches!(
            act1,
            Ok(ActionResult::Handled {
                close_overview: true
            })
        ));

        let act2 = coordinator.activate(
            &ProviderId::from_static("p2"),
            &results[1].canonical_id,
            results[1].activation.clone(),
        );
        assert!(matches!(
            act2,
            Ok(ActionResult::Handled {
                close_overview: true
            })
        ));

        // Test unknown provider activation
        let unknown = coordinator.activate(
            &ProviderId::from_static("unknown"),
            "unknown:x",
            SearchActivation::new("x"),
        );
        assert!(matches!(unknown, Err(SearchError::NotFound(_))));
    }

    #[test]
    fn test_coordinator_activation_records_learning_exactly_once() {
        use crate::shell::overview_search::learning::{
            DEFAULT_HALF_LIFE_SECS, HeedSearchLearningStore, TestLearningClock,
        };

        let p1 = Arc::new(TestProvider {
            id: "p1",
            prefix: "Provider One",
        });

        let clock = Arc::new(TestLearningClock::new(1000));
        let learning = Arc::new(HeedSearchLearningStore::with_config(
            None,
            clock,
            512,
            DEFAULT_HALF_LIFE_SECS,
        ));

        let coordinator = SearchCoordinator::new(vec![p1]).with_learning_store(learning.clone());
        let sink = SearchSink::for_test(1);

        // Searching generates impressions but records 0 activations in learning store
        coordinator.search("query", 1, &sink);
        let results = sink.snapshot();
        assert_eq!(results.len(), 1);
        assert_eq!(learning.score_boost(&results[0].canonical_id), 0);

        // One activation records exactly one increment (boost = 10)
        let act_res = coordinator.activate(
            &ProviderId::from_static("p1"),
            &results[0].canonical_id,
            results[0].activation.clone(),
        );
        assert!(act_res.is_ok());
        assert_eq!(learning.score_boost(&results[0].canonical_id), 10);
    }

    struct SleepingProvider {
        id: &'static str,
        title: &'static str,
        sleep: std::time::Duration,
    }

    impl SearchProvider for SleepingProvider {
        fn id(&self) -> ProviderId {
            ProviderId::from_static(self.id)
        }

        fn search(&self, request: SearchRequest, sink: SearchSink) {
            std::thread::sleep(self.sleep);
            sink.push(SearchCandidate::new(
                self.id(),
                format!("{}:{}", self.id, request.query),
                request.generation,
                self.title,
                None,
                ResultCategory::Custom,
                SearchResultIcon::Initial('S'),
                "Open",
                SearchActivation::new(format!("payload-{}", self.id)),
            ));
        }

        fn activate(&self, _activation: SearchActivation) -> Result<ActionResult, SearchError> {
            Ok(ActionResult::Handled {
                close_overview: true,
            })
        }
    }

    #[test]
    fn test_slow_provider_does_not_delay_dispatch_beyond_the_slowest_provider() {
        // Two providers that each take ~100ms. If dispatch were sequential,
        // total wall time would be ~200ms. Concurrent dispatch keeps it
        // close to the duration of a single provider.
        let sleep = std::time::Duration::from_millis(100);
        let p1 = Arc::new(SleepingProvider {
            id: "slow-1",
            title: "Result One",
            sleep,
        });
        let p2 = Arc::new(SleepingProvider {
            id: "slow-2",
            title: "Result Two",
            sleep,
        });

        let coordinator = SearchCoordinator::new(vec![p1, p2]);
        let sink = SearchSink::for_test(1);

        let start = std::time::Instant::now();
        coordinator.search("Result", 1, &sink);
        let elapsed = start.elapsed();

        let results = sink.snapshot();
        assert_eq!(results.len(), 2, "both slow providers must still deliver");
        assert!(
            elapsed < std::time::Duration::from_millis(180),
            "expected concurrent dispatch (~100ms), got {elapsed:?} \
             which indicates providers ran sequentially"
        );
    }

    #[test]
    fn test_fast_provider_delivers_alongside_a_slow_provider() {
        struct FastProvider;
        impl SearchProvider for FastProvider {
            fn id(&self) -> ProviderId {
                ProviderId::from_static("fast")
            }
            fn search(&self, request: SearchRequest, sink: SearchSink) {
                sink.push(SearchCandidate::new(
                    self.id(),
                    "fast:1",
                    request.generation,
                    "Fast Result",
                    None,
                    ResultCategory::Custom,
                    SearchResultIcon::Initial('F'),
                    "Open",
                    SearchActivation::new("payload-fast"),
                ));
            }
            fn activate(&self, _activation: SearchActivation) -> Result<ActionResult, SearchError> {
                Ok(ActionResult::Handled {
                    close_overview: true,
                })
            }
        }

        let slow = Arc::new(SleepingProvider {
            id: "slow",
            title: "Slow Result",
            sleep: std::time::Duration::from_millis(60),
        });
        let fast = Arc::new(FastProvider);

        let coordinator = SearchCoordinator::new(vec![slow, fast]);
        let sink = SearchSink::for_test(1);
        coordinator.search("Result", 1, &sink);

        let results = sink.snapshot();
        assert!(results.iter().any(|c| c.title == "Fast Result"));
        assert!(results.iter().any(|c| c.title == "Slow Result"));
    }

    #[test]
    fn test_a_prolific_single_provider_does_not_starve_its_own_late_candidates() {
        // A provider producing far more than the old 64-item scratch-sink
        // quota must still have every one of its candidates reach the
        // ranker. Regression test for the intra-provider starvation bug:
        // sink.push()'s return value was discarded, so a provider emitting
        // more than SCRATCH_SINK_CAPACITY candidates used to lose the tail
        // silently before ranking ever saw them.
        struct ManyCandidatesProvider;
        impl SearchProvider for ManyCandidatesProvider {
            fn id(&self) -> ProviderId {
                ProviderId::from_static("many")
            }
            fn search(&self, request: SearchRequest, sink: SearchSink) {
                // One order of magnitude past the old 64-candidate cap, and
                // still comfortably under SCRATCH_SINK_CAPACITY.
                for i in 0..(SCRATCH_SINK_CAPACITY / 4) {
                    sink.push(SearchCandidate::new(
                        self.id(),
                        format!("many:{i}"),
                        request.generation,
                        format!("Filler {i}"),
                        None,
                        ResultCategory::Custom,
                        SearchResultIcon::Initial('M'),
                        "Open",
                        SearchActivation::new(format!("payload-many-{i}")),
                    ));
                }
                // The target sits well past the old 64-item quota.
                sink.push(SearchCandidate::new(
                    self.id(),
                    "many:target",
                    request.generation,
                    "Zzzedge Unique Target",
                    None,
                    ResultCategory::Custom,
                    SearchResultIcon::Initial('Z'),
                    "Open",
                    SearchActivation::new("payload-target"),
                ));
            }
            fn activate(&self, _activation: SearchActivation) -> Result<ActionResult, SearchError> {
                Ok(ActionResult::Handled {
                    close_overview: true,
                })
            }
        }

        let coordinator = SearchCoordinator::new(vec![Arc::new(ManyCandidatesProvider)]);
        let sink = SearchSink::for_test(1);
        coordinator.search("Zzzedge", 1, &sink);

        let results = sink.snapshot();
        assert!(
            results.iter().any(|c| c.canonical_id == "many:target"),
            "candidate past the old scratch-sink quota was dropped before ranking"
        );
    }
}
