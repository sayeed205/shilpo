use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use shilpo_ext_api::{CanonicalId, ExtensionId};
use shilpo_ext_runtime::{HostError, RuntimeBudget};
use shilpo_ui::IconName;

use super::{
    parser::SearchMode,
    sink::SearchSink,
    types::{
        ActionResult, CompletionState, LatencyClass, ProviderId, ResultCategory, SearchActivation,
        SearchCandidate, SearchError, SearchProvider, SearchRequest, SearchResultIcon,
    },
};

pub const EXTENSION_MAX_CANDIDATES: usize = 64;
pub const DEFAULT_EXTENSION_SEARCH_BUDGET: Duration = Duration::from_millis(50);

/// Abstraction allowing `ExtensionSearchProvider` to query the extension host adapter or mocks.
pub trait ExtensionSearchRunner: Send + Sync {
    fn search(
        &self,
        canonical: &CanonicalId,
        request: &shilpo_ext_api::bindings::shilpo::extension::types::SearchRequest,
        budget: RuntimeBudget,
    ) -> Result<Vec<shilpo_ext_api::bindings::shilpo::extension::types::SearchCandidate>, HostError>;
}

/// Constructs a secure host-namespaced candidate identity that guests cannot forge.
pub fn host_namespace_candidate_id(
    extension_id: &ExtensionId,
    contribution_id: &str,
    local_id: &str,
) -> String {
    let sanitized_local: String = local_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("ext:{extension_id}/{contribution_id}/{sanitized_local}")
}

/// Derives the execution deadline for guest search queries.
pub fn derive_guest_deadline(
    runtime_budget: Duration,
    remaining_query_budget: Duration,
) -> Duration {
    runtime_budget.min(remaining_query_budget)
}

/// Validates that an asset path is strictly relative and contains no path traversal components (`..`).
pub fn sanitize_relative_asset_path(path: &str) -> Option<PathBuf> {
    let p = Path::new(path);
    if p.is_absolute() {
        return None;
    }
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(_) => {}
            _ => return None,
        }
    }
    if path.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(path))
}

/// Resolves standard icon names into the UI's `IconName` enum.
pub fn parse_named_icon(name: &str) -> Option<IconName> {
    match name.to_lowercase().replace(['-', ' '], "_").as_str() {
        "search" => Some(IconName::Search),
        "settings" => Some(IconName::Settings),
        "terminal" => Some(IconName::Terminal),
        "star" => Some(IconName::Star),
        "folder" => Some(IconName::Folder),
        "info" => Some(IconName::Info),
        "refresh" => Some(IconName::Refresh),
        "close" => Some(IconName::Close),
        "check" => Some(IconName::Check),
        "add" => Some(IconName::Add),
        "copy" | "content_copy" => Some(IconName::ContentCopy),
        "bluetooth" => Some(IconName::Bluetooth),
        "wifi" => Some(IconName::AndroidWifi4Bar),
        "battery" => Some(IconName::BatteryAndroidFull),
        "volume" | "audio" => Some(IconName::Airwave),
        "palette" => Some(IconName::Palette),
        "sunny" | "sun" => Some(IconName::Sunny),
        "moon" | "moon_stars" => Some(IconName::MoonStars),
        _ => None,
    }
}

fn convert_result_category(
    cat: shilpo_ext_api::bindings::shilpo::extension::types::SearchResultCategory,
) -> ResultCategory {
    use shilpo_ext_api::bindings::shilpo::extension::types as api_types;
    match cat {
        api_types::SearchResultCategory::Window => ResultCategory::Window,
        api_types::SearchResultCategory::Application => ResultCategory::Application,
        api_types::SearchResultCategory::Action => ResultCategory::Action,
        api_types::SearchResultCategory::Clipboard => ResultCategory::Clipboard,
        api_types::SearchResultCategory::Calculator => ResultCategory::Calculator,
        api_types::SearchResultCategory::Command => ResultCategory::Command,
        api_types::SearchResultCategory::WebSearch => ResultCategory::WebSearch,
        api_types::SearchResultCategory::FilePath => ResultCategory::FilePath,
        api_types::SearchResultCategory::Uri => ResultCategory::Uri,
        api_types::SearchResultCategory::Keybinding => ResultCategory::Keybinding,
        api_types::SearchResultCategory::Custom => ResultCategory::Custom,
    }
}

fn convert_search_icon(
    icon: Option<shilpo_ext_api::bindings::shilpo::extension::types::SearchIcon>,
    extension_id: &ExtensionId,
    default_icon: IconName,
) -> SearchResultIcon {
    use shilpo_ext_api::bindings::shilpo::extension::types as api_types;
    match icon {
        Some(api_types::SearchIcon::Named(name)) => {
            if let Some(icon_name) = parse_named_icon(&name) {
                SearchResultIcon::Named(icon_name)
            } else {
                SearchResultIcon::Named(default_icon)
            }
        }
        Some(api_types::SearchIcon::Asset(rel_path)) => {
            if let Some(safe_path) = sanitize_relative_asset_path(&rel_path) {
                SearchResultIcon::ExtensionAsset {
                    extension_id: extension_id.clone(),
                    relative_path: safe_path,
                }
            } else {
                SearchResultIcon::Named(default_icon)
            }
        }
        Some(api_types::SearchIcon::None) | None => SearchResultIcon::Named(default_icon),
    }
}

/// Search provider that queries an active extension via the canonical WIT interface.
pub struct ExtensionSearchProvider {
    extension_id: ExtensionId,
    contribution_id: String,
    canonical_id: CanonicalId,
    modes: Vec<SearchMode>,
    runner: Arc<dyn ExtensionSearchRunner>,
    runtime_budget: Duration,
}

impl ExtensionSearchProvider {
    pub fn new(
        extension_id: ExtensionId,
        contribution_id: impl Into<String>,
        modes: Vec<SearchMode>,
        runner: Arc<dyn ExtensionSearchRunner>,
    ) -> Self {
        let contrib_str = contribution_id.into();
        let contrib_id = shilpo_ext_api::ContributionId::new(&contrib_str)
            .unwrap_or_else(|_| shilpo_ext_api::ContributionId::new("default").unwrap());
        let canonical_id = CanonicalId::new(extension_id.clone(), contrib_id);
        Self {
            extension_id,
            contribution_id: contrib_str,
            canonical_id,
            modes,
            runner,
            runtime_budget: DEFAULT_EXTENSION_SEARCH_BUDGET,
        }
    }

    pub fn with_budget(mut self, budget: Duration) -> Self {
        self.runtime_budget = budget;
        self
    }

    pub fn extension_id(&self) -> &ExtensionId {
        &self.extension_id
    }

    pub fn contribution_id(&self) -> &str {
        &self.contribution_id
    }

    pub fn canonical_id(&self) -> &CanonicalId {
        &self.canonical_id
    }
}

impl SearchProvider for ExtensionSearchProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new(format!(
            "ext:{}/{}",
            self.extension_id, self.contribution_id
        ))
    }

    fn declared_modes(&self) -> Cow<'static, [SearchMode]> {
        Cow::Owned(self.modes.clone())
    }

    fn search(&self, request: SearchRequest, sink: SearchSink) {
        use shilpo_ext_api::bindings::shilpo::extension::types as api_types;

        let api_mode = match request.mode {
            SearchMode::Default => api_types::SearchMode::Default,
            SearchMode::Apps => api_types::SearchMode::Apps,
            SearchMode::Actions => api_types::SearchMode::Actions,
            SearchMode::Clipboard => api_types::SearchMode::Clipboard,
            SearchMode::Calculator => api_types::SearchMode::Calculator,
            SearchMode::Command => api_types::SearchMode::Command,
            SearchMode::WebSearch => api_types::SearchMode::WebSearch,
            SearchMode::Keybindings => api_types::SearchMode::Keybindings,
        };

        let api_request = api_types::SearchRequest {
            raw_query: request.raw_query.clone(),
            query: request.query.clone(),
            mode: api_mode,
            generation: request.generation,
        };

        let budget = RuntimeBudget::default();

        let candidates = match self.runner.search(&self.canonical_id, &api_request, budget) {
            Ok(candidates) => candidates,
            Err(error) => {
                tracing::debug!(
                    %error,
                    extension = %self.extension_id,
                    contribution = %self.contribution_id,
                    "extension search provider query failed or circuit is open"
                );
                return;
            }
        };

        let default_icon = request.mode.default_icon();
        for raw_cand in candidates.into_iter().take(EXTENSION_MAX_CANDIDATES) {
            let namespaced_id = host_namespace_candidate_id(
                &self.extension_id,
                &self.contribution_id,
                &raw_cand.id,
            );
            let category = convert_result_category(raw_cand.category);
            let icon = convert_search_icon(raw_cand.icon, &self.extension_id, default_icon);

            let mut cand = SearchCandidate::new(
                self.id(),
                namespaced_id,
                request.generation,
                raw_cand.title,
                raw_cand.subtitle,
                category,
                icon,
                raw_cand.activation_verb,
                SearchActivation::new(raw_cand.activation_payload),
            );
            cand.aliases = raw_cand.aliases;
            cand.keywords = raw_cand.keywords;
            cand.latency = LatencyClass::Fast;
            cand.completion = CompletionState::Complete;

            sink.push(cand);
        }
    }

    fn activate(&self, activation: SearchActivation) -> Result<ActionResult, SearchError> {
        Ok(ActionResult::InvokeExtension {
            canonical: self.canonical_id.clone(),
            payload: activation.payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_ext_api::{ContributionId, bindings::shilpo::extension::types as api_types};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockRunner {
        candidates: Vec<api_types::SearchCandidate>,
        should_fail: bool,
        call_count: Arc<AtomicUsize>,
    }

    impl ExtensionSearchRunner for MockRunner {
        fn search(
            &self,
            _canonical: &CanonicalId,
            _request: &api_types::SearchRequest,
            _budget: RuntimeBudget,
        ) -> Result<Vec<api_types::SearchCandidate>, HostError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail {
                Err(HostError::Runtime(shilpo_ext_runtime::RuntimeError::new(
                    "guest failed",
                )))
            } else {
                Ok(self.candidates.clone())
            }
        }
    }

    #[test]
    fn test_host_namespace_candidate_id() {
        let ext_id = ExtensionId::new("io.example.foo").unwrap();
        let namespaced = host_namespace_candidate_id(&ext_id, "my-search", "item/123:test");
        assert_eq!(namespaced, "ext:io.example.foo/my-search/item_123_test");
    }

    #[test]
    fn test_sanitize_relative_asset_path() {
        assert_eq!(
            sanitize_relative_asset_path("icons/test.png"),
            Some(PathBuf::from("icons/test.png"))
        );
        assert_eq!(sanitize_relative_asset_path("../escape.png"), None);
        assert_eq!(sanitize_relative_asset_path("/absolute/path.png"), None);
        assert_eq!(sanitize_relative_asset_path(""), None);
    }

    #[test]
    fn test_derive_guest_deadline() {
        assert_eq!(
            derive_guest_deadline(Duration::from_millis(50), Duration::from_millis(100)),
            Duration::from_millis(50)
        );
        assert_eq!(
            derive_guest_deadline(Duration::from_millis(50), Duration::from_millis(20)),
            Duration::from_millis(20)
        );
    }

    #[test]
    fn test_extension_search_provider_success() {
        let ext_id = ExtensionId::new("org.shilpo.demo").unwrap();
        let call_count = Arc::new(AtomicUsize::new(0));
        let runner = Arc::new(MockRunner {
            candidates: vec![api_types::SearchCandidate {
                id: "cand-1".into(),
                title: "Candidate 1".into(),
                subtitle: Some("Subtitle 1".into()),
                aliases: vec!["c1".into()],
                keywords: vec!["demo".into()],
                category: api_types::SearchResultCategory::Action,
                icon: Some(api_types::SearchIcon::Named("settings".into())),
                activation_verb: "Run".into(),
                activation_payload: "run-cand-1".into(),
            }],
            should_fail: false,
            call_count: call_count.clone(),
        });

        let provider = ExtensionSearchProvider::new(
            ext_id.clone(),
            "search-prov",
            vec![SearchMode::Default, SearchMode::Actions],
            runner,
        );

        assert_eq!(provider.id().as_str(), "ext:org.shilpo.demo/search-prov");
        assert_eq!(
            provider.declared_modes().as_ref(),
            &[SearchMode::Default, SearchMode::Actions]
        );

        let sink = SearchSink::for_test(1);
        provider.search(
            SearchRequest::new("cand", SearchMode::Default, "cand", 1),
            sink.clone(),
        );

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        let results = sink.snapshot();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].canonical_id,
            "ext:org.shilpo.demo/search-prov/cand-1"
        );
        assert_eq!(results[0].title, "Candidate 1");
        assert_eq!(results[0].category, ResultCategory::Action);
        assert_eq!(results[0].icon, SearchResultIcon::Named(IconName::Settings));
        assert_eq!(results[0].activation_verb, "Run");
        assert_eq!(results[0].activation.payload, "run-cand-1");

        let action_res = provider
            .activate(results[0].activation.clone())
            .expect("activate should succeed");
        assert_eq!(
            action_res,
            ActionResult::InvokeExtension {
                canonical: CanonicalId::new(ext_id, ContributionId::new("search-prov").unwrap(),),
                payload: "run-cand-1".into(),
            }
        );
    }

    #[test]
    fn test_extension_search_provider_failure_drops_candidates() {
        let ext_id = ExtensionId::new("org.shilpo.broken").unwrap();
        let call_count = Arc::new(AtomicUsize::new(0));
        let runner = Arc::new(MockRunner {
            candidates: vec![],
            should_fail: true,
            call_count: call_count.clone(),
        });

        let provider =
            ExtensionSearchProvider::new(ext_id, "search-prov", vec![SearchMode::Default], runner);

        let sink = SearchSink::for_test(1);
        provider.search(
            SearchRequest::new("cand", SearchMode::Default, "cand", 1),
            sink.clone(),
        );

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        let results = sink.snapshot();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_extension_search_provider_caps_at_64_candidates() {
        let ext_id = ExtensionId::new("org.shilpo.quota").unwrap();
        let candidates: Vec<_> = (0..100)
            .map(|i| api_types::SearchCandidate {
                id: format!("item-{i}"),
                title: format!("Item {i}"),
                subtitle: None,
                aliases: vec![],
                keywords: vec![],
                category: api_types::SearchResultCategory::Custom,
                icon: None,
                activation_verb: "Open".into(),
                activation_payload: format!("payload-{i}"),
            })
            .collect();

        let runner = Arc::new(MockRunner {
            candidates,
            should_fail: false,
            call_count: Arc::new(AtomicUsize::new(0)),
        });

        let provider =
            ExtensionSearchProvider::new(ext_id, "quota-search", vec![SearchMode::Default], runner);

        let sink = SearchSink::for_test(1);
        provider.search(
            SearchRequest::new("q", SearchMode::Default, "q", 1),
            sink.clone(),
        );

        let results = sink.snapshot();
        assert_eq!(results.len(), EXTENSION_MAX_CANDIDATES);
    }
}
