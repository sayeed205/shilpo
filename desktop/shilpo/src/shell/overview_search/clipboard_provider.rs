use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use shilpo_services::ClipboardItem;
use shilpo_ui::IconName;
use tokio::sync::watch;

use super::{
    parser::SearchMode,
    sink::SearchSink,
    types::{
        ActionResult, CompletionState, LatencyClass, ProviderId, ResultCategory, SearchActivation,
        SearchCandidate, SearchError, SearchProvider, SearchRequest, SearchResultIcon,
    },
};

/// Provider that searches clipboard history subscribed via [`ClipboardService`].
#[derive(Clone)]
pub struct ClipboardSearchProvider {
    subscription: Option<watch::Receiver<Vec<ClipboardItem>>>,
    cached_items: Arc<Mutex<HashMap<String, ClipboardItem>>>,
}

impl ClipboardSearchProvider {
    /// Creates a new clipboard search provider holding the watch subscription receiver.
    pub fn new(subscription: Option<watch::Receiver<Vec<ClipboardItem>>>) -> Self {
        Self {
            subscription,
            cached_items: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl SearchProvider for ClipboardSearchProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from_static("clipboard-search")
    }

    fn declared_modes(&self) -> &'static [SearchMode] {
        &[SearchMode::Clipboard]
    }

    fn prefix_icon(&self, mode: SearchMode) -> Option<IconName> {
        match mode {
            SearchMode::Clipboard => Some(IconName::Star),
            _ => None,
        }
    }

    fn search(&self, request: SearchRequest, sink: SearchSink) {
        let query_generation = request.generation;
        let provider_id = self.id();
        let mut cached = self.cached_items.lock().unwrap();

        // Fresh read from the watch channel subscription on every search() invocation
        let items = self
            .subscription
            .as_ref()
            .map(|rx| rx.borrow().clone())
            .unwrap_or_default();

        for item in items {
            let canonical_id = format!("clipboard:{}", item.id);
            let act_key = format!("clipboard:{query_generation}:{canonical_id}");
            cached.insert(act_key.clone(), item.clone());

            let candidate = SearchCandidate {
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
            };

            sink.push(candidate);
        }
    }

    fn activate(&self, activation: SearchActivation) -> Result<ActionResult, SearchError> {
        let item = self
            .cached_items
            .lock()
            .unwrap()
            .get(&activation.payload)
            .cloned()
            .ok_or_else(|| SearchError::NotFound(activation.payload.clone()))?;

        Ok(ActionResult::CopyClipboard(item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::overview_search::sink::SinkConfig;

    fn sample_item(id: u64, text: &str) -> ClipboardItem {
        ClipboardItem {
            id,
            text: text.to_string(),
            timestamp: (1700000000 + id).to_string(),
        }
    }

    #[test]
    fn test_clipboard_declared_modes_and_prefix_icon() {
        let provider = ClipboardSearchProvider::new(None);
        assert_eq!(provider.declared_modes(), &[SearchMode::Clipboard]);
        assert_eq!(
            provider.prefix_icon(SearchMode::Clipboard),
            Some(IconName::Star)
        );
        assert_eq!(provider.prefix_icon(SearchMode::Default), None);
    }

    #[test]
    fn test_clipboard_candidates_reflect_live_history_changes_between_searches() {
        let (tx, rx) = watch::channel(vec![sample_item(1, "first copied text")]);
        let provider = ClipboardSearchProvider::new(Some(rx));

        // First search sees initial clipboard item
        let sink1 = SearchSink::new(1, SinkConfig::default());
        provider.search(
            SearchRequest::new(";first", SearchMode::Clipboard, "first", 1),
            sink1.clone(),
        );
        let candidates1 = sink1.snapshot();
        assert_eq!(candidates1.len(), 1);
        assert_eq!(candidates1[0].title, "first copied text");

        // Update clipboard channel with new items while provider instance is held
        tx.send(vec![
            sample_item(1, "first copied text"),
            sample_item(2, "second copied text"),
        ])
        .unwrap();

        // Second search on same provider instance sees updated history
        let sink2 = SearchSink::new(2, SinkConfig::default());
        provider.search(
            SearchRequest::new(";second", SearchMode::Clipboard, "second", 2),
            sink2.clone(),
        );
        let candidates2 = sink2.snapshot();
        assert_eq!(candidates2.len(), 2);
        assert!(candidates2.iter().any(|c| c.title == "second copied text"));
    }

    #[test]
    fn test_clipboard_activation() {
        let (_tx, rx) = watch::channel(vec![sample_item(42, "hello clipboard")]);
        let provider = ClipboardSearchProvider::new(Some(rx));

        let sink = SearchSink::new(1, SinkConfig::default());
        provider.search(
            SearchRequest::new(";hello", SearchMode::Clipboard, "hello", 1),
            sink.clone(),
        );
        let candidate = &sink.snapshot()[0];

        let result = provider.activate(candidate.activation.clone()).unwrap();
        match result {
            ActionResult::CopyClipboard(item) => {
                assert_eq!(item.id, 42);
                assert_eq!(item.text, "hello clipboard");
            }
            other => panic!("expected CopyClipboard, got {other:?}"),
        }
    }
}
