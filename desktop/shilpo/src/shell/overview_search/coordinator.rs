use std::sync::Arc;

use shilpo_ui::IconName;

use super::{
    learning::{NoopSearchLearningStore, SearchLearningStore},
    parser::{self, SearchMode},
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

    /// Fans out a search query to registered providers supporting the parsed query mode
    /// and ranks the merged results into the provided sink.
    ///
    /// Each eligible provider's `search` call runs concurrently on its own thread so a slow or
    /// stalled provider cannot delay another provider's candidates from being collected.
    /// Providers whose declared modes do not include `request.mode` are not spawned at all.
    /// Each provider writes into a private scratch sink sized well above any realistic
    /// single-provider output (see [`SCRATCH_SINK_CAPACITY`]), so a provider that returns
    /// many legitimate candidates cannot have any of them silently dropped before ranking.
    /// Collected candidates are merged across all scratch sinks, scored, ordered by the
    /// host ranker, and truncated to top-k once into the caller's sink.
    pub fn search(&self, raw_query: &str, generation: u64, sink: &SearchSink) {
        let (mode, query) = parser::parse_query(raw_query);
        let request = SearchRequest::new(raw_query, mode, query, generation);

        let eligible_providers: Vec<&Arc<dyn SearchProvider>> = self
            .providers
            .iter()
            .filter(|p| p.declared_modes().contains(&mode))
            .collect();

        let scratch_config = SinkConfig {
            max_per_provider: SCRATCH_SINK_CAPACITY,
            max_total: SCRATCH_SINK_CAPACITY,
        };
        let scratch_sinks: Vec<SearchSink> = eligible_providers
            .iter()
            .map(|_| SearchSink::new(generation, scratch_config.clone()))
            .collect();

        std::thread::scope(|scope| {
            for (provider, scratch) in eligible_providers.iter().zip(&scratch_sinks) {
                let request = request.clone();
                let scratch = scratch.clone();
                let provider = (*provider).clone();
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

    /// Returns the prefix icon for the given raw query based on declared provider descriptors.
    pub fn prefix_icon(&self, raw_query: &str) -> IconName {
        let (mode, _) = parser::parse_query(raw_query);
        if mode == SearchMode::Default {
            return IconName::Search;
        }
        for provider in &self.providers {
            if provider.declared_modes().contains(&mode)
                && let Some(icon) = provider.prefix_icon(mode)
            {
                return icon;
            }
        }
        mode.default_icon()
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

    #[test]
    fn test_declared_mode_scoping_does_not_dispatch_non_matching_providers() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct DispatchTrackingProvider {
            id: &'static str,
            modes: &'static [SearchMode],
            dispatches: Arc<AtomicUsize>,
        }

        impl SearchProvider for DispatchTrackingProvider {
            fn id(&self) -> ProviderId {
                ProviderId::from_static(self.id)
            }
            fn declared_modes(&self) -> &'static [SearchMode] {
                self.modes
            }
            fn search(&self, request: SearchRequest, sink: SearchSink) {
                self.dispatches.fetch_add(1, Ordering::SeqCst);
                sink.push(SearchCandidate::new(
                    self.id(),
                    format!("{}:{}", self.id, request.query),
                    request.generation,
                    format!("Title {}", self.id),
                    None,
                    ResultCategory::Custom,
                    SearchResultIcon::Initial('D'),
                    "Open",
                    SearchActivation::new("act"),
                ));
            }
            fn activate(&self, _activation: SearchActivation) -> Result<ActionResult, SearchError> {
                Ok(ActionResult::Handled {
                    close_overview: true,
                })
            }
        }

        let app_dispatches = Arc::new(AtomicUsize::new(0));
        let calc_dispatches = Arc::new(AtomicUsize::new(0));
        let clip_dispatches = Arc::new(AtomicUsize::new(0));

        let app_prov = Arc::new(DispatchTrackingProvider {
            id: "app-tracker",
            modes: &[SearchMode::Default, SearchMode::Apps],
            dispatches: app_dispatches.clone(),
        });
        let calc_prov = Arc::new(DispatchTrackingProvider {
            id: "calc-tracker",
            modes: &[SearchMode::Calculator],
            dispatches: calc_dispatches.clone(),
        });
        let clip_prov = Arc::new(DispatchTrackingProvider {
            id: "clip-tracker",
            modes: &[SearchMode::Clipboard],
            dispatches: clip_dispatches.clone(),
        });

        let coordinator = SearchCoordinator::new(vec![app_prov, calc_prov, clip_prov]);

        // 1. Query with '>' scopes to Apps mode: only app_prov dispatched
        let sink1 = SearchSink::for_test(1);
        coordinator.search(">term", 1, &sink1);
        assert_eq!(app_dispatches.load(Ordering::SeqCst), 1);
        assert_eq!(calc_dispatches.load(Ordering::SeqCst), 0);
        assert_eq!(clip_dispatches.load(Ordering::SeqCst), 0);

        // 2. Query with '=' scopes to Calculator mode: only calc_prov dispatched
        let sink2 = SearchSink::for_test(2);
        coordinator.search("=2+2", 2, &sink2);
        assert_eq!(app_dispatches.load(Ordering::SeqCst), 1);
        assert_eq!(calc_dispatches.load(Ordering::SeqCst), 1);
        assert_eq!(clip_dispatches.load(Ordering::SeqCst), 0);

        // 3. Query with ';' scopes to Clipboard mode: only clip_prov dispatched
        let sink3 = SearchSink::for_test(3);
        coordinator.search(";note", 3, &sink3);
        assert_eq!(app_dispatches.load(Ordering::SeqCst), 1);
        assert_eq!(calc_dispatches.load(Ordering::SeqCst), 1);
        assert_eq!(clip_dispatches.load(Ordering::SeqCst), 1);

        // 4. Default query: only app_prov dispatched (declared Default)
        let sink4 = SearchSink::for_test(4);
        coordinator.search("firefox", 4, &sink4);
        assert_eq!(app_dispatches.load(Ordering::SeqCst), 2);
        assert_eq!(calc_dispatches.load(Ordering::SeqCst), 1);
        assert_eq!(clip_dispatches.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_prefix_icon_uses_declared_provider_descriptors() {
        struct MockIconProvider;
        impl SearchProvider for MockIconProvider {
            fn id(&self) -> ProviderId {
                ProviderId::from_static("mock-icon")
            }
            fn declared_modes(&self) -> &'static [SearchMode] {
                &[SearchMode::Actions]
            }
            fn prefix_icon(&self, mode: SearchMode) -> Option<IconName> {
                if mode == SearchMode::Actions {
                    Some(IconName::Settings)
                } else {
                    None
                }
            }
            fn search(&self, _request: SearchRequest, _sink: SearchSink) {}
            fn activate(&self, _activation: SearchActivation) -> Result<ActionResult, SearchError> {
                Ok(ActionResult::Handled {
                    close_overview: true,
                })
            }
        }

        let coordinator = SearchCoordinator::new(vec![Arc::new(MockIconProvider)]);
        assert_eq!(coordinator.prefix_icon("/action"), IconName::Settings);
        assert_eq!(coordinator.prefix_icon("normal query"), IconName::Search);
    }

    #[test]
    fn test_all_sigils_scope_to_expected_providers() {
        use shilpo_services::ClipboardItem;
        use tokio::sync::watch;

        use crate::actions::{ActionCategory, ActionDescriptor, ActionId, ActionInputRequirement};
        use crate::shell::overview_search::{
            ActionSearchProvider, CalculatorSearchProvider, ClipboardSearchProvider,
            QuicklinksSearchProvider,
        };

        let actions = vec![ActionDescriptor {
            id: ActionId::ToggleOverview,
            name: "toggle-overview".to_string(),
            label: "Toggle Overview".to_string(),
            category: ActionCategory::Overlay,
            input: ActionInputRequirement::NoInput,
            enabled: true,
        }];
        let (_tx, rx) = watch::channel(vec![ClipboardItem::new_text(
            "copied snippet".to_string(),
            chrono::Utc::now(),
        )]);
        let keybindings = vec![("Super+T".to_string(), "Terminal".to_string())];

        let action_prov = Arc::new(ActionSearchProvider::new(actions));
        let clip_prov = Arc::new(ClipboardSearchProvider::new(Some(rx)));
        let calc_prov = Arc::new(CalculatorSearchProvider::new());
        let quick_prov = Arc::new(QuicklinksSearchProvider::new(keybindings));

        let coordinator =
            SearchCoordinator::new(vec![action_prov, clip_prov, calc_prov, quick_prov]);

        // 1. '/' scopes to actions
        let sink = SearchSink::for_test(1);
        coordinator.search("/toggle", 1, &sink);
        let res = sink.snapshot();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].category, ResultCategory::Action);

        // 2. ';' scopes to clipboard
        let sink = SearchSink::for_test(2);
        coordinator.search(";copied", 2, &sink);
        let res = sink.snapshot();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].category, ResultCategory::Clipboard);

        // 3. '=' scopes to calculator
        let sink = SearchSink::for_test(3);
        coordinator.search("=5 * 5", 3, &sink);
        let res = sink.snapshot();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].title, "25");
        assert_eq!(res[0].category, ResultCategory::Calculator);

        // 4. '?' scopes to web search
        let sink = SearchSink::for_test(4);
        coordinator.search("?rust documentation", 4, &sink);
        let res = sink.snapshot();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].category, ResultCategory::WebSearch);

        // 5. '<' scopes to keybindings
        let sink = SearchSink::for_test(5);
        coordinator.search("<Super", 5, &sink);
        let res = sink.snapshot();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].category, ResultCategory::Keybinding);
    }

    #[test]
    fn test_implicit_and_explicit_calculator_scoping() {
        use crate::shell::overview_search::CalculatorSearchProvider;

        let calc_prov = Arc::new(CalculatorSearchProvider::new());
        let coordinator = SearchCoordinator::new(vec![calc_prov]);

        // Bare implicit arithmetic
        let sink1 = SearchSink::for_test(1);
        coordinator.search("2 + 2", 1, &sink1);
        let res1 = sink1.snapshot();
        assert_eq!(res1.len(), 1);
        assert_eq!(res1[0].title, "4");

        // Complex implicit expression with parenthesis
        let sink2 = SearchSink::for_test(2);
        coordinator.search("10 * (5 - 3)", 2, &sink2);
        let res2 = sink2.snapshot();
        assert_eq!(res2.len(), 1);
        assert_eq!(res2[0].title, "20");

        // Non-arithmetic query "hello 2" parses as Default and is NOT dispatched to Calculator
        let sink3 = SearchSink::for_test(3);
        coordinator.search("hello 2", 3, &sink3);
        let res3 = sink3.snapshot();
        assert_eq!(res3.len(), 0);
    }

    #[test]
    fn test_unscoped_query_returns_ordered_mix_from_multiple_providers() {
        use std::path::PathBuf;

        use shilpo_services::{AppScanner, Application};

        use crate::actions::{ActionCategory, ActionDescriptor, ActionId, ActionInputRequirement};
        use crate::shell::overview_search::{
            ActionSearchProvider, AppSearchProvider, QuicklinksSearchProvider,
        };

        let scanner = AppScanner::from_applications(vec![Application {
            name: "Terminal Emulator".to_string(),
            description: Some("System terminal".to_string()),
            exec: "terminal".to_string(),
            icon: None,
            icon_path: None,
            desktop_file: PathBuf::from("/usr/share/applications/term.desktop"),
            categories: vec!["System".to_string()],
            working_dir: None,
            terminal: false,
            try_exec: None,
        }]);

        let actions = vec![ActionDescriptor {
            id: ActionId::ToggleOverview,
            name: "terminal-toggle".to_string(),
            label: "Terminal Toggle Action".to_string(),
            category: ActionCategory::Overlay,
            input: ActionInputRequirement::NoInput,
            enabled: true,
        }];

        let app_prov = Arc::new(AppSearchProvider::new(scanner));
        let action_prov = Arc::new(ActionSearchProvider::new(actions));
        let quick_prov = Arc::new(QuicklinksSearchProvider::new(Vec::new()));

        let coordinator = SearchCoordinator::new(vec![app_prov, action_prov, quick_prov]);

        let sink = SearchSink::for_test(1);
        coordinator.search("terminal", 1, &sink);
        let results = sink.snapshot();

        // Must contain results from AppSearchProvider, ActionSearchProvider, and QuicklinksSearchProvider
        assert!(
            results
                .iter()
                .any(|c| c.category == ResultCategory::Application)
        );
        assert!(results.iter().any(|c| c.category == ResultCategory::Action));
        assert!(
            results
                .iter()
                .any(|c| c.category == ResultCategory::Command)
        );
        assert!(
            results
                .iter()
                .any(|c| c.category == ResultCategory::WebSearch)
        );

        // Application (prior 200) outranks Command (prior 50) and WebSearch (prior 20)
        let app_idx = results
            .iter()
            .position(|c| c.category == ResultCategory::Application)
            .unwrap();
        let cmd_idx = results
            .iter()
            .position(|c| c.category == ResultCategory::Command)
            .unwrap();
        let web_idx = results
            .iter()
            .position(|c| c.category == ResultCategory::WebSearch)
            .unwrap();

        assert!(app_idx < cmd_idx);
        assert!(cmd_idx < web_idx);
    }

    #[test]
    fn test_every_action_result_variant_produced_by_domain_providers_via_coordinator() {
        use shilpo_services::{
            AppScanner, Application, ClipboardItem, CompositorCapabilities, CompositorSnapshot,
            DomainLifecycle, DomainVersion, TestCompositorAdapter, WindowInfo,
        };
        use std::path::PathBuf;
        use tokio::sync::watch;

        use crate::actions::{ActionCategory, ActionDescriptor, ActionId, ActionInputRequirement};
        use crate::shell::overview_search::{
            ActionSearchProvider, AppSearchProvider, CalculatorSearchProvider,
            ClipboardSearchProvider, QuicklinksSearchProvider, WindowSearchProvider,
        };

        let snapshot = CompositorSnapshot {
            version: DomainVersion::new(1, 1),
            connection: DomainLifecycle::Ready,
            capabilities: CompositorCapabilities {
                can_focus_window: true,
                ..Default::default()
            },
            windows: vec![WindowInfo {
                id: 42,
                title: Some("Terminal".to_string()),
                app_id: Some("org.gnome.Terminal".to_string()),
                workspace_id: Some(1),
                is_focused: false,
                is_floating: false,
                is_urgent: false,
                layout_x: None,
                layout_y: None,
            }],
            ..Default::default()
        };

        let scanner = AppScanner::from_applications(vec![Application {
            name: "Calculator App".to_string(),
            description: None,
            exec: "gnome-calculator".to_string(),
            icon: None,
            icon_path: None,
            desktop_file: PathBuf::from("/usr/share/applications/calc.desktop"),
            categories: Vec::new(),
            working_dir: None,
            terminal: false,
            try_exec: None,
        }]);

        let actions = vec![ActionDescriptor {
            id: ActionId::Quit,
            name: "quit".to_string(),
            label: "Quit Shilpo".to_string(),
            category: ActionCategory::System,
            input: ActionInputRequirement::NoInput,
            enabled: true,
        }];

        let (_tx, rx) = watch::channel(vec![ClipboardItem::new_text(
            "saved text".to_string(),
            chrono::Utc::now(),
        )]);

        let keybindings = vec![("Super+L".to_string(), "Lock Screen".to_string())];

        let app_prov = Arc::new(AppSearchProvider::new(scanner));
        let win_prov = Arc::new(WindowSearchProvider::new(Some(Arc::new(
            TestCompositorAdapter::new(snapshot),
        ))));
        let act_prov = Arc::new(ActionSearchProvider::new(actions));
        let clip_prov = Arc::new(ClipboardSearchProvider::new(Some(rx)));
        let calc_prov = Arc::new(CalculatorSearchProvider::new());
        let quick_prov = Arc::new(QuicklinksSearchProvider::new(keybindings));

        let coordinator = SearchCoordinator::new(vec![
            app_prov.clone(),
            win_prov.clone(),
            act_prov.clone(),
            clip_prov.clone(),
            calc_prov.clone(),
            quick_prov.clone(),
        ]);

        // 1. LaunchApp
        let sink = SearchSink::for_test(1);
        coordinator.search(">Calc", 1, &sink);
        let cand = &sink.snapshot()[0];
        let res = coordinator
            .activate(
                &cand.provider_id,
                &cand.canonical_id,
                cand.activation.clone(),
            )
            .unwrap();
        assert!(matches!(res, ActionResult::LaunchApp(_)));

        // 2. Handled (Window)
        let sink = SearchSink::for_test(2);
        coordinator.search("Terminal", 2, &sink);
        let win_cand = sink
            .snapshot()
            .into_iter()
            .find(|c| c.category == ResultCategory::Window)
            .unwrap();
        let res = coordinator
            .activate(
                &win_cand.provider_id,
                &win_cand.canonical_id,
                win_cand.activation.clone(),
            )
            .unwrap();
        assert!(matches!(
            res,
            ActionResult::Handled {
                close_overview: true
            }
        ));

        // 3. InvokeAction
        let sink = SearchSink::for_test(3);
        coordinator.search("/quit", 3, &sink);
        let act_cand = &sink.snapshot()[0];
        let res = coordinator
            .activate(
                &act_cand.provider_id,
                &act_cand.canonical_id,
                act_cand.activation.clone(),
            )
            .unwrap();
        assert!(matches!(res, ActionResult::InvokeAction(_)));

        // 4. CopyClipboard
        let sink = SearchSink::for_test(4);
        coordinator.search(";saved", 4, &sink);
        let clip_cand = &sink.snapshot()[0];
        let res = coordinator
            .activate(
                &clip_cand.provider_id,
                &clip_cand.canonical_id,
                clip_cand.activation.clone(),
            )
            .unwrap();
        assert!(matches!(res, ActionResult::CopyClipboard(_)));

        // 5. CopyCalculation
        let sink = SearchSink::for_test(5);
        coordinator.search("=100 + 200", 5, &sink);
        let calc_cand = &sink.snapshot()[0];
        let res = coordinator
            .activate(
                &calc_cand.provider_id,
                &calc_cand.canonical_id,
                calc_cand.activation.clone(),
            )
            .unwrap();
        assert_eq!(res, ActionResult::CopyCalculation("300".to_string()));

        // 6. OpenPath
        let sink = SearchSink::for_test(6);
        coordinator.search("~/", 6, &sink);
        let path_cand = sink
            .snapshot()
            .into_iter()
            .find(|c| c.category == ResultCategory::FilePath)
            .unwrap();
        let res = coordinator
            .activate(
                &path_cand.provider_id,
                &path_cand.canonical_id,
                path_cand.activation.clone(),
            )
            .unwrap();
        let home = std::env::var("HOME").unwrap();
        assert_eq!(res, ActionResult::OpenPath(PathBuf::from(home)));

        // 7. OpenUri
        let sink = SearchSink::for_test(7);
        coordinator.search("https://example.com/test", 7, &sink);
        let uri_cand = sink
            .snapshot()
            .into_iter()
            .find(|c| c.category == ResultCategory::Uri)
            .unwrap();
        let res = coordinator
            .activate(
                &uri_cand.provider_id,
                &uri_cand.canonical_id,
                uri_cand.activation.clone(),
            )
            .unwrap();
        assert_eq!(
            res,
            ActionResult::OpenUri("https://example.com/test".to_string())
        );

        // 8. ExecuteCommand
        let sink = SearchSink::for_test(8);
        coordinator.search("$echo hi", 8, &sink);
        if let Some(cmd_cand) = sink
            .snapshot()
            .into_iter()
            .find(|c| c.category == ResultCategory::Command)
        {
            let res = coordinator
                .activate(
                    &cmd_cand.provider_id,
                    &cmd_cand.canonical_id,
                    cmd_cand.activation.clone(),
                )
                .unwrap();
            assert_eq!(res, ActionResult::ExecuteCommand("echo hi".to_string()));
        }

        // 9. OpenWeb
        let sink = SearchSink::for_test(9);
        coordinator.search("?rust lang", 9, &sink);
        let web_cand = sink
            .snapshot()
            .into_iter()
            .find(|c| c.category == ResultCategory::WebSearch)
            .unwrap();
        let res = coordinator
            .activate(
                &web_cand.provider_id,
                &web_cand.canonical_id,
                web_cand.activation.clone(),
            )
            .unwrap();
        assert_eq!(
            res,
            ActionResult::OpenWeb("https://www.google.com/search?q=rust+lang".to_string())
        );

        // 10. CopyKeybinding
        let sink = SearchSink::for_test(10);
        coordinator.search("<Super", 10, &sink);
        let key_cand = sink
            .snapshot()
            .into_iter()
            .find(|c| c.category == ResultCategory::Keybinding)
            .unwrap();
        let res = coordinator
            .activate(
                &key_cand.provider_id,
                &key_cand.canonical_id,
                key_cand.activation.clone(),
            )
            .unwrap();
        assert_eq!(res, ActionResult::CopyKeybinding("Super+L".to_string()));
    }
}
