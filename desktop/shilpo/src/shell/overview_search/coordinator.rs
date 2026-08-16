use super::parser;
use super::sink::{SearchSink, SinkConfig};
use super::types::{
    ActionResult, ProviderId, SearchActivation, SearchError, SearchProvider, SearchRequest,
};
use std::sync::Arc;

/// Host search coordinator that manages registered providers and fans out requests.
#[derive(Clone, Default)]
pub struct SearchCoordinator {
    providers: Vec<Arc<dyn SearchProvider>>,
}

impl SearchCoordinator {
    /// Creates a new coordinator with the given providers.
    pub fn new(providers: Vec<Arc<dyn SearchProvider>>) -> Self {
        Self { providers }
    }

    /// Registers an additional provider.
    pub fn register(&mut self, provider: Arc<dyn SearchProvider>) {
        self.providers.push(provider);
    }

    /// Fans out a search query to all registered providers using the provided sink.
    ///
    /// Each provider's `search` call runs on its own thread so a slow or
    /// stalled provider cannot delay another provider's candidates from
    /// being collected. Providers write into a private scratch sink; once
    /// every thread has joined, accepted candidates are merged into the
    /// caller's sink in provider-registration order, keeping ordering
    /// deterministic until the host ranker (#201) takes ownership of it.
    pub fn search(&self, raw_query: &str, generation: u64, sink: &SearchSink) {
        let (mode, query) = parser::parse_query(raw_query);
        let request = SearchRequest::new(raw_query, mode, query, generation);

        let scratch_sinks: Vec<SearchSink> = self
            .providers
            .iter()
            .map(|_| SearchSink::new(generation, SinkConfig::default()))
            .collect();

        std::thread::scope(|scope| {
            for (provider, scratch) in self.providers.iter().zip(&scratch_sinks) {
                let request = request.clone();
                let scratch = scratch.clone();
                scope.spawn(move || provider.search(request, scratch));
            }
        });

        for scratch in &scratch_sinks {
            for candidate in scratch.snapshot() {
                sink.push(candidate);
            }
        }
    }

    /// Routes an activation request to the appropriate provider by ID.
    pub fn activate(
        &self,
        provider_id: &ProviderId,
        activation: SearchActivation,
    ) -> Result<ActionResult, SearchError> {
        let provider = self
            .providers
            .iter()
            .find(|p| p.id() == *provider_id)
            .ok_or_else(|| SearchError::NotFound(provider_id.to_string()))?;

        provider.activate(activation)
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
            SearchActivation::new("x"),
        );
        assert!(matches!(unknown, Err(SearchError::NotFound(_))));
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
        coordinator.search("query", 1, &sink);
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
        coordinator.search("query", 1, &sink);

        let results = sink.snapshot();
        assert!(results.iter().any(|c| c.title == "Fast Result"));
        assert!(results.iter().any(|c| c.title == "Slow Result"));
    }
}
