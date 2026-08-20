use std::sync::Arc;

use shilpo_services::{CompositorAdapter, CompositorCommand};
use shilpo_ui::IconName;

use super::{
    parser::SearchMode,
    sink::SearchSink,
    types::{
        ActionResult, CompletionState, LatencyClass, ProviderId, ResultCategory, SearchActivation,
        SearchCandidate, SearchError, SearchProvider, SearchRequest, SearchResultIcon,
    },
};

/// Provider that searches open windows reported by the compositor.
#[derive(Clone)]
pub struct WindowSearchProvider {
    compositor: Option<Arc<dyn CompositorAdapter>>,
}

impl WindowSearchProvider {
    /// Creates a new window search provider capturing the compositor adapter.
    pub fn new(compositor: Option<Arc<dyn CompositorAdapter>>) -> Self {
        Self { compositor }
    }
}

impl SearchProvider for WindowSearchProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from_static("window-search")
    }

    fn declared_modes(&self) -> std::borrow::Cow<'static, [SearchMode]> {
        std::borrow::Cow::Borrowed(&[SearchMode::Default])
    }

    fn search(&self, request: SearchRequest, sink: SearchSink) {
        let Some(compositor) = &self.compositor else {
            return;
        };

        // Fresh read of compositor state on every search() invocation
        let snapshot = compositor.current();
        let query_generation = request.generation;
        let provider_id = self.id();

        for window in &snapshot.windows {
            let canonical_id = format!("window:{}", window.id);
            let title = window
                .title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .or_else(|| window.app_id.clone())
                .unwrap_or_else(|| format!("Window {}", window.id));

            let subtitle = match (&window.app_id, &window.title) {
                (Some(app_id), Some(_)) if !app_id.is_empty() => {
                    format!("Open window • {app_id}")
                }
                _ => "Open window".to_string(),
            };

            let mut aliases = Vec::new();
            if let Some(app_id) = &window.app_id
                && !app_id.is_empty()
                && app_id != &title
            {
                aliases.push(app_id.clone());
            }

            let candidate = SearchCandidate {
                provider_id: provider_id.clone(),
                canonical_id,
                generation: query_generation,
                title,
                subtitle: Some(subtitle),
                aliases,
                keywords: Vec::new(),
                category: ResultCategory::Window,
                latency: LatencyClass::Instant,
                completion: CompletionState::Complete,
                icon: SearchResultIcon::Named(IconName::Dashboard),
                activation_verb: "Switch to".to_string(),
                match_positions: Vec::new(),
                activation: SearchActivation::new(format!("window:{}", window.id)),
            };

            sink.push(candidate);
        }
    }

    fn activate(&self, activation: SearchActivation) -> Result<ActionResult, SearchError> {
        let Some(compositor) = &self.compositor else {
            return Err(SearchError::ActivationFailed(
                "compositor unavailable".into(),
            ));
        };

        let window_id = activation
            .payload
            .strip_prefix("window:")
            .unwrap_or(&activation.payload)
            .parse::<u64>()
            .map_err(|_| SearchError::NotFound(activation.payload.clone()))?;

        let ticket = compositor
            .command_broker()
            .submit(CompositorCommand::FocusWindow(window_id))
            .map_err(|err| SearchError::ActivationFailed(err.to_string()))?;
        ticket.detach();

        Ok(ActionResult::Handled {
            close_overview: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use shilpo_services::{
        CompositorCapabilities, CompositorSnapshot, DomainLifecycle, DomainVersion,
        TestCompositorAdapter, WindowInfo,
    };

    use super::*;

    fn make_window(id: u64, title: &str, app_id: &str) -> WindowInfo {
        WindowInfo {
            id,
            title: Some(title.to_string()),
            app_id: Some(app_id.to_string()),
            workspace_id: Some(1),
            is_focused: false,
            is_floating: false,
            is_urgent: false,
            layout_x: None,
            layout_y: None,
        }
    }

    fn ready_snapshot(windows: Vec<WindowInfo>) -> CompositorSnapshot {
        CompositorSnapshot {
            version: DomainVersion::new(1, 1),
            connection: DomainLifecycle::Ready,
            capabilities: CompositorCapabilities {
                can_focus_window: true,
                ..Default::default()
            },
            windows,
            ..Default::default()
        }
    }

    #[test]
    fn test_window_found_by_title_and_app_id() {
        let snapshot = ready_snapshot(vec![
            make_window(101, "Pull Request #202 - shilpo-rs/shilpo", "firefox"),
            make_window(102, "Alacritty Terminal", "Alacritty"),
        ]);

        let adapter = Arc::new(TestCompositorAdapter::new(snapshot));
        let provider = WindowSearchProvider::new(Some(adapter));
        let sink = SearchSink::for_test(1);

        provider.search(
            SearchRequest::new("", SearchMode::Default, "", 1),
            sink.clone(),
        );

        let results = sink.snapshot();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].canonical_id, "window:101");
        assert_eq!(results[0].category, ResultCategory::Window);
        assert!(results[0].aliases.contains(&"firefox".to_string()));

        assert_eq!(results[1].canonical_id, "window:102");
        assert!(results[1].aliases.contains(&"Alacritty".to_string()));
    }

    #[test]
    fn test_window_activation_submits_focus_command() {
        let snapshot = ready_snapshot(vec![make_window(42, "Editor", "zed")]);

        let adapter = Arc::new(TestCompositorAdapter::new(snapshot));
        let provider = WindowSearchProvider::new(Some(adapter.clone()));

        let activation_res = provider
            .activate(SearchActivation::new("window:42"))
            .unwrap();
        assert_eq!(
            activation_res,
            ActionResult::Handled {
                close_overview: true
            }
        );

        let start = std::time::Instant::now();
        let mut executed = Vec::new();
        while start.elapsed() < std::time::Duration::from_millis(500) {
            executed = adapter.executed_commands();
            if !executed.is_empty() {
                break;
            }
            std::thread::yield_now();
        }
        assert_eq!(executed, vec![CompositorCommand::FocusWindow(42)]);
    }

    #[test]
    fn test_consecutive_searches_read_fresh_compositor_snapshot() {
        let initial_snapshot = ready_snapshot(vec![make_window(1, "Window 1", "app1")]);

        let adapter = Arc::new(TestCompositorAdapter::new(initial_snapshot));
        let provider = WindowSearchProvider::new(Some(adapter.clone()));

        // First search
        let sink1 = SearchSink::for_test(1);
        provider.search(
            SearchRequest::new("", SearchMode::Default, "", 1),
            sink1.clone(),
        );
        assert_eq!(sink1.snapshot().len(), 1);

        // Update live snapshot
        let mut updated_snapshot = ready_snapshot(vec![
            make_window(1, "Window 1", "app1"),
            make_window(2, "Window 2", "app2"),
        ]);
        updated_snapshot.version = DomainVersion::new(1, 2);
        adapter.update(updated_snapshot);

        // Second search on same provider
        let sink2 = SearchSink::for_test(2);
        provider.search(
            SearchRequest::new("", SearchMode::Default, "", 2),
            sink2.clone(),
        );
        assert_eq!(
            sink2.snapshot().len(),
            2,
            "search() must reflect updated compositor windows on every invocation"
        );
    }

    #[test]
    fn test_none_compositor_or_windowless_snapshot_degrades_cleanly() {
        // 1. None compositor
        let provider_none = WindowSearchProvider::new(None);
        let sink_none = SearchSink::for_test(1);
        provider_none.search(
            SearchRequest::new("", SearchMode::Default, "", 1),
            sink_none.clone(),
        );
        assert_eq!(sink_none.snapshot().len(), 0);

        let act_err = provider_none.activate(SearchActivation::new("window:1"));
        assert!(matches!(act_err, Err(SearchError::ActivationFailed(_))));

        // 2. Empty windows snapshot
        let empty_adapter = Arc::new(TestCompositorAdapter::new(CompositorSnapshot::default()));
        let provider_empty = WindowSearchProvider::new(Some(empty_adapter));
        let sink_empty = SearchSink::for_test(1);
        provider_empty.search(
            SearchRequest::new("", SearchMode::Default, "", 1),
            sink_empty.clone(),
        );
        assert_eq!(sink_empty.snapshot().len(), 0);
    }
}
