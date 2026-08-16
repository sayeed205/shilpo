use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use shilpo_services::{AppScanner, Application, ClipboardItem};
use shilpo_ui::IconName;

use super::{
    calculator,
    parser::SearchMode,
    ranking,
    sink::SearchSink,
    types::{
        ActionResult, CompletionState, LatencyClass, ProviderId, ResultCategory, SearchActivation,
        SearchCandidate, SearchError, SearchProvider, SearchRequest, SearchResultIcon,
    },
};
use crate::actions::ActionDescriptor;

/// Transitional legacy search intent enum.
///
/// Kept strictly private to this legacy adapter. Will be deleted in #204 (Part of #133).
#[derive(Debug, Clone)]
pub(crate) enum SearchIntent {
    LaunchApp(Application),
    InvokeAction(ActionDescriptor),
    CopyClipboard(ClipboardItem),
    CopyCalculation(String),
    ExecuteCommand(String),
    OpenWeb(String),
    OpenPath(PathBuf),
    OpenUri(String),
    CopyKeybinding(String),
}

/// Transitional legacy provider wrapping the pre-refactor synchronous OverviewSearch engine.
///
/// Marked as transitional: this adapter will be decomposed into independent domain providers
/// (AppSearchProvider, WindowSearchProvider, ActionSearchProvider, etc.) and removed
/// in #204 (Part of #133).
#[derive(Clone)]
pub struct LegacyOverviewSearchProvider {
    scanner: AppScanner,
    actions: Vec<ActionDescriptor>,
    clipboard_history: Vec<ClipboardItem>,
    keybindings: Vec<(String, String)>,
    cached_intents: Arc<Mutex<HashMap<String, SearchIntent>>>,
}

pub type OverviewSearch = LegacyOverviewSearchProvider;

impl LegacyOverviewSearchProvider {
    pub fn new(
        scanner: AppScanner,
        actions: Vec<ActionDescriptor>,
        clipboard_history: Vec<ClipboardItem>,
        keybindings: Vec<(String, String)>,
    ) -> Self {
        Self {
            scanner,
            actions,
            clipboard_history,
            keybindings,
            cached_intents: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl SearchProvider for LegacyOverviewSearchProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from_static("legacy-overview-search")
    }

    fn search(&self, request: SearchRequest, sink: SearchSink) {
        let mode = request.mode;
        let query = &request.query;
        let query_generation = request.generation;
        let provider_id = self.id();

        let mut candidates = Vec::new();
        let mut intents = self.cached_intents.lock().unwrap();

        match mode {
            SearchMode::Default => {
                if let Some(path) = ranking::expand_path(&request.raw_query) {
                    let canonical_id = format!("path:{}", path.display());
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::OpenPath(path.clone()));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("File")
                            .to_string(),
                        subtitle: Some(path.display().to_string()),
                        aliases: Vec::new(),
                        keywords: Vec::new(),
                        category: ResultCategory::FilePath,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Folder),
                        activation_verb: "Open path".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                } else if ranking::is_uri_spec(&request.raw_query) {
                    let uri = request.raw_query.trim().to_string();
                    let canonical_id = format!("uri:{}", uri);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::OpenUri(uri.clone()));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: uri,
                        subtitle: Some("Web or protocol URI".to_string()),
                        aliases: Vec::new(),
                        keywords: Vec::new(),
                        category: ResultCategory::Uri,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Star),
                        activation_verb: "Open link".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }

                let mut seen_execs = HashSet::new();
                for app in self.scanner.applications() {
                    if !seen_execs.insert(app.exec.clone()) {
                        continue;
                    }
                    let canonical_id = format!("app:{}", app.exec);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::LaunchApp(app.clone()));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: app.name.clone(),
                        subtitle: Some(app.description.clone().unwrap_or_else(|| app.exec.clone())),
                        aliases: vec![app.exec.clone()],
                        keywords: app.categories.clone(),
                        category: ResultCategory::Application,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::AppIcon(app.icon_path.clone()),
                        activation_verb: "Launch".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }

                for action in &self.actions {
                    if !action.input.can_invoke_without_input() {
                        continue;
                    }
                    let canonical_id = format!("action:{}", action.id);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::InvokeAction(action.clone()));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: action.label.clone(),
                        subtitle: Some(format!("System Action ({})", action.name)),
                        aliases: vec![action.name.clone()],
                        keywords: Vec::new(),
                        category: ResultCategory::Action,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Settings),
                        activation_verb: "Run".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }

                if !query.trim().is_empty() {
                    let command = query.trim().to_string();
                    let canonical_id = format!("cmd:{}", command);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(
                        act_key.clone(),
                        SearchIntent::ExecuteCommand(command.clone()),
                    );

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: command,
                        subtitle: Some("Run command".to_string()),
                        aliases: Vec::new(),
                        keywords: Vec::new(),
                        category: ResultCategory::Command,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Terminal),
                        activation_verb: "Run".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }

                if !query.trim().is_empty() {
                    let q = query.trim();
                    let url = format!(
                        "https://www.google.com/search?q={}",
                        percent_encode_query(q)
                    );
                    let canonical_id = format!("web:{}", url);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::OpenWeb(url));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: q.to_string(),
                        subtitle: Some("Search the web".to_string()),
                        aliases: Vec::new(),
                        keywords: Vec::new(),
                        category: ResultCategory::WebSearch,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Search),
                        activation_verb: "Search".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }
            }
            SearchMode::Apps => {
                let mut seen_execs = HashSet::new();
                for app in self.scanner.applications() {
                    if !seen_execs.insert(app.exec.clone()) {
                        continue;
                    }
                    let canonical_id = format!("app:{}", app.exec);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::LaunchApp(app.clone()));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: app.name.clone(),
                        subtitle: Some(app.description.clone().unwrap_or_else(|| app.exec.clone())),
                        aliases: vec![app.exec.clone()],
                        keywords: app.categories.clone(),
                        category: ResultCategory::Application,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::AppIcon(app.icon_path.clone()),
                        activation_verb: "Launch".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }
            }
            SearchMode::Actions => {
                for action in &self.actions {
                    if !action.input.can_invoke_without_input() {
                        continue;
                    }
                    let canonical_id = format!("action:{}", action.id);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::InvokeAction(action.clone()));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: action.label.clone(),
                        subtitle: Some(format!("System Action ({})", action.name)),
                        aliases: vec![action.name.clone()],
                        keywords: Vec::new(),
                        category: ResultCategory::Action,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Settings),
                        activation_verb: "Run".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }
            }
            SearchMode::Clipboard => {
                for item in &self.clipboard_history {
                    let canonical_id = format!("clipboard:{}", item.id);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::CopyClipboard(item.clone()));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: item.text.clone(),
                        subtitle: Some(format!("Copied at {}", item.timestamp)),
                        aliases: Vec::new(),
                        keywords: Vec::new(),
                        category: ResultCategory::Clipboard,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Star),
                        activation_verb: "Copy".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }
            }
            SearchMode::Calculator => {
                if let Some(val) = calculator::evaluate_expression(query) {
                    let canonical_id = format!("calc:{}", val);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::CopyCalculation(val.clone()));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: val,
                        subtitle: Some(format!("= {}", query)),
                        aliases: Vec::new(),
                        keywords: Vec::new(),
                        category: ResultCategory::Calculator,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Star),
                        activation_verb: "Copy result".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }
            }
            SearchMode::Command => {
                if !query.trim().is_empty() && shilpo_services::find_terminal_emulator().is_some() {
                    let cmd = query.trim().to_string();
                    let canonical_id = format!("cmd:{}", cmd);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::ExecuteCommand(cmd.clone()));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: format!("$ {}", cmd),
                        subtitle: Some("Execute shell command in terminal".to_string()),
                        aliases: Vec::new(),
                        keywords: Vec::new(),
                        category: ResultCategory::Command,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Terminal),
                        activation_verb: "Run command".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }
            }
            SearchMode::WebSearch => {
                if !query.trim().is_empty() {
                    let encoded = percent_encode_query(query.trim());
                    let url = format!("https://www.google.com/search?q={}", encoded);
                    let canonical_id = format!("web:{}", url);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(act_key.clone(), SearchIntent::OpenWeb(url.clone()));

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: format!("Search Google for \"{}\"", query.trim()),
                        subtitle: Some(url),
                        aliases: Vec::new(),
                        keywords: Vec::new(),
                        category: ResultCategory::WebSearch,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Search),
                        activation_verb: "Search".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }
            }
            SearchMode::Keybindings => {
                for (shortcut, label) in &self.keybindings {
                    let canonical_id = format!("keybinding:{}", shortcut);
                    let act_key = format!("legacy:{query_generation}:{canonical_id}");
                    intents.insert(
                        act_key.clone(),
                        SearchIntent::CopyKeybinding(shortcut.clone()),
                    );

                    candidates.push(SearchCandidate {
                        provider_id: provider_id.clone(),
                        canonical_id,
                        generation: query_generation,
                        title: shortcut.clone(),
                        subtitle: Some(label.clone()),
                        aliases: Vec::new(),
                        keywords: Vec::new(),
                        category: ResultCategory::Keybinding,
                        latency: LatencyClass::Instant,
                        completion: CompletionState::Complete,
                        icon: SearchResultIcon::Named(IconName::Star),
                        activation_verb: "Copy shortcut".to_string(),
                        match_positions: Vec::new(),
                        activation: SearchActivation::new(act_key),
                    });
                }
            }
        }

        drop(intents);

        for candidate in candidates {
            sink.push(candidate);
        }
    }

    fn activate(&self, activation: SearchActivation) -> Result<ActionResult, SearchError> {
        let intent = self
            .cached_intents
            .lock()
            .unwrap()
            .get(&activation.payload)
            .cloned()
            .ok_or_else(|| SearchError::NotFound(activation.payload.clone()))?;

        match intent {
            SearchIntent::LaunchApp(app) => Ok(ActionResult::LaunchApp(app)),
            SearchIntent::InvokeAction(action) => Ok(ActionResult::InvokeAction(action)),
            SearchIntent::CopyClipboard(item) => Ok(ActionResult::CopyClipboard(item)),
            SearchIntent::CopyCalculation(val) => Ok(ActionResult::CopyCalculation(val)),
            SearchIntent::ExecuteCommand(cmd) => Ok(ActionResult::ExecuteCommand(cmd)),
            SearchIntent::OpenWeb(url) => Ok(ActionResult::OpenWeb(url)),
            SearchIntent::OpenPath(path) => Ok(ActionResult::OpenPath(path)),
            SearchIntent::OpenUri(uri) => Ok(ActionResult::OpenUri(uri)),
            SearchIntent::CopyKeybinding(shortcut) => Ok(ActionResult::CopyKeybinding(shortcut)),
        }
    }
}

fn percent_encode_query(query: &str) -> String {
    let mut encoded = String::with_capacity(query.len());
    for byte in query.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else if byte == b' ' {
            encoded.push('+');
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actions::{ActionCategory, ActionDescriptor, ActionId, ActionInputRequirement},
        shell::overview_search::coordinator::SearchCoordinator,
    };

    fn create_test_provider() -> LegacyOverviewSearchProvider {
        let app1 = Application {
            name: "Calculator".to_string(),
            exec: "gnome-calculator".to_string(),
            icon: Some("accessories-calculator".to_string()),
            icon_path: None,
            description: Some("Perform arithmetic calculations".to_string()),
            categories: vec!["Utility".to_string(), "Calculator".to_string()],
            desktop_file: PathBuf::from("/usr/share/applications/org.gnome.Calculator.desktop"),
            working_dir: None,
            terminal: false,
            try_exec: None,
        };
        let app2 = Application {
            name: "Firefox".to_string(),
            exec: "firefox".to_string(),
            icon: Some("firefox".to_string()),
            icon_path: None,
            description: Some("Web Browser".to_string()),
            categories: vec!["Network".to_string(), "WebBrowser".to_string()],
            desktop_file: PathBuf::from("/usr/share/applications/firefox.desktop"),
            working_dir: None,
            terminal: false,
            try_exec: None,
        };
        let scanner = AppScanner::from_applications(vec![app1, app2]);
        let actions = vec![ActionDescriptor {
            id: ActionId::ToggleOverview,
            name: "toggle-overview".to_string(),
            label: "Toggle Overview".to_string(),
            category: ActionCategory::Overlay,
            input: ActionInputRequirement::NoInput,
            enabled: true,
        }];
        let clipboard_history = vec![ClipboardItem {
            id: 1,
            text: "hello world from clipboard".to_string(),
            timestamp: "12:00".to_string(),
        }];
        let keybindings = vec![("Super+Q".to_string(), "Close Window".to_string())];

        LegacyOverviewSearchProvider::new(scanner, actions, clipboard_history, keybindings)
    }

    #[test]
    fn encodes_reserved_and_unicode_query_bytes() {
        assert_eq!(
            percent_encode_query("rust & gpui/日本語"),
            "rust+%26+gpui%2F%E6%97%A5%E6%9C%AC%E8%AA%9E"
        );
    }

    #[test]
    fn test_legacy_provider_with_coordinator_reproduces_ranked_results() {
        let provider = create_test_provider();
        let coordinator = SearchCoordinator::new(vec![Arc::new(provider.clone())])
            .with_recent_apps(vec!["firefox".to_string()]);

        // 1. Default mode - empty query
        let sink = SearchSink::for_test(1);
        coordinator.search("", 1, &sink);
        let results = sink.snapshot();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].title, "Firefox");
        assert_eq!(results[0].category, ResultCategory::Application);
        assert_eq!(results[1].title, "Calculator");
        assert_eq!(results[2].title, "Toggle Overview");

        // 2. Default mode - calc query with fallbacks
        let sink = SearchSink::for_test(1);
        coordinator.search("calc", 1, &sink);
        let results = sink.snapshot();
        assert!(!results.is_empty());
        assert_eq!(results[0].title, "Calculator");
        assert!(
            results
                .iter()
                .any(|r| r.category == ResultCategory::Command)
        );
        assert!(
            results
                .iter()
                .any(|r| r.category == ResultCategory::WebSearch)
        );

        // 3. Apps mode
        let sink = SearchSink::for_test(1);
        coordinator.search(">firefox", 1, &sink);
        let results = sink.snapshot();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Firefox");

        // 4. Actions mode
        let sink = SearchSink::for_test(1);
        coordinator.search("/toggle", 1, &sink);
        let results = sink.snapshot();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Toggle Overview");

        // 5. Clipboard mode
        let sink = SearchSink::for_test(1);
        coordinator.search(";hello", 1, &sink);
        let results = sink.snapshot();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "hello world from clipboard");

        // 6. Explicit calculator mode
        let sink = SearchSink::for_test(1);
        coordinator.search("=2+2", 1, &sink);
        let results = sink.snapshot();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "4");

        // 7. Implicit calculator mode
        let sink = SearchSink::for_test(1);
        coordinator.search("2 + 2", 1, &sink);
        let results = sink.snapshot();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "4");

        // 8. Web search mode
        let sink = SearchSink::for_test(1);
        coordinator.search("?rust lang", 1, &sink);
        let results = sink.snapshot();
        assert_eq!(results.len(), 1);
        assert!(results[0].subtitle.as_ref().unwrap().contains("google.com"));

        // 9. Keybindings mode
        let sink = SearchSink::for_test(1);
        coordinator.search("<Super+", 1, &sink);
        let results = sink.snapshot();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Super+Q");
    }

    #[test]
    fn test_activation_through_activate_produces_same_effects() {
        let provider = create_test_provider();
        let coordinator = SearchCoordinator::new(vec![Arc::new(provider.clone())]);

        // 1. App activation
        let sink = SearchSink::for_test(1);
        coordinator.search(">firefox", 1, &sink);
        let cand = &sink.snapshot()[0];
        let action_res = provider.activate(cand.activation.clone()).unwrap();
        assert!(matches!(action_res, ActionResult::LaunchApp(app) if app.name == "Firefox"));

        // 2. Action activation
        let sink = SearchSink::for_test(1);
        coordinator.search("/toggle", 1, &sink);
        let cand = &sink.snapshot()[0];
        let action_res = provider.activate(cand.activation.clone()).unwrap();
        assert!(
            matches!(action_res, ActionResult::InvokeAction(action) if action.name == "toggle-overview")
        );

        // 3. Clipboard activation
        let sink = SearchSink::for_test(1);
        coordinator.search(";hello", 1, &sink);
        let cand = &sink.snapshot()[0];
        let action_res = provider.activate(cand.activation.clone()).unwrap();
        assert!(
            matches!(action_res, ActionResult::CopyClipboard(item) if item.text.contains("hello world"))
        );

        // 4. Calculator activation
        let sink = SearchSink::for_test(1);
        coordinator.search("=2+2", 1, &sink);
        let cand = &sink.snapshot()[0];
        let action_res = provider.activate(cand.activation.clone()).unwrap();
        assert!(matches!(action_res, ActionResult::CopyCalculation(val) if val == "4"));

        // 5. Keybinding activation
        let sink = SearchSink::for_test(1);
        coordinator.search("<Super+", 1, &sink);
        let cand = &sink.snapshot()[0];
        let action_res = provider.activate(cand.activation.clone()).unwrap();
        assert!(
            matches!(action_res, ActionResult::CopyKeybinding(shortcut) if shortcut == "Super+Q")
        );

        // 6. Web search activation
        let sink = SearchSink::for_test(1);
        coordinator.search("?rust", 1, &sink);
        let cand = &sink.snapshot()[0];
        let action_res = provider.activate(cand.activation.clone()).unwrap();
        assert!(matches!(action_res, ActionResult::OpenWeb(url) if url.contains("google.com")));

        // 7. Unknown activation payload
        let err = provider.activate(SearchActivation::new("nonexistent-key"));
        assert!(matches!(err, Err(SearchError::NotFound(_))));
    }
}
