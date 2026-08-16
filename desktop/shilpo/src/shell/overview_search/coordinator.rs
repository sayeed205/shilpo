use super::parser;
use super::sink::SearchSink;
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
    pub fn search(&self, raw_query: &str, generation: u64, sink: &SearchSink) {
        let (mode, query) = parser::parse_query(raw_query);
        let request = SearchRequest::new(raw_query, mode, query, generation);

        for provider in &self.providers {
            provider.search(request.clone(), sink.clone());
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
}
