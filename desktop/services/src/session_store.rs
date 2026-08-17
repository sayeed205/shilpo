//! Persistent session store backed by LMDB via [`heed`].
//!
//! This module owns the on-disk lifecycle of per-session data—clipboard
//! history, output-bar state, and audio preferences. Key policies:
//!
//! - **Bounded retention**: clipboard history is capped at
//!   [`DEFAULT_CLIPBOARD_HISTORY_LIMIT`] entries (currently 100). Insert and
//!   pruning occur within a single LMDB write transaction so disk retention
//!   matches the in-memory limit at all times.
//!
//! - **Private storage** (Unix): the store directory is created with mode
//!   `0700` and individual data files with mode `0600`, preventing other users
//!   from reading clipboard or preference data.
//!
//! - **Classified recovery**: on open failure, the store narrows the error to a
//!   small set of confirmed-corruption MDB variants. Only those trigger
//!   quarantine—the corrupt directory is renamed to a timestamped sibling and a
//!   fresh store is created. All other errors propagate without modifying or
//!   deleting data.
//!
//! - **Memory-only fallback**: when no backing store is provided (e.g. in tests
//!   or when the open path fails), callers receive `None` and persist
//!   exclusively in memory. The clipboard service treats `None` as offline
//!   rather than as an error.
use heed::types::{SerdeJson, Str};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub const DEFAULT_CLIPBOARD_HISTORY_LIMIT: usize = 100;

static QUARANTINE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub enum SessionStoreError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Backend {
        operation: &'static str,
        source: heed::Error,
    },
    Corrupt {
        path: PathBuf,
        source: heed::Error,
    },
    InvalidLimit,
}

impl std::fmt::Display for SessionStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "I/O error at {}: {source}", path.display()),
            Self::Backend { operation, source } => {
                write!(f, "session store {operation} error: {source}")
            }
            Self::Corrupt { path, source } => {
                write!(f, "session store corrupt at {}: {source}", path.display())
            }
            Self::InvalidLimit => write!(f, "invalid limit: must be non-zero"),
        }
    }
}

impl std::error::Error for SessionStoreError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryOutcome {
    Opened,
    Quarantined { path: PathBuf },
}

pub struct OpenedSessionStore {
    pub store: HeedSessionStore,
    pub recovery: RecoveryOutcome,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OutputBarState {
    pub visible: bool,
    pub position_edge: String,
    pub thickness: u32,
    pub exclusive_zone: Option<u32>,
    pub active_workspace_id: Option<u64>,
}

pub const CLIPBOARD_ITEM_VERSION: u8 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ClipboardContent {
    Text(String),
    FileReference(Vec<PathBuf>),
    Image,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardItem {
    pub version: u8,
    pub id: String,
    pub content: ClipboardContent,
    pub last_copied_at: chrono::DateTime<chrono::Utc>,
}

impl ClipboardItem {
    pub const CURRENT_VERSION: u8 = CLIPBOARD_ITEM_VERSION;

    pub fn compute_content_hash(content: &ClipboardContent) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        match content {
            ClipboardContent::Text(text) => {
                hasher.update(b"text:");
                hasher.update(text.as_bytes());
            }
            ClipboardContent::FileReference(paths) => {
                hasher.update(b"file_ref:");
                for path in paths {
                    hasher.update(path.to_string_lossy().as_bytes());
                    hasher.update(b"\0");
                }
            }
            ClipboardContent::Image => {
                hasher.update(b"image:");
            }
        }
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }

    pub fn new_text(text: String, last_copied_at: chrono::DateTime<chrono::Utc>) -> Self {
        let content = ClipboardContent::Text(text);
        let id = Self::compute_content_hash(&content);
        Self {
            version: Self::CURRENT_VERSION,
            id,
            content,
            last_copied_at,
        }
    }

    pub fn new_file_reference(
        paths: Vec<PathBuf>,
        last_copied_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let content = ClipboardContent::FileReference(paths);
        let id = Self::compute_content_hash(&content);
        Self {
            version: Self::CURRENT_VERSION,
            id,
            content,
            last_copied_at,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match &self.content {
            ClipboardContent::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn display_text(&self) -> String {
        match &self.content {
            ClipboardContent::Text(text) => text.clone(),
            ClipboardContent::FileReference(paths) => paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            ClipboardContent::Image => "[Image]".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AudioPreference {
    pub default_device: Option<String>,
    pub default_port: Option<String>,
}

pub const SEARCH_LEARNING_RECORD_VERSION: u8 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SearchLearningRecord {
    pub version: u8,
    pub activation_count: u32,
    pub last_activated_at_secs: u64,
    pub decayed_score: f64,
}

impl SearchLearningRecord {
    pub const CURRENT_VERSION: u8 = SEARCH_LEARNING_RECORD_VERSION;

    pub fn new_initial(activated_at_secs: u64) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            activation_count: 1,
            last_activated_at_secs: activated_at_secs,
            decayed_score: 1.0,
        }
    }
}

pub struct HeedSessionStore {
    env: heed::Env,
    output_bars_db: heed::Database<Str, SerdeJson<OutputBarState>>,
    clipboard_history_db: heed::Database<Str, SerdeJson<ClipboardItem>>,
    audio_pref_db: heed::Database<Str, SerdeJson<AudioPreference>>,
    search_learning_db: heed::Database<Str, SerdeJson<SearchLearningRecord>>,
    _lock_file: Option<fs::File>,
}

impl HeedSessionStore {
    pub fn default_db_dir() -> PathBuf {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from(".local/share"))
            .join("shilpo");
        base.join("session.lmdb")
    }

    pub fn is_corrupt_error(err: &heed::Error) -> bool {
        matches!(
            err,
            heed::Error::Mdb(
                heed::MdbError::Corrupted
                    | heed::MdbError::Panic
                    | heed::MdbError::Invalid
                    | heed::MdbError::PageNotFound,
            )
        )
    }

    pub fn open(dir: &Path) -> Result<Self, SessionStoreError> {
        Self::open_internal(dir)
    }

    fn open_internal(dir: &Path) -> Result<Self, SessionStoreError> {
        if let Err(e) = fs::create_dir_all(dir) {
            return Err(SessionStoreError::Io {
                path: dir.to_path_buf(),
                source: e,
            });
        }

        #[cfg(unix)]
        {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|e| {
                SessionStoreError::Io {
                    path: dir.to_path_buf(),
                    source: e,
                }
            })?;
        }

        let lock_path = dir.join("session.lock");
        let lock_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| SessionStoreError::Io {
                path: lock_path.clone(),
                source: e,
            })?;

        #[cfg(unix)]
        {
            lock_file
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|e| SessionStoreError::Io {
                    path: lock_path.clone(),
                    source: e,
                })?;
        }

        let env = unsafe {
            heed::EnvOpenOptions::new()
                .max_dbs(10)
                .map_size(10 * 1024 * 1024)
                .open(dir)
                .map_err(|e| {
                    if Self::is_corrupt_error(&e) {
                        SessionStoreError::Corrupt {
                            path: dir.to_path_buf(),
                            source: e,
                        }
                    } else {
                        SessionStoreError::Backend {
                            operation: "open",
                            source: e,
                        }
                    }
                })?
        };

        #[cfg(unix)]
        {
            let data_file = dir.join("data.mdb");
            if data_file.exists() {
                fs::set_permissions(&data_file, fs::Permissions::from_mode(0o600)).map_err(
                    |e| SessionStoreError::Io {
                        path: data_file.clone(),
                        source: e,
                    },
                )?;
            }
            let mdb_lock_file = dir.join("lock.mdb");
            if mdb_lock_file.exists() {
                fs::set_permissions(&mdb_lock_file, fs::Permissions::from_mode(0o600)).map_err(
                    |e| SessionStoreError::Io {
                        path: mdb_lock_file.clone(),
                        source: e,
                    },
                )?;
            }
        }

        let mut wtxn = env.write_txn().map_err(|e| SessionStoreError::Backend {
            operation: "write_txn",
            source: e,
        })?;

        let output_bars_db = env
            .create_database(&mut wtxn, Some("output_bars"))
            .map_err(|e| SessionStoreError::Backend {
                operation: "create_database output_bars",
                source: e,
            })?;

        let clipboard_history_db = env
            .create_database(&mut wtxn, Some("clipboard_history"))
            .map_err(|e| SessionStoreError::Backend {
                operation: "create_database clipboard_history",
                source: e,
            })?;

        let audio_pref_db = env
            .create_database(&mut wtxn, Some("audio_preference"))
            .map_err(|e| SessionStoreError::Backend {
                operation: "create_database audio_preference",
                source: e,
            })?;

        let search_learning_db = env
            .create_database(&mut wtxn, Some("search_learning"))
            .map_err(|e| SessionStoreError::Backend {
                operation: "create_database search_learning",
                source: e,
            })?;

        wtxn.commit().map_err(|e| SessionStoreError::Backend {
            operation: "commit_databases",
            source: e,
        })?;

        Ok(Self {
            env,
            output_bars_db,
            clipboard_history_db,
            audio_pref_db,
            search_learning_db,
            _lock_file: Some(lock_file),
        })
    }

    pub fn open_with_recovery(dir: &Path) -> Result<OpenedSessionStore, SessionStoreError> {
        match Self::open(dir) {
            Ok(store) => Ok(OpenedSessionStore {
                store,
                recovery: RecoveryOutcome::Opened,
            }),
            Err(SessionStoreError::Corrupt { .. }) => {
                let parent = dir.parent().unwrap_or_else(|| Path::new("."));
                let nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let counter = QUARANTINE_COUNTER.fetch_add(1, Ordering::SeqCst);
                let file_name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_else(|| "session.lmdb".into());
                let quarantine_name = format!("{file_name}.corrupt-{nanos}-{counter}");
                let quarantine_path = parent.join(quarantine_name);

                fs::rename(dir, &quarantine_path).map_err(|e| SessionStoreError::Io {
                    path: dir.to_path_buf(),
                    source: e,
                })?;

                let store = match Self::open_internal(dir) {
                    Ok(store) => store,
                    Err(error) => {
                        // A failed fresh open must not strand the user's original
                        // database at its quarantine path. Remove only the exact
                        // newly-created recovery directory before restoring it.
                        if dir.exists() {
                            fs::remove_dir_all(dir).map_err(|restore_error| {
                                SessionStoreError::Io {
                                    path: dir.to_path_buf(),
                                    source: restore_error,
                                }
                            })?;
                        }
                        fs::rename(&quarantine_path, dir).map_err(|restore_error| {
                            SessionStoreError::Io {
                                path: quarantine_path.clone(),
                                source: restore_error,
                            }
                        })?;
                        return Err(error);
                    }
                };
                Ok(OpenedSessionStore {
                    store,
                    recovery: RecoveryOutcome::Quarantined {
                        path: quarantine_path,
                    },
                })
            }
            Err(e) => Err(e),
        }
    }

    pub fn record_clipboard_item(
        &self,
        item: &ClipboardItem,
        max_entries: usize,
    ) -> Result<(), SessionStoreError> {
        if max_entries == 0 {
            return Err(SessionStoreError::InvalidLimit);
        }

        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "write_txn",
                source: e,
            })?;

        self.clipboard_history_db
            .put(&mut wtxn, &item.id, item)
            .map_err(|e| SessionStoreError::Backend {
                operation: "put_clipboard_item",
                source: e,
            })?;

        let iter =
            self.clipboard_history_db
                .iter(&wtxn)
                .map_err(|e| SessionStoreError::Backend {
                    operation: "iter_clipboard_history",
                    source: e,
                })?;

        let mut entries = Vec::new();
        for res in iter {
            let (key, it) = res.map_err(|e| SessionStoreError::Backend {
                operation: "decode_clipboard_item",
                source: e,
            })?;
            if it.version == ClipboardItem::CURRENT_VERSION {
                entries.push((key.to_string(), it.last_copied_at));
            }
        }

        entries.sort_by_key(|(_, ts)| *ts);

        if entries.len() > max_entries {
            let to_remove = entries.len() - max_entries;
            for (key, _) in &entries[..to_remove] {
                self.clipboard_history_db
                    .delete(&mut wtxn, key)
                    .map_err(|e| SessionStoreError::Backend {
                        operation: "delete_clipboard_item",
                        source: e,
                    })?;
            }
        }

        wtxn.commit().map_err(|e| SessionStoreError::Backend {
            operation: "commit_record_clipboard",
            source: e,
        })
    }

    pub fn prune_clipboard_history(&self, max_entries: usize) -> Result<(), SessionStoreError> {
        if max_entries == 0 {
            return Err(SessionStoreError::InvalidLimit);
        }

        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "write_txn",
                source: e,
            })?;

        let iter =
            self.clipboard_history_db
                .iter(&wtxn)
                .map_err(|e| SessionStoreError::Backend {
                    operation: "iter_clipboard_history",
                    source: e,
                })?;

        let mut entries = Vec::new();
        for res in iter {
            let (key, it) = res.map_err(|e| SessionStoreError::Backend {
                operation: "decode_clipboard_item",
                source: e,
            })?;
            if it.version == ClipboardItem::CURRENT_VERSION {
                entries.push((key.to_string(), it.last_copied_at));
            }
        }

        entries.sort_by_key(|(_, ts)| *ts);

        if entries.len() > max_entries {
            let to_remove = entries.len() - max_entries;
            for (key, _) in &entries[..to_remove] {
                self.clipboard_history_db
                    .delete(&mut wtxn, key)
                    .map_err(|e| SessionStoreError::Backend {
                        operation: "delete_clipboard_item",
                        source: e,
                    })?;
            }
        }

        wtxn.commit().map_err(|e| SessionStoreError::Backend {
            operation: "commit_prune_clipboard",
            source: e,
        })
    }

    pub fn clipboard_history(
        &self,
        max_entries: usize,
    ) -> Result<Vec<ClipboardItem>, SessionStoreError> {
        if max_entries == 0 {
            return Err(SessionStoreError::InvalidLimit);
        }

        let rtxn = self
            .env
            .read_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "read_txn",
                source: e,
            })?;

        let iter =
            self.clipboard_history_db
                .iter(&rtxn)
                .map_err(|e| SessionStoreError::Backend {
                    operation: "iter_clipboard_history",
                    source: e,
                })?;

        let mut items = Vec::new();
        for res in iter {
            let (_, item) = res.map_err(|e| SessionStoreError::Backend {
                operation: "decode_clipboard_item",
                source: e,
            })?;
            if item.version == ClipboardItem::CURRENT_VERSION {
                items.push(item);
            }
        }

        items.sort_by_key(|i| i.last_copied_at);
        items.reverse();
        items.truncate(max_entries);
        Ok(items)
    }

    pub fn clear_clipboard_history(&self) -> Result<(), SessionStoreError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "write_txn",
                source: e,
            })?;

        self.clipboard_history_db
            .clear(&mut wtxn)
            .map_err(|e| SessionStoreError::Backend {
                operation: "clear_clipboard_history",
                source: e,
            })?;

        wtxn.commit().map_err(|e| SessionStoreError::Backend {
            operation: "commit_clear_clipboard",
            source: e,
        })
    }

    pub fn get_output_bar(
        &self,
        output_name: &str,
    ) -> Result<Option<OutputBarState>, SessionStoreError> {
        let rtxn = self
            .env
            .read_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "read_txn",
                source: e,
            })?;

        let state = self.output_bars_db.get(&rtxn, output_name).map_err(|e| {
            SessionStoreError::Backend {
                operation: "get_output_bar",
                source: e,
            }
        })?;

        Ok(state)
    }

    pub fn put_output_bar(
        &self,
        output_name: &str,
        state: &OutputBarState,
    ) -> Result<(), SessionStoreError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "write_txn",
                source: e,
            })?;

        self.output_bars_db
            .put(&mut wtxn, output_name, state)
            .map_err(|e| SessionStoreError::Backend {
                operation: "put_output_bar",
                source: e,
            })?;

        wtxn.commit().map_err(|e| SessionStoreError::Backend {
            operation: "commit_output_bar",
            source: e,
        })
    }

    pub fn save_audio_preference(&self, pref: &AudioPreference) -> Result<(), SessionStoreError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "write_txn",
                source: e,
            })?;

        self.audio_pref_db
            .put(&mut wtxn, "default", pref)
            .map_err(|e| SessionStoreError::Backend {
                operation: "put_audio_preference",
                source: e,
            })?;

        wtxn.commit().map_err(|e| SessionStoreError::Backend {
            operation: "commit_audio_preference",
            source: e,
        })
    }

    pub fn get_audio_preference(&self) -> Result<AudioPreference, SessionStoreError> {
        let rtxn = self
            .env
            .read_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "read_txn",
                source: e,
            })?;

        let pref = self
            .audio_pref_db
            .get(&rtxn, "default")
            .map_err(|e| SessionStoreError::Backend {
                operation: "get_audio_preference",
                source: e,
            })?
            .unwrap_or_default();

        Ok(pref)
    }

    pub fn get_search_learning_record(
        &self,
        canonical_id: &str,
    ) -> Result<Option<SearchLearningRecord>, SessionStoreError> {
        let rtxn = self
            .env
            .read_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "read_txn",
                source: e,
            })?;

        let record = self
            .search_learning_db
            .get(&rtxn, canonical_id)
            .map_err(|e| SessionStoreError::Backend {
                operation: "get_search_learning_record",
                source: e,
            })?;

        if let Some(rec) = record {
            if rec.version == SearchLearningRecord::CURRENT_VERSION {
                Ok(Some(rec))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub fn list_search_learning_records(
        &self,
    ) -> Result<Vec<(String, SearchLearningRecord)>, SessionStoreError> {
        let rtxn = self
            .env
            .read_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "read_txn",
                source: e,
            })?;

        let iter = self
            .search_learning_db
            .iter(&rtxn)
            .map_err(|e| SessionStoreError::Backend {
                operation: "iter_search_learning",
                source: e,
            })?;

        let mut results = Vec::new();
        for res in iter {
            let (key, rec) = res.map_err(|e| SessionStoreError::Backend {
                operation: "decode_search_learning_record",
                source: e,
            })?;
            if rec.version == SearchLearningRecord::CURRENT_VERSION {
                results.push((key.to_string(), rec));
            }
        }

        Ok(results)
    }

    pub fn record_search_activation(
        &self,
        canonical_id: &str,
        activated_at_secs: u64,
        max_entries: usize,
        half_life_secs: u64,
    ) -> Result<(), SessionStoreError> {
        if max_entries == 0 {
            return Err(SessionStoreError::InvalidLimit);
        }

        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "write_txn",
                source: e,
            })?;

        let existing = self
            .search_learning_db
            .get(&wtxn, canonical_id)
            .map_err(|e| SessionStoreError::Backend {
                operation: "get_search_learning_record",
                source: e,
            })?;

        if let Some(mut record) = existing
            && record.version == SearchLearningRecord::CURRENT_VERSION
        {
            let elapsed = activated_at_secs.saturating_sub(record.last_activated_at_secs);
            let decay_factor = (-(elapsed as f64)
                * (std::f64::consts::LN_2 / (half_life_secs.max(1) as f64)))
                .exp();
            let new_decayed_score = record.decayed_score * decay_factor + 1.0;

            record.decayed_score = new_decayed_score;
            record.activation_count = record.activation_count.saturating_add(1);
            record.last_activated_at_secs = activated_at_secs;

            self.search_learning_db
                .put(&mut wtxn, canonical_id, &record)
                .map_err(|e| SessionStoreError::Backend {
                    operation: "put_search_learning_record",
                    source: e,
                })?;
        } else {
            let mut entries: Vec<(String, u64)> = Vec::new();
            let iter =
                self.search_learning_db
                    .iter(&wtxn)
                    .map_err(|e| SessionStoreError::Backend {
                        operation: "iter_search_learning_for_eviction",
                        source: e,
                    })?;

            for res in iter {
                let (key, rec) = res.map_err(|e| SessionStoreError::Backend {
                    operation: "decode_search_learning_for_eviction",
                    source: e,
                })?;
                entries.push((key.to_string(), rec.last_activated_at_secs));
            }

            if entries.len() >= max_entries {
                entries.sort_by_key(|(_, last_act)| *last_act);
                if let Some((oldest_key, _)) = entries.first() {
                    self.search_learning_db
                        .delete(&mut wtxn, oldest_key)
                        .map_err(|e| SessionStoreError::Backend {
                            operation: "delete_lru_search_learning_record",
                            source: e,
                        })?;
                }
            }

            let new_record = SearchLearningRecord::new_initial(activated_at_secs);
            self.search_learning_db
                .put(&mut wtxn, canonical_id, &new_record)
                .map_err(|e| SessionStoreError::Backend {
                    operation: "put_initial_search_learning_record",
                    source: e,
                })?;
        }

        wtxn.commit().map_err(|e| SessionStoreError::Backend {
            operation: "commit_search_activation",
            source: e,
        })
    }

    pub fn forget_search_result(&self, canonical_id: &str) -> Result<bool, SessionStoreError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "write_txn",
                source: e,
            })?;

        let deleted = self
            .search_learning_db
            .delete(&mut wtxn, canonical_id)
            .map_err(|e| SessionStoreError::Backend {
                operation: "delete_search_learning_record",
                source: e,
            })?;

        wtxn.commit().map_err(|e| SessionStoreError::Backend {
            operation: "commit_forget_search_result",
            source: e,
        })?;

        Ok(deleted)
    }

    pub fn clear_search_learning(&self) -> Result<(), SessionStoreError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| SessionStoreError::Backend {
                operation: "write_txn",
                source: e,
            })?;

        self.search_learning_db
            .clear(&mut wtxn)
            .map_err(|e| SessionStoreError::Backend {
                operation: "clear_search_learning",
                source: e,
            })?;

        wtxn.commit().map_err(|e| SessionStoreError::Backend {
            operation: "commit_clear_search_learning",
            source: e,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_DIR_COUNTER: AtomicU32 = AtomicU32::new(1);

    fn temp_test_dir() -> PathBuf {
        let pid = std::process::id();
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("shilpo_session_test_{pid}_{nanos}_{counter}"));
        let _ = fs::create_dir_all(&path);
        path
    }

    #[test]
    fn clipboard_atomic_retention_and_pruning() {
        let dir = temp_test_dir();
        let base_time = chrono::Utc::now();
        {
            let store = HeedSessionStore::open(&dir).unwrap();
            for i in 1..=105 {
                let time = base_time + chrono::Duration::seconds(i);
                let item = ClipboardItem::new_text(format!("item_{i}"), time);
                store.record_clipboard_item(&item, 100).unwrap();
            }
        }

        // Reopen and check history
        let store = HeedSessionStore::open(&dir).unwrap();
        let history = store.clipboard_history(100).unwrap();
        assert_eq!(history.len(), 100);

        // Newest item should be item_105, oldest returned should be item_6
        assert_eq!(history.first().unwrap().text().unwrap(), "item_105");
        assert_eq!(history.last().unwrap().text().unwrap(), "item_6");

        // Directly query DB to ensure items 1..=5 were deleted
        let rtxn = store.env.read_txn().unwrap();
        for i in 1..=5 {
            let item = ClipboardItem::new_text(format!("item_{i}"), base_time);
            assert!(
                store
                    .clipboard_history_db
                    .get(&rtxn, &item.id)
                    .unwrap()
                    .is_none()
            );
        }

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_clipboard_dedup_and_promote_on_repeat() {
        let dir = temp_test_dir();
        let store = HeedSessionStore::open(&dir).unwrap();
        let t1 = chrono::Utc::now();
        let t2 = t1 + chrono::Duration::seconds(10);
        let t3 = t1 + chrono::Duration::seconds(20);

        let item_a1 = ClipboardItem::new_text("content A".into(), t1);
        let item_b = ClipboardItem::new_text("content B".into(), t2);
        let item_a2 = ClipboardItem::new_text("content A".into(), t3);

        store.record_clipboard_item(&item_a1, 100).unwrap();
        store.record_clipboard_item(&item_b, 100).unwrap();
        store.record_clipboard_item(&item_a2, 100).unwrap();

        let history = store.clipboard_history(100).unwrap();
        assert_eq!(
            history.len(),
            2,
            "copying A, B, then A must yield 2 entries, not 3"
        );
        assert_eq!(history[0].text().unwrap(), "content A");
        assert_eq!(history[0].last_copied_at, t3);
        assert_eq!(history[1].text().unwrap(), "content B");
        assert_eq!(
            history[0].id, item_a1.id,
            "canonical id must be stable across repeats"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_clipboard_two_distinct_contents_same_timestamp_produce_distinct_entries() {
        let dir = temp_test_dir();
        let store = HeedSessionStore::open(&dir).unwrap();
        let identical_time = chrono::Utc::now();

        let item_a = ClipboardItem::new_text("payload alpha".into(), identical_time);
        let item_b = ClipboardItem::new_text("payload beta".into(), identical_time);

        assert_ne!(item_a.id, item_b.id);

        store.record_clipboard_item(&item_a, 100).unwrap();
        store.record_clipboard_item(&item_b, 100).unwrap();

        let history = store.clipboard_history(100).unwrap();
        assert_eq!(
            history.len(),
            2,
            "two distinct contents in same millisecond must produce two entries"
        );
        let texts: Vec<&str> = history.iter().map(|i| i.text().unwrap()).collect();
        assert!(texts.contains(&"payload alpha"));
        assert!(texts.contains(&"payload beta"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_clipboard_different_days_ordering_after_reopen() {
        let dir = temp_test_dir();
        let day1 = chrono::Utc::now() - chrono::Duration::days(2);
        let day2 = chrono::Utc::now() - chrono::Duration::days(1);
        let day3 = chrono::Utc::now();

        {
            let store = HeedSessionStore::open(&dir).unwrap();
            let item1 = ClipboardItem::new_text("day 1 note".into(), day1);
            let item2 = ClipboardItem::new_text("day 2 note".into(), day2);
            let item3 = ClipboardItem::new_text("day 3 note".into(), day3);

            store.record_clipboard_item(&item1, 100).unwrap();
            store.record_clipboard_item(&item2, 100).unwrap();
            store.record_clipboard_item(&item3, 100).unwrap();
        }

        // Reopen store
        let store = HeedSessionStore::open(&dir).unwrap();
        let history = store.clipboard_history(100).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].text().unwrap(), "day 3 note");
        assert_eq!(history[1].text().unwrap(), "day 2 note");
        assert_eq!(history[2].text().unwrap(), "day 1 note");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_clipboard_unknown_version_skipped_safely() {
        let dir = temp_test_dir();
        let store = HeedSessionStore::open(&dir).unwrap();

        let mut wtxn = store.env.write_txn().unwrap();
        let bad_item = ClipboardItem {
            version: 99,
            id: "sha256:future_ver".to_string(),
            content: ClipboardContent::Text("future content".to_string()),
            last_copied_at: chrono::Utc::now(),
        };
        store
            .clipboard_history_db
            .put(&mut wtxn, &bad_item.id, &bad_item)
            .unwrap();
        wtxn.commit().unwrap();

        let history = store.clipboard_history(100).unwrap();
        assert_eq!(
            history.len(),
            0,
            "unknown record version must be filtered safely"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_clipboard_runtime_shrink_prunes_bounds() {
        let dir = temp_test_dir();
        let store = HeedSessionStore::open(&dir).unwrap();
        let base_time = chrono::Utc::now();

        for i in 1..=20 {
            let item = ClipboardItem::new_text(
                format!("item_{i}"),
                base_time + chrono::Duration::seconds(i),
            );
            store.record_clipboard_item(&item, 20).unwrap();
        }

        assert_eq!(store.clipboard_history(20).unwrap().len(), 20);

        // Lower limit at runtime to 5
        store.prune_clipboard_history(5).unwrap();

        let history = store.clipboard_history(20).unwrap();
        assert_eq!(history.len(), 5);
        assert_eq!(history[0].text().unwrap(), "item_20");
        assert_eq!(history[4].text().unwrap(), "item_16");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corruption_classification() {
        assert!(HeedSessionStore::is_corrupt_error(&heed::Error::Mdb(
            heed::MdbError::Corrupted
        )));
        assert!(HeedSessionStore::is_corrupt_error(&heed::Error::Mdb(
            heed::MdbError::Panic
        )));
        assert!(HeedSessionStore::is_corrupt_error(&heed::Error::Mdb(
            heed::MdbError::Invalid
        )));
        assert!(HeedSessionStore::is_corrupt_error(&heed::Error::Mdb(
            heed::MdbError::PageNotFound
        )));

        // Non-corrupt variants must return false
        assert!(!HeedSessionStore::is_corrupt_error(&heed::Error::Mdb(
            heed::MdbError::VersionMismatch
        )));
        assert!(!HeedSessionStore::is_corrupt_error(&heed::Error::Mdb(
            heed::MdbError::KeyExist
        )));
        assert!(!HeedSessionStore::is_corrupt_error(&heed::Error::Mdb(
            heed::MdbError::MapFull
        )));
        assert!(!HeedSessionStore::is_corrupt_error(&heed::Error::Mdb(
            heed::MdbError::DbsFull
        )));
    }

    #[test]
    fn confirmed_corruption_renames_to_quarantine_and_preserves_data() {
        let dir = temp_test_dir();
        // Create store directory with marker
        let marker_file = dir.join("important_marker.txt");
        fs::write(&marker_file, b"user_marker_data").unwrap();

        // Write corrupt bytes to data.mdb
        let data_file = dir.join("data.mdb");
        fs::write(&data_file, b"invalid_garbage_bytes_for_lmdb_header").unwrap();

        // Open with recovery
        let opened = HeedSessionStore::open_with_recovery(&dir).unwrap();
        if let RecoveryOutcome::Quarantined { path } = opened.recovery {
            assert!(path.exists());
            assert!(path.join("important_marker.txt").exists());
            assert_eq!(
                fs::read(path.join("important_marker.txt")).unwrap(),
                b"user_marker_data"
            );
            let _ = fs::remove_dir_all(path);
        } else {
            panic!("expected Quarantined recovery outcome");
        }

        assert!(dir.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn non_corruption_open_error_preserves_dir_and_returns_error() {
        let dir = temp_test_dir();
        let marker = dir.join("marker.txt");
        fs::write(&marker, b"should_remain").unwrap();

        // Create session.lock as a directory to force an I/O error when opening lock file
        fs::create_dir_all(dir.join("session.lock")).unwrap();

        let res = HeedSessionStore::open_with_recovery(&dir);
        assert!(res.is_err());
        assert!(marker.exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn unix_permissions_enforcement() {
        let dir = temp_test_dir();
        let store = HeedSessionStore::open(&dir).unwrap();

        let dir_perm = fs::metadata(&dir).unwrap().permissions();
        assert_eq!(dir_perm.mode() & 0o777, 0o700);

        let data_path = dir.join("data.mdb");
        if data_path.exists() {
            let data_perm = fs::metadata(&data_path).unwrap().permissions();
            assert_eq!(data_perm.mode() & 0o077, 0o000);
        }

        drop(store);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_search_learning_activation_decay_and_reopen() {
        let dir = temp_test_dir();
        let store = HeedSessionStore::open(&dir).unwrap();

        let half_life = 14 * 24 * 3600; // 14 days
        let t0 = 1_000_000;

        // Record activation at t0
        store
            .record_search_activation("app:firefox", t0, 512, half_life)
            .unwrap();

        let rec = store
            .get_search_learning_record("app:firefox")
            .unwrap()
            .unwrap();
        assert_eq!(rec.version, 1);
        assert_eq!(rec.activation_count, 1);
        assert_eq!(rec.last_activated_at_secs, t0);
        assert!((rec.decayed_score - 1.0).abs() < 1e-6);

        // Record second activation after 14 days (half-life)
        let t1 = t0 + half_life;
        store
            .record_search_activation("app:firefox", t1, 512, half_life)
            .unwrap();

        let rec2 = store
            .get_search_learning_record("app:firefox")
            .unwrap()
            .unwrap();
        assert_eq!(rec2.activation_count, 2);
        assert_eq!(rec2.last_activated_at_secs, t1);
        // Previous 1.0 decayed to 0.5 + 1.0 = 1.5
        assert!((rec2.decayed_score - 1.5).abs() < 1e-4);

        // Reopen store and verify persistence
        drop(store);
        let store_reopened = HeedSessionStore::open(&dir).unwrap();
        let rec3 = store_reopened
            .get_search_learning_record("app:firefox")
            .unwrap()
            .unwrap();
        assert_eq!(rec3, rec2);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_search_learning_eviction_at_max_entries() {
        let dir = temp_test_dir();
        let store = HeedSessionStore::open(&dir).unwrap();

        let half_life = 14 * 24 * 3600;
        let max_entries = 3;

        // Insert 3 entries at timestamps 10, 20, 30
        store
            .record_search_activation("item:1", 10, max_entries, half_life)
            .unwrap();
        store
            .record_search_activation("item:2", 20, max_entries, half_life)
            .unwrap();
        store
            .record_search_activation("item:3", 30, max_entries, half_life)
            .unwrap();

        let list = store.list_search_learning_records().unwrap();
        assert_eq!(list.len(), 3);

        // Insert 4th entry at timestamp 40 -> should evict item:1 (timestamp 10)
        store
            .record_search_activation("item:4", 40, max_entries, half_life)
            .unwrap();

        let list2 = store.list_search_learning_records().unwrap();
        assert_eq!(list2.len(), 3);
        assert!(
            store
                .get_search_learning_record("item:1")
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .get_search_learning_record("item:2")
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .get_search_learning_record("item:3")
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .get_search_learning_record("item:4")
                .unwrap()
                .is_some()
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_search_learning_forget_and_clear() {
        let dir = temp_test_dir();
        let store = HeedSessionStore::open(&dir).unwrap();

        store
            .record_search_activation("item:a", 100, 512, 1000)
            .unwrap();
        store
            .record_search_activation("item:b", 200, 512, 1000)
            .unwrap();

        assert_eq!(store.list_search_learning_records().unwrap().len(), 2);

        // Forget item:a
        let deleted = store.forget_search_result("item:a").unwrap();
        assert!(deleted);
        assert!(
            store
                .get_search_learning_record("item:a")
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .get_search_learning_record("item:b")
                .unwrap()
                .is_some()
        );

        // Forget non-existent item
        let deleted_again = store.forget_search_result("item:non_existent").unwrap();
        assert!(!deleted_again);

        // Clear all
        store.clear_search_learning().unwrap();
        assert_eq!(store.list_search_learning_records().unwrap().len(), 0);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_search_learning_rejects_unknown_version() {
        let dir = temp_test_dir();
        let store = HeedSessionStore::open(&dir).unwrap();

        // Write a record with unknown version 99 directly
        let mut wtxn = store.env.write_txn().unwrap();
        let bad_rec = SearchLearningRecord {
            version: 99,
            activation_count: 5,
            last_activated_at_secs: 100,
            decayed_score: 5.0,
        };
        store
            .search_learning_db
            .put(&mut wtxn, "item:bad", &bad_rec)
            .unwrap();
        wtxn.commit().unwrap();

        // get_search_learning_record safely filters it out (returns None)
        assert!(
            store
                .get_search_learning_record("item:bad")
                .unwrap()
                .is_none()
        );
        assert_eq!(store.list_search_learning_records().unwrap().len(), 0);

        let _ = fs::remove_dir_all(dir);
    }
}
