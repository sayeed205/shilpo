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
use heed::byteorder::NativeEndian;
use heed::types::{SerdeJson, Str, U64};
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardItem {
    pub id: u64,
    pub text: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AudioPreference {
    pub default_device: Option<String>,
    pub default_port: Option<String>,
}

pub struct HeedSessionStore {
    env: heed::Env,
    output_bars_db: heed::Database<Str, SerdeJson<OutputBarState>>,
    clipboard_history_db: heed::Database<U64<NativeEndian>, SerdeJson<ClipboardItem>>,
    audio_pref_db: heed::Database<Str, SerdeJson<AudioPreference>>,
    _lock_file: Option<fs::File>,
}

impl HeedSessionStore {
    pub fn default_db_dir() -> PathBuf {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("shilpo").join("session.lmdb")
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

        wtxn.commit().map_err(|e| SessionStoreError::Backend {
            operation: "commit_databases",
            source: e,
        })?;

        Ok(Self {
            env,
            output_bars_db,
            clipboard_history_db,
            audio_pref_db,
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

        let mut entries: Vec<u64> = Vec::new();
        let iter =
            self.clipboard_history_db
                .iter(&wtxn)
                .map_err(|e| SessionStoreError::Backend {
                    operation: "iter_clipboard_history",
                    source: e,
                })?;

        for res in iter {
            let (key, _) = res.map_err(|e| SessionStoreError::Backend {
                operation: "decode_clipboard_item",
                source: e,
            })?;
            entries.push(key);
        }

        entries.sort_unstable();

        if entries.len() > max_entries {
            let to_remove = entries.len() - max_entries;
            for key in &entries[..to_remove] {
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
            items.push(item);
        }

        items.sort_by_key(|i| i.id);
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
        {
            let store = HeedSessionStore::open(&dir).unwrap();
            for i in 1..=105 {
                let item = ClipboardItem {
                    id: i,
                    text: format!("item_{i}"),
                    timestamp: format!("ts_{i}"),
                };
                store.record_clipboard_item(&item, 100).unwrap();
            }
        }

        // Reopen and check history
        let store = HeedSessionStore::open(&dir).unwrap();
        let history = store.clipboard_history(100).unwrap();
        assert_eq!(history.len(), 100);

        // Newest item should be ID 105, oldest returned should be ID 6
        assert_eq!(history.first().unwrap().id, 105);
        assert_eq!(history.last().unwrap().id, 6);

        // Directly query DB to ensure IDs 1..=5 were deleted
        let rtxn = store.env.read_txn().unwrap();
        for id in 1..=5 {
            assert!(
                store
                    .clipboard_history_db
                    .get(&rtxn, &id)
                    .unwrap()
                    .is_none()
            );
        }

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
}
