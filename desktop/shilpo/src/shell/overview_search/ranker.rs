use super::{
    learning::SearchLearningStore,
    matcher::fuzzy_match,
    types::{ResultCategory, SearchCandidate},
};

/// Configuration weights and priors for the host search ranker.
#[derive(Debug, Clone)]
pub struct RankerConfig {
    /// Maximum number of final results returned to the view.
    pub top_k: usize,
    /// Multiplier weight for matches in the candidate title.
    pub title_weight: f64,
    /// Multiplier weight for matches in candidate aliases.
    pub alias_weight: f64,
    /// Multiplier weight for matches in candidate keywords / categories.
    pub keyword_weight: f64,
    /// Multiplier weight for matches in candidate subtitles / descriptions.
    pub subtitle_weight: f64,
    /// Prior score for open window candidates.
    pub window_prior: i64,
    /// Prior score for application candidates.
    pub app_prior: i64,
    /// Prior score for action candidates.
    pub action_prior: i64,
    /// Prior score for keybinding candidates.
    pub keybinding_prior: i64,
    /// Prior score for clipboard candidates.
    pub clipboard_prior: i64,
    /// Prior score for file path & URI candidates.
    pub path_uri_prior: i64,
    /// Prior score for calculator candidates.
    pub calc_prior: i64,
    /// Prior score for terminal command fallback candidates.
    pub command_prior: i64,
    /// Prior score for web search fallback candidates.
    pub web_prior: i64,
    /// Prior score for custom/extension candidates.
    pub custom_prior: i64,
}

impl Default for RankerConfig {
    fn default() -> Self {
        Self {
            top_k: 8,
            title_weight: 1.0,
            alias_weight: 0.85,
            keyword_weight: 0.6,
            subtitle_weight: 0.4,
            window_prior: 250,
            app_prior: 200,
            action_prior: 150,
            keybinding_prior: 120,
            clipboard_prior: 100,
            path_uri_prior: 100,
            calc_prior: 500,
            command_prior: -100,
            web_prior: -200,
            custom_prior: 100,
        }
    }
}

/// Ranks and filters a merged set of search candidates from all providers,
/// populating match positions on surviving candidates and truncating to `top_k` exactly once.
pub fn rank(
    candidates: Vec<SearchCandidate>,
    query: &str,
    learning: &dyn SearchLearningStore,
    config: &RankerConfig,
) -> Vec<SearchCandidate> {
    let q = query.trim();

    let mut scored: Vec<(SearchCandidate, i64)> = candidates
        .into_iter()
        .filter_map(|mut cand| {
            let (score, match_positions) = score_candidate(&cand, q, learning, config)?;
            cand.match_positions = match_positions;
            Some((cand, score))
        })
        .collect();

    // Deterministic total ordering: sort descending by score, breaking ties by canonical_id ascending.
    scored.sort_by(|(a, score_a), (b, score_b)| {
        score_b
            .cmp(score_a)
            .then_with(|| a.canonical_id.cmp(&b.canonical_id))
    });

    // Truncate to top-k exactly once over the merged, ranked candidate set.
    scored.truncate(config.top_k);

    scored.into_iter().map(|(cand, _)| cand).collect()
}

/// Scores a single candidate against a query and learning store.
/// Returns `Some((score, match_positions))` if the candidate matches the query, or `None` if it should be dropped.
fn score_candidate(
    candidate: &SearchCandidate,
    query: &str,
    learning: &dyn SearchLearningStore,
    config: &RankerConfig,
) -> Option<(i64, Vec<usize>)> {
    if query.is_empty() {
        // Universal fallbacks are suppressed on empty queries in default mode
        if matches!(
            candidate.category,
            ResultCategory::Command | ResultCategory::WebSearch
        ) {
            return None;
        }

        let mut score = match candidate.category {
            ResultCategory::Window => config.window_prior,
            ResultCategory::Application => config.app_prior,
            ResultCategory::Action => config.action_prior,
            ResultCategory::Keybinding => config.keybinding_prior,
            ResultCategory::Clipboard => config.clipboard_prior,
            ResultCategory::FilePath | ResultCategory::Uri => config.path_uri_prior,
            ResultCategory::Calculator => config.calc_prior,
            ResultCategory::Command => config.command_prior,
            ResultCategory::WebSearch => config.web_prior,
            ResultCategory::Custom => config.custom_prior,
        };

        let learning_boost = learning.score_boost(&candidate.canonical_id);
        score += learning_boost;

        return Some((score, Vec::new()));
    }

    // Universal fallbacks with dedicated triggers / explicit synthesis
    match candidate.category {
        ResultCategory::Calculator => {
            let score = 2000 + config.calc_prior + learning.score_boost(&candidate.canonical_id);
            return Some((score, Vec::new()));
        }
        ResultCategory::FilePath | ResultCategory::Uri => {
            let score =
                1500 + config.path_uri_prior + learning.score_boost(&candidate.canonical_id);
            return Some((score, Vec::new()));
        }
        ResultCategory::Command => {
            let score = config.command_prior + learning.score_boost(&candidate.canonical_id);
            return Some((score, Vec::new()));
        }
        ResultCategory::WebSearch => {
            let score = config.web_prior + learning.score_boost(&candidate.canonical_id);
            return Some((score, Vec::new()));
        }
        _ => {}
    }

    let mut best_field_score: f64 = 0.0;
    let mut title_match = None;

    // 1. Title match
    if let Some(res) = fuzzy_match(query, &candidate.title) {
        let weighted = res.score as f64 * config.title_weight;
        if weighted > best_field_score {
            best_field_score = weighted;
        }
        title_match = Some(res);
    }

    // 2. Alias matches
    for alias in &candidate.aliases {
        if let Some(res) = fuzzy_match(query, alias) {
            let weighted = res.score as f64 * config.alias_weight;
            if weighted > best_field_score {
                best_field_score = weighted;
            }
        }
    }

    // 3. Keyword matches
    for keyword in &candidate.keywords {
        if let Some(res) = fuzzy_match(query, keyword) {
            let weighted = res.score as f64 * config.keyword_weight;
            if weighted > best_field_score {
                best_field_score = weighted;
            }
        }
    }

    // 4. Subtitle match
    if let Some(subtitle) = &candidate.subtitle
        && let Some(res) = fuzzy_match(query, subtitle)
    {
        let weighted = res.score as f64 * config.subtitle_weight;
        if weighted > best_field_score {
            best_field_score = weighted;
        }
    }

    // If no field matched, drop this candidate from the results
    if best_field_score <= 0.0 {
        return None;
    }

    let text_score = best_field_score.round() as i64;
    let prior = match candidate.category {
        ResultCategory::Window => config.window_prior,
        ResultCategory::Application => config.app_prior,
        ResultCategory::Action => config.action_prior,
        ResultCategory::Keybinding => config.keybinding_prior,
        ResultCategory::Clipboard => config.clipboard_prior,
        ResultCategory::FilePath | ResultCategory::Uri => config.path_uri_prior,
        ResultCategory::Calculator => config.calc_prior,
        ResultCategory::Command => config.command_prior,
        ResultCategory::WebSearch => config.web_prior,
        ResultCategory::Custom => config.custom_prior,
    };

    let learning_boost = learning.score_boost(&candidate.canonical_id);
    let total_score = text_score + prior + learning_boost;
    let match_positions = title_match.map(|m| m.positions).unwrap_or_default();

    Some((total_score, match_positions))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::shell::overview_search::{
        learning::{
            DEFAULT_HALF_LIFE_SECS, HeedSearchLearningStore, NoopSearchLearningStore,
            TestLearningClock,
        },
        types::{CompletionState, LatencyClass, ProviderId, SearchActivation, SearchResultIcon},
    };

    fn make_test_candidate(
        provider: &'static str,
        canonical_id: &'static str,
        title: &'static str,
        category: ResultCategory,
    ) -> SearchCandidate {
        SearchCandidate {
            provider_id: ProviderId::from_static(provider),
            canonical_id: canonical_id.to_string(),
            generation: 1,
            title: title.to_string(),
            subtitle: None,
            aliases: Vec::new(),
            keywords: Vec::new(),
            category,
            latency: LatencyClass::Instant,
            completion: CompletionState::Complete,
            icon: SearchResultIcon::Initial('T'),
            activation_verb: "Open".to_string(),
            match_positions: Vec::new(),
            activation: SearchActivation::new(canonical_id),
        }
    }

    #[test]
    fn test_exact_title_match_outranks_subsequence_from_higher_prior_provider() {
        // App has higher prior (200), but only subsequence match for "fx"
        let app = make_test_candidate(
            "apps",
            "app:firefox",
            "Firefox Browser",
            ResultCategory::Application,
        );
        // Action has lower prior (150), but exact title match for "fx"
        let action = make_test_candidate("actions", "action:fx", "fx", ResultCategory::Action);

        let candidates = vec![app, action];
        let ranked = rank(
            candidates,
            "fx",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );

        assert_eq!(ranked.len(), 2);
        assert_eq!(
            ranked[0].canonical_id, "action:fx",
            "Exact match from lower prior provider must outrank subsequence match from higher prior provider"
        );
        assert_eq!(ranked[1].canonical_id, "app:firefox");
    }

    #[test]
    fn test_open_window_outranks_launching_duplicate_app_instance() {
        let app = make_test_candidate(
            "apps",
            "app:firefox.desktop",
            "Firefox",
            ResultCategory::Application,
        );
        let window = make_test_candidate("windows", "window:42", "Firefox", ResultCategory::Window);

        let candidates = vec![app, window];
        let ranked = rank(
            candidates,
            "Firefox",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );

        assert_eq!(ranked.len(), 2);
        assert_eq!(
            ranked[0].canonical_id, "window:42",
            "Open window (prior 250) must outrank launching a new app instance (prior 200)"
        );
        assert_eq!(ranked[1].canonical_id, "app:firefox.desktop");
    }

    #[test]
    fn test_repeated_identical_queries_produce_byte_identical_ordering() {
        let candidates = vec![
            make_test_candidate(
                "apps",
                "app:calc",
                "Calculator",
                ResultCategory::Application,
            ),
            make_test_candidate("apps", "app:term", "Terminal", ResultCategory::Application),
            make_test_candidate(
                "actions",
                "action:calc",
                "Recalculate",
                ResultCategory::Action,
            ),
            make_test_candidate("apps", "app:files", "Files", ResultCategory::Application),
        ];

        let config = RankerConfig::default();
        let first = rank(
            candidates.clone(),
            "calc",
            &NoopSearchLearningStore,
            &config,
        );
        let first_ids: Vec<String> = first.iter().map(|c| c.canonical_id.clone()).collect();

        for _ in 0..50 {
            let next = rank(
                candidates.clone(),
                "calc",
                &NoopSearchLearningStore,
                &config,
            );
            let next_ids: Vec<String> = next.iter().map(|c| c.canonical_id.clone()).collect();
            assert_eq!(
                first_ids, next_ids,
                "Ordering must be byte-identical on repeated invocations"
            );
        }
    }

    #[test]
    fn test_equal_score_candidates_order_by_canonical_identity() {
        // Same category, same title -> same score
        let c1 = make_test_candidate(
            "p1",
            "b-canonical",
            "Duplicate Title",
            ResultCategory::Application,
        );
        let c2 = make_test_candidate(
            "p1",
            "a-canonical",
            "Duplicate Title",
            ResultCategory::Application,
        );

        // Arrival order: c1 then c2
        let ranked = rank(
            vec![c1.clone(), c2.clone()],
            "Duplicate",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );
        assert_eq!(ranked[0].canonical_id, "a-canonical");
        assert_eq!(ranked[1].canonical_id, "b-canonical");

        // Arrival order: c2 then c1
        let ranked2 = rank(
            vec![c2, c1],
            "Duplicate",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );
        assert_eq!(ranked2[0].canonical_id, "a-canonical");
        assert_eq!(ranked2[1].canonical_id, "b-canonical");
    }

    #[test]
    fn test_late_candidate_inserts_at_rank_without_reordering_unrelated_rows() {
        let a = make_test_candidate("apps", "app:a", "Browser", ResultCategory::Application);
        let c = make_test_candidate("apps", "app:c", "Web Browser", ResultCategory::Application);
        let d = make_test_candidate(
            "apps",
            "app:d",
            "The Internet Browser",
            ResultCategory::Application,
        );

        let initial = rank(
            vec![a.clone(), c.clone(), d.clone()],
            "Browser",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );
        let initial_ids: Vec<&str> = initial
            .iter()
            .map(|item| item.canonical_id.as_str())
            .collect();
        assert_eq!(initial_ids, vec!["app:a", "app:c", "app:d"]);

        // Late candidate arrives with score between a and c
        let b = make_test_candidate("apps", "app:b", "A Browser", ResultCategory::Application);
        let updated = rank(
            vec![a, c, d, b],
            "Browser",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );
        let updated_ids: Vec<&str> = updated
            .iter()
            .map(|item| item.canonical_id.as_str())
            .collect();

        assert_eq!(updated_ids, vec!["app:a", "app:b", "app:c", "app:d"]);
    }

    #[test]
    fn test_two_registered_providers_both_reach_final_result_set() {
        // Provider 1 produces 20 lower-matching items
        let mut candidates = Vec::new();
        for i in 0..20 {
            candidates.push(make_test_candidate(
                "provider-1",
                Box::leak(format!("p1:{i}").into_boxed_str()),
                Box::leak(format!("Scattered match term {i}").into_boxed_str()),
                ResultCategory::Application,
            ));
        }

        // Provider 2 produces 2 exact/strong items
        candidates.push(make_test_candidate(
            "provider-2",
            "p2:exact",
            "Term Exact",
            ResultCategory::Action,
        ));
        candidates.push(make_test_candidate(
            "provider-2",
            "p2:prefix",
            "Terminal App",
            ResultCategory::Action,
        ));

        let ranked = rank(
            candidates,
            "Term",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );

        assert!(ranked.len() <= 8);
        assert_eq!(ranked[0].canonical_id, "p2:exact");
        assert_eq!(ranked[1].canonical_id, "p2:prefix");
        // Provider 1 items fill remaining top-k slots
        assert!(
            ranked
                .iter()
                .any(|c| c.provider_id.as_str() == "provider-1")
        );
    }

    #[test]
    fn test_match_positions_populated_for_matched_candidate() {
        let cand = make_test_candidate(
            "apps",
            "app:calc",
            "Gnome Calculator",
            ResultCategory::Application,
        );
        let ranked = rank(
            vec![cand],
            "calc",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].match_positions, vec![6, 7, 8, 9]);
    }

    #[test]
    fn test_golden_curated_corpus_ordering() {
        let app_calc = make_test_candidate(
            "apps",
            "app:gnome-calculator",
            "Calculator",
            ResultCategory::Application,
        );
        let mut app_calc_full = app_calc.clone();
        app_calc_full.aliases = vec!["gnome-calculator".to_string()];
        app_calc_full.keywords = vec!["Utility".to_string(), "Math".to_string()];

        let app_ff = make_test_candidate(
            "apps",
            "app:firefox",
            "Firefox",
            ResultCategory::Application,
        );
        let mut app_ff_full = app_ff.clone();
        app_ff_full.aliases = vec!["firefox".to_string(), "web".to_string()];

        let app_term = make_test_candidate(
            "apps",
            "app:terminal",
            "Terminal",
            ResultCategory::Application,
        );
        let win_term =
            make_test_candidate("windows", "window:123", "Terminal", ResultCategory::Window);
        let act_toggle = make_test_candidate(
            "actions",
            "action:toggle-overview",
            "Toggle Overview",
            ResultCategory::Action,
        );
        let cmd_fallback =
            make_test_candidate("system", "cmd:term", "term", ResultCategory::Command);
        let web_fallback = make_test_candidate(
            "system",
            "web:https://google.com/search?q=term",
            "term",
            ResultCategory::WebSearch,
        );

        let corpus = vec![
            app_calc_full,
            app_ff_full,
            app_term,
            win_term,
            act_toggle,
            cmd_fallback,
            web_fallback,
        ];

        let config = RankerConfig::default();
        let clock = Arc::new(TestLearningClock::new(1000));
        let learning =
            HeedSearchLearningStore::with_config(None, clock, 512, DEFAULT_HALF_LIFE_SECS);
        for _ in 0..4 {
            learning.record_activation("app:firefox");
        }

        // 1. Query: "" (empty query with activated app firefox)
        let res_empty = rank(corpus.clone(), "", &learning, &config);
        let ids_empty: Vec<&str> = res_empty.iter().map(|c| c.canonical_id.as_str()).collect();
        assert_eq!(
            ids_empty,
            vec![
                "window:123",
                "app:firefox",
                "app:gnome-calculator",
                "app:terminal",
                "action:toggle-overview",
            ]
        );

        // 2. Query: "calc" (App Calculator > Command > WebSearch)
        let res_calc = rank(corpus.clone(), "calc", &NoopSearchLearningStore, &config);
        let ids_calc: Vec<&str> = res_calc.iter().map(|c| c.canonical_id.as_str()).collect();
        assert_eq!(
            ids_calc,
            vec![
                "app:gnome-calculator",
                "cmd:term",
                "web:https://google.com/search?q=term"
            ]
        );

        // 3. Query: "term" (Window Terminal > App Terminal > Command > WebSearch)
        let res_term = rank(corpus.clone(), "term", &NoopSearchLearningStore, &config);
        let ids_term: Vec<&str> = res_term.iter().map(|c| c.canonical_id.as_str()).collect();
        assert_eq!(
            ids_term,
            vec![
                "window:123",
                "app:terminal",
                "cmd:term",
                "web:https://google.com/search?q=term"
            ]
        );

        // 4. Query: "toggle" (Action Toggle > Command > WebSearch)
        let res_toggle = rank(corpus, "toggle", &NoopSearchLearningStore, &config);
        let ids_toggle: Vec<&str> = res_toggle.iter().map(|c| c.canonical_id.as_str()).collect();
        assert_eq!(
            ids_toggle,
            vec![
                "action:toggle-overview",
                "cmd:term",
                "web:https://google.com/search?q=term"
            ]
        );
    }

    #[test]
    fn test_multibyte_utf8_match_positions() {
        let cand = make_test_candidate(
            "apps",
            "app:rover",
            "🦀 Rust Rover",
            ResultCategory::Application,
        );
        let ranked = rank(
            vec![cand],
            "rover",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );

        assert_eq!(ranked.len(), 1);
        // '🦀' is 1 char (4 bytes), ' ' is 1 char, 'R', 'u', 's', 't', ' ', 'R', 'o', 'v', 'e', 'r'
        // 'R' is char index 7, 'o' is 8, 'v' is 9, 'e' is 10, 'r' is 11
        assert_eq!(ranked[0].match_positions, vec![7, 8, 9, 10, 11]);
    }

    #[test]
    fn test_alias_and_keyword_matching() {
        let mut cand = make_test_candidate(
            "apps",
            "app:code",
            "Visual Studio Code",
            ResultCategory::Application,
        );
        cand.aliases = vec!["code".to_string(), "vsc".to_string()];
        cand.keywords = vec!["development".to_string(), "editor".to_string()];

        let res_code = rank(
            vec![cand.clone()],
            "code",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );
        assert_eq!(res_code.len(), 1);
        assert_eq!(res_code[0].canonical_id, "app:code");

        let res_kw = rank(
            vec![cand],
            "editor",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );
        assert_eq!(res_kw.len(), 1);
        assert_eq!(res_kw[0].canonical_id, "app:code");
    }

    #[test]
    fn test_repeated_activation_saturates_at_plus_40_and_raises_rank() {
        let clock = Arc::new(TestLearningClock::new(1000));
        let learning =
            HeedSearchLearningStore::with_config(None, clock, 512, DEFAULT_HALF_LIFE_SECS);
        let config = RankerConfig::default();

        let cand_a = make_test_candidate("apps", "app:a", "App", ResultCategory::Application);
        let cand_b = make_test_candidate("apps", "app:b", "App", ResultCategory::Application);

        // Initially, alphabetical / canonical tie-breaker applies (app:a ranks before app:b)
        let initial = rank(
            vec![cand_a.clone(), cand_b.clone()],
            "App",
            &learning,
            &config,
        );
        assert_eq!(initial[0].canonical_id, "app:a");
        assert_eq!(initial[1].canonical_id, "app:b");

        // Activate App Beta 2 times (+20 boost) -> App Beta rises above App Alpha
        learning.record_activation("app:b");
        learning.record_activation("app:b");
        let trained_2 = rank(
            vec![cand_a.clone(), cand_b.clone()],
            "App",
            &learning,
            &config,
        );
        assert_eq!(trained_2[0].canonical_id, "app:b");
        assert_eq!(trained_2[1].canonical_id, "app:a");

        // Activate App Beta 10 more times -> boost saturates at +40 exactly
        for _ in 0..10 {
            learning.record_activation("app:b");
        }
        assert_eq!(learning.score_boost("app:b"), 40);
    }

    #[test]
    fn test_higher_category_tier_still_outranks_saturated_lower_tier_candidate() {
        let clock = Arc::new(TestLearningClock::new(1000));
        let learning =
            HeedSearchLearningStore::with_config(None, clock, 512, DEFAULT_HALF_LIFE_SECS);
        let config = RankerConfig::default();

        let app_cand =
            make_test_candidate("apps", "app:search", "Search", ResultCategory::Application);
        let action_cand =
            make_test_candidate("actions", "action:search", "Search", ResultCategory::Action);

        // Fully train action_cand (+40 saturation boost)
        for _ in 0..10 {
            learning.record_activation("action:search");
        }
        assert_eq!(learning.score_boost("action:search"), 40);

        // App prior (200) vs Action prior (150) + learning boost (40) = 190.
        // App (200) must still strictly outrank Action (190) on same match quality.
        let ranked = rank(vec![app_cand, action_cand], "Search", &learning, &config);
        assert_eq!(ranked[0].canonical_id, "app:search");
        assert_eq!(ranked[1].canonical_id, "action:search");
    }

    #[test]
    fn test_learning_applies_to_non_application_window_candidates() {
        let clock = Arc::new(TestLearningClock::new(1000));
        let learning =
            HeedSearchLearningStore::with_config(None, clock, 512, DEFAULT_HALF_LIFE_SECS);
        let config = RankerConfig::default();

        let win_1 = make_test_candidate("windows", "window:1", "Terminal", ResultCategory::Window);
        let win_2 = make_test_candidate("windows", "window:2", "Terminal", ResultCategory::Window);

        // Initial order: window:1 before window:2 by canonical_id
        let initial = rank(
            vec![win_1.clone(), win_2.clone()],
            "Terminal",
            &learning,
            &config,
        );
        assert_eq!(initial[0].canonical_id, "window:1");
        assert_eq!(initial[1].canonical_id, "window:2");

        // Train window:2
        learning.record_activation("window:2");
        let updated = rank(vec![win_1, win_2], "Terminal", &learning, &config);
        assert_eq!(updated[0].canonical_id, "window:2");
        assert_eq!(updated[1].canonical_id, "window:1");
    }

    #[test]
    fn test_time_decay_lowers_older_activation_boost_relative_to_recent() {
        let clock = Arc::new(TestLearningClock::new(1_000_000));
        let learning =
            HeedSearchLearningStore::with_config(None, clock.clone(), 512, DEFAULT_HALF_LIFE_SECS);
        let config = RankerConfig::default();

        let cand_old =
            make_test_candidate("apps", "app:old", "Editor", ResultCategory::Application);
        let cand_new =
            make_test_candidate("apps", "app:new", "Editor", ResultCategory::Application);

        // At t0: activate cand_old once (boost = 10)
        learning.record_activation("app:old");
        assert_eq!(learning.score_boost("app:old"), 10);

        // Advance time by 14 days (half-life): cand_old decays to 5 boost
        clock.advance_secs(DEFAULT_HALF_LIFE_SECS);
        assert_eq!(learning.score_boost("app:old"), 5);

        // Now activate cand_new once at the new timestamp (boost = 10)
        learning.record_activation("app:new");
        assert_eq!(learning.score_boost("app:new"), 10);

        // cand_new with 1 recent activation (boost 10) outranks cand_old with 1 decayed activation (boost 5)
        let ranked = rank(vec![cand_old, cand_new], "Editor", &learning, &config);
        assert_eq!(ranked[0].canonical_id, "app:new");
        assert_eq!(ranked[1].canonical_id, "app:old");
    }

    #[test]
    fn test_starvation_regression_symmetric_provider_registration() {
        use std::sync::Arc;

        use crate::shell::overview_search::{
            coordinator::SearchCoordinator,
            sink::SearchSink,
            types::{ActionResult, SearchError, SearchProvider, SearchRequest},
        };

        struct ProlificProvider;
        impl SearchProvider for ProlificProvider {
            fn id(&self) -> ProviderId {
                ProviderId::from_static("prolific")
            }
            fn search(&self, _request: SearchRequest, sink: SearchSink) {
                for i in 0..50 {
                    sink.push(make_test_candidate(
                        "prolific",
                        Box::leak(format!("prolific:{i}").into_boxed_str()),
                        Box::leak(format!("Prolific Item {i}").into_boxed_str()),
                        ResultCategory::Application,
                    ));
                }
            }
            fn activate(&self, _activation: SearchActivation) -> Result<ActionResult, SearchError> {
                Ok(ActionResult::Handled {
                    close_overview: true,
                })
            }
        }

        struct TargetedProvider;
        impl SearchProvider for TargetedProvider {
            fn id(&self) -> ProviderId {
                ProviderId::from_static("targeted")
            }
            fn search(&self, _request: SearchRequest, sink: SearchSink) {
                sink.push(make_test_candidate(
                    "targeted",
                    "targeted:exact",
                    "Targeted Best Match",
                    ResultCategory::Action,
                ));
            }
            fn activate(&self, _activation: SearchActivation) -> Result<ActionResult, SearchError> {
                Ok(ActionResult::Handled {
                    close_overview: true,
                })
            }
        }

        // Order 1: Prolific registered first, Targeted registered second
        let coord1 =
            SearchCoordinator::new(vec![Arc::new(ProlificProvider), Arc::new(TargetedProvider)]);
        let sink1 = SearchSink::for_test(1);
        coord1.search("Targeted", 1, &sink1);
        let res1 = sink1.snapshot();
        assert_eq!(
            res1[0].canonical_id, "targeted:exact",
            "Targeted item must not be starved by prolific provider"
        );

        // Order 2: Targeted registered first, Prolific registered second
        let coord2 =
            SearchCoordinator::new(vec![Arc::new(TargetedProvider), Arc::new(ProlificProvider)]);
        let sink2 = SearchSink::for_test(1);
        coord2.search("Targeted", 1, &sink2);
        let res2 = sink2.snapshot();
        assert_eq!(
            res2[0].canonical_id, "targeted:exact",
            "Targeted item must rank at top regardless of registration order"
        );
    }

    #[test]
    fn test_filepath_and_uri_candidates_keep_their_guaranteed_baseline_score() {
        // Mirrors legacy_adapter.rs's construction: title is the filename/URI only,
        // subtitle carries the full path. The query is the raw path/URI the user typed,
        // which is not a subsequence of the bare filename title.
        let mut path_cand = make_test_candidate(
            "legacy",
            "path:/home/user/Documents/report.txt",
            "report.txt",
            ResultCategory::FilePath,
        );
        path_cand.subtitle = Some("/home/user/Documents/report.txt".to_string());

        let mut uri_cand = make_test_candidate(
            "legacy",
            "uri:https://example.com/x",
            "https://example.com/x",
            ResultCategory::Uri,
        );
        uri_cand.subtitle = Some("Web or protocol URI".to_string());

        // A generic application match that would otherwise dominate on text score alone.
        let unrelated_app = make_test_candidate(
            "apps",
            "app:unrelated",
            "Some Unrelated App",
            ResultCategory::Application,
        );

        let ranked = rank(
            vec![unrelated_app, path_cand],
            "/home/user/Documents/report.txt",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );
        assert_eq!(
            ranked.len(),
            1,
            "unrelated app must not match a literal path query"
        );
        assert_eq!(
            ranked[0].canonical_id,
            "path:/home/user/Documents/report.txt"
        );

        let (path_score, _) = score_candidate(
            &ranked[0].clone(),
            "/home/user/Documents/report.txt",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        )
        .unwrap();
        assert_eq!(
            path_score, 1600,
            "FilePath candidates must keep their fixed 1500 + path_uri_prior baseline"
        );

        let (uri_score, _) = score_candidate(
            &uri_cand,
            "https://example.com/x",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        )
        .unwrap();
        assert_eq!(
            uri_score, 1600,
            "Uri candidates must keep their fixed 1500 + path_uri_prior baseline"
        );
    }

    #[test]
    fn test_calculator_candidates_keep_their_guaranteed_baseline_score() {
        let calc_cand = make_test_candidate(
            "legacy",
            "calc:2+2",
            "2 + 2 = 4",
            ResultCategory::Calculator,
        );

        let (score, _) = score_candidate(
            &calc_cand,
            "2+2",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        )
        .unwrap();
        assert_eq!(
            score, 2500,
            "Calculator candidates must keep their fixed 2000 + calc_prior baseline"
        );

        // Calculator must still dominate an application whose title happens to match.
        let app_cand =
            make_test_candidate("apps", "app:calc-app", "2+2", ResultCategory::Application);
        let ranked = rank(
            vec![app_cand, calc_cand],
            "2+2",
            &NoopSearchLearningStore,
            &RankerConfig::default(),
        );
        assert_eq!(ranked[0].canonical_id, "calc:2+2");
    }
}
