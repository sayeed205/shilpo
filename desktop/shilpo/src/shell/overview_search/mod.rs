pub mod app_provider;
pub mod calculator;
pub mod coordinator;
pub mod legacy_adapter;
pub mod matcher;
pub mod parser;
pub mod ranker;
pub mod ranking;
pub mod sink;
pub mod types;
pub mod window_provider;

pub use app_provider::AppSearchProvider;
pub use coordinator::SearchCoordinator;
pub use legacy_adapter::{LegacyOverviewSearchProvider, OverviewSearch};
pub use matcher::{MatchResult, fuzzy_match, fuzzy_score};
pub use parser::SearchMode;
pub use ranker::{RankerConfig, rank};
pub use sink::{SearchSink, SinkConfig};
pub use types::{
    ActionResult, CompletionState, LatencyClass, ProviderId, ResultCategory, SearchActivation,
    SearchCandidate, SearchError, SearchProvider, SearchRequest, SearchResultIcon,
};
pub use window_provider::WindowSearchProvider;
