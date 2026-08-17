pub mod action_provider;
pub mod app_provider;
pub mod calculator;
pub mod calculator_provider;
pub mod clipboard_provider;
pub mod coordinator;
pub mod learning;
pub mod matcher;
pub mod parser;
pub mod quicklinks_provider;
pub mod ranker;
pub mod ranking;
pub mod sink;
pub mod types;
pub mod window_provider;

pub use action_provider::ActionSearchProvider;
pub use app_provider::AppSearchProvider;
pub use calculator_provider::CalculatorSearchProvider;
pub use clipboard_provider::ClipboardSearchProvider;
pub use coordinator::SearchCoordinator;
pub use learning::{
    DEFAULT_HALF_LIFE_SECS, HeedSearchLearningStore, LearningClock, MAX_INFLUENCE_BOOST,
    MAX_LEARNING_ENTRIES, NoopSearchLearningStore, SearchLearningStore, SystemLearningClock,
    TestLearningClock,
};
pub use matcher::{MatchResult, fuzzy_match, fuzzy_score};
pub use parser::SearchMode;
pub use quicklinks_provider::QuicklinksSearchProvider;
pub use ranker::{RankerConfig, rank};
pub use sink::{SearchSink, SinkConfig};
pub use types::{
    ActionResult, CompletionState, LatencyClass, ProviderId, ResultCategory, SearchActivation,
    SearchCandidate, SearchError, SearchProvider, SearchRequest, SearchResultIcon,
};
pub use window_provider::WindowSearchProvider;
