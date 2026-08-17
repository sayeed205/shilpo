//! Revisioned service domain port supervision constants.

/// Initial delay for exponential backoff on owner failure (250 ms).
pub const INITIAL_BACKOFF_MS: u64 = 250;

/// Maximum backoff cap for owner reconnect attempts (30 seconds).
pub const MAX_BACKOFF_MS: u64 = 30_000;

/// Rolling window duration for tracking owner failure frequency (60 seconds).
pub const FAILURE_WINDOW_MS: u64 = 60_000;

/// Continuous stable running duration required to clear the failure window (5 minutes).
pub const STABLE_RESET_MS: u64 = 300_000;

/// Number of failures within the rolling failure window that trips the supervisor into quarantine.
pub const QUARANTINE_FAILURES: usize = 5;
