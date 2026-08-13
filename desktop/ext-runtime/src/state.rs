//! Extension-scoped typed state store.
//!
//! State is namespaced by validated [`ExtensionId`] and a UTF-8 key, stored in a dedicated
//! LMDB environment beneath `CatalogPaths::data_dir/extensions/state.lmdb`, and exposed
//! through a narrow [`StateStore`] trait so tests can inject a deterministic in-memory
//! fake without opening LMDB.
//!
//! # Guarantees
//!
//! - A key must be non-empty and at most [`MAX_KEY_BYTES`] UTF-8 bytes.
//! - Each extension may store at most [`MAX_KEYS_PER_EXTENSION`] keys.
//! - Each encoded value may occupy at most [`MAX_VALUE_BYTES`].
//! - The sum of encoded key bytes plus encoded value bytes per extension is at most
//!   [`MAX_TOTAL_BYTES_PER_EXTENSION`].
//! - A changed write or deletion increments the extension's durable `u64` revision
//!   exactly once in the same LMDB write transaction as the mutation. Idempotent
//!   mutations (`changed = false`) never increment the revision.
//! - [`StateValue::SecretRef`] does not exist: the typed value set has no credential
//!   variant, and corrupt persisted records that claim an unknown (secret-ref-shaped)
//!   tag fail closed with [`StateStoreError::Corrupt`] instead of yielding a handle.
//! - Values are persisted in a stable tagged encoding; Rust enum layout, debug
//!   formatting, and JSON never reach the disk.
//!
//! The production [`HeedStateStore`] fails closed: if the persistent store cannot be
//! opened, runtime construction fails rather than silently falling back to memory.

use heed::types::{Bytes, Str};
use shilpo_ext_api::ExtensionId;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StatePolicy {
    #[default]
    Retain,
    Delete,
}

/// Maximum length of a state key in UTF-8 bytes.
pub const MAX_KEY_BYTES: usize = 256;
/// Maximum number of keys per extension.
pub const MAX_KEYS_PER_EXTENSION: usize = 256;
/// Maximum encoded size of a single value, in bytes.
pub const MAX_VALUE_BYTES: usize = 64 * 1024;
/// Maximum sum of encoded key bytes plus encoded value bytes per extension.
pub const MAX_TOTAL_BYTES_PER_EXTENSION: u64 = 4 * 1024 * 1024;
/// Upper bound on queued, undelivered state-change events per extension. When the
/// queue exceeds this bound, pending updates are coalesced by `(extension_id,
/// watch_id)` to the latest revision.
pub const MAX_PENDING_STATE_EVENTS: usize = 64;

/// Typed value accepted by the extension state store.
///
/// Deliberately has no secret-ref variant: extension state must never become a
/// credential store.
#[derive(Clone, Debug, PartialEq)]
pub enum StateValue {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
}

/// A value snapshot with the extension's durable revision at read time.
#[derive(Clone, Debug, PartialEq)]
pub struct StateSnapshot {
    pub value: Option<StateValue>,
    pub revision: u64,
}

/// Outcome of a state mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateMutation {
    pub changed: bool,
    pub revision: u64,
}

/// Typed store errors. Messages never include stored values or bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateStoreError {
    /// Key is empty or exceeds [`MAX_KEY_BYTES`].
    InvalidKey(String),
    /// Value cannot be represented by the persisted encoding (non-finite float).
    InvalidValue(String),
    /// Writing would exceed [`MAX_KEYS_PER_EXTENSION`].
    KeyCountLimit,
    /// Encoded value exceeds [`MAX_VALUE_BYTES`].
    ValueSizeLimit { limit: usize, actual: usize },
    /// Sum of encoded key and value bytes would exceed [`MAX_TOTAL_BYTES_PER_EXTENSION`].
    ByteBudgetExceeded { limit: u64, actual: u64 },
    /// The per-extension revision counter is exhausted.
    RevisionOverflow,
    /// Persisted data could not be decoded.
    Corrupt(String),
    /// The backing LMDB environment could not be opened or serviced.
    BackendUnavailable(String),
    /// Unexpected internal failure.
    Internal(String),
}

impl fmt::Display for StateStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey(message) => write!(f, "invalid state key: {message}"),
            Self::InvalidValue(message) => write!(f, "invalid state value: {message}"),
            Self::KeyCountLimit => write!(
                f,
                "state key count limit exceeded: at most {MAX_KEYS_PER_EXTENSION} keys per extension"
            ),
            Self::ValueSizeLimit { limit, actual } => write!(
                f,
                "state value size limit exceeded: {actual} bytes is over the {limit} byte limit"
            ),
            Self::ByteBudgetExceeded { limit, actual } => write!(
                f,
                "state byte budget exceeded: {actual} bytes is over the {limit} byte limit"
            ),
            Self::RevisionOverflow => write!(f, "state revision counter exhausted"),
            Self::Corrupt(message) => write!(f, "corrupt persisted state: {message}"),
            Self::BackendUnavailable(message) => {
                write!(f, "state store backend unavailable: {message}")
            }
            Self::Internal(message) => write!(f, "internal state store error: {message}"),
        }
    }
}

impl std::error::Error for StateStoreError {}

/// Injectable state-store boundary used by the Wasmtime host implementation and by
/// uninstall logic. Implementations must be safe to share across threads.
pub trait StateStore: Send + Sync {
    /// Reads the value and current revision for `key`; a missing key yields
    /// `StateSnapshot { value: None, revision: current-extension-revision }`.
    fn read(&self, extension_id: &ExtensionId, key: &str)
    -> Result<StateSnapshot, StateStoreError>;

    /// Writes `value` for `key`. A byte-for-byte equivalent value is an idempotent
    /// no-op. A changed write increments the revision exactly once.
    fn write(
        &self,
        extension_id: &ExtensionId,
        key: &str,
        value: StateValue,
    ) -> Result<StateMutation, StateStoreError>;

    /// Deletes `key`. Deleting a missing key is an idempotent no-op. A changed
    /// deletion increments the revision exactly once.
    fn delete(
        &self,
        extension_id: &ExtensionId,
        key: &str,
    ) -> Result<StateMutation, StateStoreError>;

    /// Removes all values and metadata for exactly one Extension ID and resets its
    /// revision to `0`. Never touches another extension's namespace.
    fn delete_all(&self, extension_id: &ExtensionId) -> Result<(), StateStoreError>;
}

fn validate_key(key: &str) -> Result<(), StateStoreError> {
    if key.is_empty() {
        return Err(StateStoreError::InvalidKey("key must be non-empty".into()));
    }
    if key.len() > MAX_KEY_BYTES {
        return Err(StateStoreError::InvalidKey(format!(
            "key is {} bytes; the maximum is {MAX_KEY_BYTES}",
            key.len()
        )));
    }
    Ok(())
}

/// Stable tagged representation of a [`StateValue`]. The tags are part of the on-disk
/// contract and must never be repurposed.
const TAG_NONE: u8 = 0x00;
const TAG_BOOL: u8 = 0x01;
const TAG_INT: u8 = 0x02;
const TAG_FLOAT: u8 = 0x03;
const TAG_TEXT: u8 = 0x04;
const TAG_BYTES: u8 = 0x05;

pub(crate) fn encode_value(value: &StateValue) -> Result<Vec<u8>, StateStoreError> {
    match value {
        StateValue::None => Ok(vec![TAG_NONE]),
        StateValue::Bool(value) => Ok(vec![TAG_BOOL, *value as u8]),
        StateValue::Int(value) => {
            let mut encoded = Vec::with_capacity(9);
            encoded.push(TAG_INT);
            encoded.extend_from_slice(&value.to_le_bytes());
            Ok(encoded)
        }
        StateValue::Float(value) => {
            if !value.is_finite() {
                return Err(StateStoreError::InvalidValue(
                    "non-finite floats cannot be stored in extension state".into(),
                ));
            }
            let mut encoded = Vec::with_capacity(9);
            encoded.push(TAG_FLOAT);
            encoded.extend_from_slice(&value.to_bits().to_le_bytes());
            Ok(encoded)
        }
        StateValue::Text(value) => {
            let mut encoded = Vec::with_capacity(5 + value.len());
            encoded.push(TAG_TEXT);
            encoded.extend_from_slice(&(value.len() as u32).to_le_bytes());
            encoded.extend_from_slice(value.as_bytes());
            Ok(encoded)
        }
        StateValue::Bytes(value) => {
            let mut encoded = Vec::with_capacity(5 + value.len());
            encoded.push(TAG_BYTES);
            encoded.extend_from_slice(&(value.len() as u32).to_le_bytes());
            encoded.extend_from_slice(value);
            Ok(encoded)
        }
    }
}

/// Encoded byte length of a value record, matching [`encode_value`] without
/// allocating. Used to charge guest hostcall budget.
pub fn encoded_len(value: &StateValue) -> usize {
    match value {
        StateValue::None => 1,
        StateValue::Bool(_) => 2,
        StateValue::Int(_) | StateValue::Float(_) => 9,
        StateValue::Text(value) => 5 + value.len(),
        StateValue::Bytes(value) => 5 + value.len(),
    }
}

pub(crate) fn decode_value(bytes: &[u8]) -> Result<StateValue, StateStoreError> {
    let (tag, rest) = bytes
        .split_first()
        .ok_or_else(|| StateStoreError::Corrupt("empty state record".into()))?;
    match *tag {
        TAG_NONE if rest.is_empty() => Ok(StateValue::None),
        TAG_BOOL if rest.len() == 1 => match rest[0] {
            0 => Ok(StateValue::Bool(false)),
            1 => Ok(StateValue::Bool(true)),
            _ => Err(StateStoreError::Corrupt(
                "boolean record contains an invalid value".into(),
            )),
        },
        TAG_INT if rest.len() == 8 => Ok(StateValue::Int(i64::from_le_bytes(
            rest.try_into().expect("length checked"),
        ))),
        TAG_FLOAT if rest.len() == 8 => {
            let bits = u64::from_le_bytes(rest.try_into().expect("length checked"));
            let value = f64::from_bits(bits);
            if value.is_finite() {
                Ok(StateValue::Float(value))
            } else {
                Err(StateStoreError::Corrupt(
                    "record contains a non-finite float".into(),
                ))
            }
        }
        TAG_TEXT => {
            let length = read_u32_payload(rest)?;
            let text = rest
                .get(4..4 + length as usize)
                .ok_or_else(|| StateStoreError::Corrupt("truncated text record".into()))?;
            let text = std::str::from_utf8(text)
                .map_err(|_| StateStoreError::Corrupt("text record is not UTF-8".into()))?;
            if rest.len() != 4 + length as usize {
                return Err(StateStoreError::Corrupt(
                    "text record contains trailing bytes".into(),
                ));
            }
            Ok(StateValue::Text(text.to_owned()))
        }
        TAG_BYTES => {
            let length = read_u32_payload(rest)?;
            let bytes = rest
                .get(4..4 + length as usize)
                .ok_or_else(|| StateStoreError::Corrupt("truncated bytes record".into()))?;
            if rest.len() != 4 + length as usize {
                return Err(StateStoreError::Corrupt(
                    "bytes record contains trailing bytes".into(),
                ));
            }
            Ok(StateValue::Bytes(bytes.to_vec()))
        }
        tag => Err(StateStoreError::Corrupt(format!(
            "record has unknown tag byte {tag}"
        ))),
    }
}

fn read_u32_payload(bytes: &[u8]) -> Result<u32, StateStoreError> {
    let raw = bytes
        .get(..4)
        .ok_or_else(|| StateStoreError::Corrupt("truncated record length".into()))?;
    Ok(u32::from_le_bytes(raw.try_into().expect("length checked")))
}

/// Per-extension bookkeeping persisted alongside the values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct ExtensionMetadata {
    revision: u64,
    key_count: u32,
    byte_total: u64,
}

const METADATA_ENCODED_LEN: usize = 8 + 4 + 8;

fn encode_metadata(metadata: &ExtensionMetadata) -> [u8; METADATA_ENCODED_LEN] {
    let mut encoded = [0u8; METADATA_ENCODED_LEN];
    encoded[..8].copy_from_slice(&metadata.revision.to_le_bytes());
    encoded[8..12].copy_from_slice(&metadata.key_count.to_le_bytes());
    encoded[12..].copy_from_slice(&metadata.byte_total.to_le_bytes());
    encoded
}

fn decode_metadata(bytes: &[u8]) -> Result<ExtensionMetadata, StateStoreError> {
    if bytes.len() != METADATA_ENCODED_LEN {
        return Err(StateStoreError::Corrupt(format!(
            "metadata record has {} bytes; expected {METADATA_ENCODED_LEN}",
            bytes.len()
        )));
    }
    Ok(ExtensionMetadata {
        revision: u64::from_le_bytes(bytes[..8].try_into().expect("length checked")),
        key_count: u32::from_le_bytes(bytes[8..12].try_into().expect("length checked")),
        byte_total: u64::from_le_bytes(bytes[12..].try_into().expect("length checked")),
    })
}

/// On-disk key for a value: `<extension-id>\0<key>`. The separator cannot appear in a
/// validated extension ID, so raw guest strings can never select another extension's
/// namespace.
fn composite_key(extension_id: &ExtensionId, key: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(extension_id.as_str().len() + 1 + key.len());
    bytes.extend_from_slice(extension_id.as_str().as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(key.as_bytes());
    bytes
}

enum MutationPlan {
    NoOp,
    Apply {
        new_revision: u64,
        new_key_count: u32,
        new_byte_total: u64,
    },
}

/// Computes quota evaluation and the revision/count/byte update for one mutation.
/// Rejected plans return an error and leave all metadata untouched.
fn plan_mutation(
    metadata: &ExtensionMetadata,
    key_bytes: usize,
    existing_value: Option<&[u8]>,
    new_value: Option<&[u8]>,
) -> Result<MutationPlan, StateStoreError> {
    match (existing_value, new_value) {
        (Some(old), Some(new)) if old == new => Ok(MutationPlan::NoOp),
        (None, None) => Ok(MutationPlan::NoOp),
        (existing, incoming) => {
            if let Some(incoming) = incoming {
                if incoming.len() > MAX_VALUE_BYTES {
                    return Err(StateStoreError::ValueSizeLimit {
                        limit: MAX_VALUE_BYTES,
                        actual: incoming.len(),
                    });
                }
                if existing.is_none() && metadata.key_count >= MAX_KEYS_PER_EXTENSION as u32 {
                    return Err(StateStoreError::KeyCountLimit);
                }
            }
            let old_bytes = existing.map_or(0, |value| value.len());
            let new_bytes = incoming.map_or(0, |value| value.len());
            let new_key = existing.is_none();
            let removing = incoming.is_none();
            let delta = (key_bytes + new_bytes) as i128 - (key_bytes + old_bytes) as i128;
            let new_total = metadata.byte_total as i128 + delta;
            if new_total < 0 {
                return Err(StateStoreError::Internal(
                    "state byte accounting underflow".into(),
                ));
            }
            if new_total > MAX_TOTAL_BYTES_PER_EXTENSION as i128 {
                return Err(StateStoreError::ByteBudgetExceeded {
                    limit: MAX_TOTAL_BYTES_PER_EXTENSION,
                    actual: new_total as u64,
                });
            }
            let new_revision = metadata
                .revision
                .checked_add(1)
                .ok_or(StateStoreError::RevisionOverflow)?;
            let new_key_count = metadata.key_count + new_key as u32 - removing as u32;
            Ok(MutationPlan::Apply {
                new_revision,
                new_key_count,
                new_byte_total: new_total as u64,
            })
        }
    }
}

/// Deterministic in-memory [`StateStore`] for tests. Applies the same encoding,
/// quota, and revision rules as [`HeedStateStore`].
#[derive(Default)]
pub struct FakeStateStore {
    inner: Mutex<FakeInner>,
}

#[derive(Default)]
struct FakeInner {
    values: HashMap<(String, String), Vec<u8>>,
    metadata: BTreeMap<String, ExtensionMetadata>,
}

impl FakeStateStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StateStore for FakeStateStore {
    fn read(
        &self,
        extension_id: &ExtensionId,
        key: &str,
    ) -> Result<StateSnapshot, StateStoreError> {
        validate_key(key)?;
        let inner = self.inner.lock().expect("fake state store lock poisoned");
        let metadata = inner
            .metadata
            .get(extension_id.as_str())
            .copied()
            .unwrap_or_default();
        let value = inner
            .values
            .get(&(extension_id.to_string(), key.to_owned()))
            .map(|encoded| decode_value(encoded))
            .transpose()?;
        Ok(StateSnapshot {
            value,
            revision: metadata.revision,
        })
    }

    fn write(
        &self,
        extension_id: &ExtensionId,
        key: &str,
        value: StateValue,
    ) -> Result<StateMutation, StateStoreError> {
        validate_key(key)?;
        let encoded = encode_value(&value)?;
        let mut inner = self.inner.lock().expect("fake state store lock poisoned");
        let mut metadata = inner
            .metadata
            .get(extension_id.as_str())
            .copied()
            .unwrap_or_default();
        let location = (extension_id.to_string(), key.to_owned());
        let existing = inner.values.get(&location).cloned();
        if let Some(record) = &existing {
            decode_value(record)?;
        }
        match plan_mutation(&metadata, key.len(), existing.as_deref(), Some(&encoded))? {
            MutationPlan::NoOp => Ok(StateMutation {
                changed: false,
                revision: metadata.revision,
            }),
            MutationPlan::Apply {
                new_revision,
                new_key_count,
                new_byte_total,
            } => {
                inner.values.insert(location, encoded);
                metadata = ExtensionMetadata {
                    revision: new_revision,
                    key_count: new_key_count,
                    byte_total: new_byte_total,
                };
                inner.metadata.insert(extension_id.to_string(), metadata);
                Ok(StateMutation {
                    changed: true,
                    revision: new_revision,
                })
            }
        }
    }

    fn delete(
        &self,
        extension_id: &ExtensionId,
        key: &str,
    ) -> Result<StateMutation, StateStoreError> {
        validate_key(key)?;
        let mut inner = self.inner.lock().expect("fake state store lock poisoned");
        let mut metadata = inner
            .metadata
            .get(extension_id.as_str())
            .copied()
            .unwrap_or_default();
        let location = (extension_id.to_string(), key.to_owned());
        let existing = inner.values.get(&location).cloned();
        if let Some(record) = &existing {
            decode_value(record)?;
        }
        match plan_mutation(&metadata, key.len(), existing.as_deref(), None)? {
            MutationPlan::NoOp => Ok(StateMutation {
                changed: false,
                revision: metadata.revision,
            }),
            MutationPlan::Apply {
                new_revision,
                new_key_count,
                new_byte_total,
            } => {
                inner.values.remove(&location);
                metadata = ExtensionMetadata {
                    revision: new_revision,
                    key_count: new_key_count,
                    byte_total: new_byte_total,
                };
                inner.metadata.insert(extension_id.to_string(), metadata);
                Ok(StateMutation {
                    changed: true,
                    revision: new_revision,
                })
            }
        }
    }

    fn delete_all(&self, extension_id: &ExtensionId) -> Result<(), StateStoreError> {
        let mut inner = self.inner.lock().expect("fake state store lock poisoned");
        let name = extension_id.to_string();
        inner.values.retain(|(owner, _), _| *owner != name);
        inner.metadata.remove(&name);
        Ok(())
    }
}

/// LMDB-backed production [`StateStore`]. Fails closed on open.
pub struct HeedStateStore {
    env: heed::Env,
    values: heed::Database<Bytes, Bytes>,
    metadata: heed::Database<Str, Bytes>,
    _lock_file: Option<fs::File>,
}

impl fmt::Debug for HeedStateStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeedStateStore")
            .field("env", &"<lmdb>")
            .finish_non_exhaustive()
    }
}

impl HeedStateStore {
    /// Opens (creating if needed) the LMDB environment at `dir` with private Unix
    /// permissions matching the session-store privacy posture.
    pub fn open(dir: &Path) -> Result<Self, StateStoreError> {
        fs::create_dir_all(dir)
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
                .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        }

        let lock_path = dir.join("state.lock");
        let lock_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            lock_file
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        }

        let env = unsafe {
            heed::EnvOpenOptions::new()
                .max_dbs(4)
                .map_size(256 * 1024 * 1024)
                .open(dir)
                .map_err(|error| {
                    StateStoreError::BackendUnavailable(format!(
                        "failed to open LMDB environment: {error}"
                    ))
                })?
        };

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for file_name in ["data.mdb", "lock.mdb"] {
                let file_path = dir.join(file_name);
                if file_path.exists() {
                    fs::set_permissions(&file_path, fs::Permissions::from_mode(0o600))
                        .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
                }
            }
        }

        let mut wtxn = env
            .write_txn()
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        let values = env
            .create_database(&mut wtxn, Some("values"))
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        let metadata = env
            .create_database(&mut wtxn, Some("metadata"))
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        wtxn.commit()
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;

        Ok(Self {
            env,
            values,
            metadata,
            _lock_file: Some(lock_file),
        })
    }
}

impl StateStore for HeedStateStore {
    fn read(
        &self,
        extension_id: &ExtensionId,
        key: &str,
    ) -> Result<StateSnapshot, StateStoreError> {
        validate_key(key)?;
        let txn = self
            .env
            .read_txn()
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        let metadata = self
            .metadata
            .get(&txn, extension_id.as_str())
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?
            .map(decode_metadata)
            .transpose()?
            .unwrap_or_default();
        let value = self
            .values
            .get(&txn, composite_key(extension_id, key).as_slice())
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?
            .map(decode_value)
            .transpose()?;
        Ok(StateSnapshot {
            value,
            revision: metadata.revision,
        })
    }

    fn write(
        &self,
        extension_id: &ExtensionId,
        key: &str,
        value: StateValue,
    ) -> Result<StateMutation, StateStoreError> {
        validate_key(key)?;
        let encoded = encode_value(&value)?;
        let mut txn = self
            .env
            .write_txn()
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        let metadata = self
            .metadata
            .get(&txn, extension_id.as_str())
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?
            .map(decode_metadata)
            .transpose()?
            .unwrap_or_default();
        let db_key = composite_key(extension_id, key);
        let existing = self
            .values
            .get(&txn, db_key.as_slice())
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?
            .map(|value| value.to_vec());
        if let Some(record) = &existing {
            decode_value(record)?;
        }
        match plan_mutation(&metadata, key.len(), existing.as_deref(), Some(&encoded))? {
            MutationPlan::NoOp => Ok(StateMutation {
                changed: false,
                revision: metadata.revision,
            }),
            MutationPlan::Apply {
                new_revision,
                new_key_count,
                new_byte_total,
            } => {
                self.values
                    .put(&mut txn, db_key.as_slice(), encoded.as_slice())
                    .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
                let next = ExtensionMetadata {
                    revision: new_revision,
                    key_count: new_key_count,
                    byte_total: new_byte_total,
                };
                self.metadata
                    .put(&mut txn, extension_id.as_str(), &encode_metadata(&next))
                    .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
                txn.commit()
                    .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
                Ok(StateMutation {
                    changed: true,
                    revision: new_revision,
                })
            }
        }
    }

    fn delete(
        &self,
        extension_id: &ExtensionId,
        key: &str,
    ) -> Result<StateMutation, StateStoreError> {
        validate_key(key)?;
        let mut txn = self
            .env
            .write_txn()
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        let metadata = self
            .metadata
            .get(&txn, extension_id.as_str())
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?
            .map(decode_metadata)
            .transpose()?
            .unwrap_or_default();
        let db_key = composite_key(extension_id, key);
        let existing = self
            .values
            .get(&txn, db_key.as_slice())
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?
            .map(|value| value.to_vec());
        if let Some(record) = &existing {
            decode_value(record)?;
        }
        match plan_mutation(&metadata, key.len(), existing.as_deref(), None)? {
            MutationPlan::NoOp => Ok(StateMutation {
                changed: false,
                revision: metadata.revision,
            }),
            MutationPlan::Apply {
                new_revision,
                new_key_count,
                new_byte_total,
            } => {
                self.values
                    .delete(&mut txn, db_key.as_slice())
                    .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
                let next = ExtensionMetadata {
                    revision: new_revision,
                    key_count: new_key_count,
                    byte_total: new_byte_total,
                };
                self.metadata
                    .put(&mut txn, extension_id.as_str(), &encode_metadata(&next))
                    .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
                txn.commit()
                    .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
                Ok(StateMutation {
                    changed: true,
                    revision: new_revision,
                })
            }
        }
    }

    fn delete_all(&self, extension_id: &ExtensionId) -> Result<(), StateStoreError> {
        let mut txn = self
            .env
            .write_txn()
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        let prefix = composite_key(extension_id, "");
        let mut keys = Vec::new();
        let mut cursor = self
            .values
            .prefix_iter(&txn, prefix.as_slice())
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        while let Some(entry) = cursor
            .next()
            .transpose()
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?
        {
            keys.push(entry.0.to_vec());
        }
        drop(cursor);
        for key in keys {
            self.values
                .delete(&mut txn, key.as_slice())
                .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        }
        self.metadata
            .delete(&mut txn, extension_id.as_str())
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        txn.commit()
            .map_err(|error| StateStoreError::BackendUnavailable(format!("{error}")))?;
        Ok(())
    }
}

#[cfg(test)]
impl HeedStateStore {
    /// Test-only hook to write a raw record so corrupt persisted data can be
    /// exercised without a second LMDB handle on the same directory.
    pub(crate) fn inject_raw_record(&self, extension_id: &ExtensionId, key: &str, raw: &[u8]) {
        let mut txn = self.env.write_txn().expect("write txn");
        let db_key = composite_key(extension_id, key);
        self.values
            .put(&mut txn, db_key.as_slice(), raw)
            .expect("raw put");
        txn.commit().expect("commit");
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    pub(crate) fn isolation(store: &dyn StateStore) {
        let alice = ExtensionId::new("io.github.alice.state").unwrap();
        let bob = ExtensionId::new("io.github.bob.state").unwrap();

        let mutation = store
            .write(&alice, "shared", StateValue::Text("alice-value".into()))
            .unwrap();
        assert!(mutation.changed);
        assert_eq!(mutation.revision, 1);

        let bob_snapshot = store.read(&bob, "shared").unwrap();
        assert_eq!(bob_snapshot.value, None);
        assert_eq!(
            bob_snapshot.revision, 0,
            "sibling writes must not move revisions"
        );

        let bob_delete = store.delete(&bob, "shared").unwrap();
        assert!(!bob_delete.changed);
        assert_eq!(bob_delete.revision, 0);

        let bob_write = store.write(&bob, "shared", StateValue::Int(1)).unwrap();
        assert_eq!(bob_write.revision, 1);
        let alice_snapshot = store.read(&alice, "shared").unwrap();
        assert_eq!(
            alice_snapshot.value,
            Some(StateValue::Text("alice-value".into()))
        );
        assert_eq!(
            alice_snapshot.revision, 1,
            "cross-extension writes must not move revisions"
        );
    }

    pub(crate) fn key_limits(store: &dyn StateStore) {
        let id = ExtensionId::new("io.github.test.key-limits").unwrap();
        let error = store.write(&id, "", StateValue::Int(1)).unwrap_err();
        assert!(matches!(error, StateStoreError::InvalidKey(_)));
        let error = store.read(&id, "").unwrap_err();
        assert!(matches!(error, StateStoreError::InvalidKey(_)));
        let error = store.delete(&id, "").unwrap_err();
        assert!(matches!(error, StateStoreError::InvalidKey(_)));

        let max_key = "k".repeat(MAX_KEY_BYTES);
        store.write(&id, &max_key, StateValue::Int(1)).unwrap();
        let oversized = "k".repeat(MAX_KEY_BYTES + 1);
        let error = store
            .write(&id, &oversized, StateValue::Int(1))
            .unwrap_err();
        assert!(matches!(error, StateStoreError::InvalidKey(_)));
        let error = store.read(&id, &oversized).unwrap_err();
        assert!(matches!(error, StateStoreError::InvalidKey(_)));
    }

    pub(crate) fn revision_semantics(store: &dyn StateStore) {
        let id = ExtensionId::new("io.github.test.revisions").unwrap();

        let initial = store.read(&id, "k").unwrap();
        assert_eq!(initial.value, None);
        assert_eq!(initial.revision, 0, "revision starts at 0");

        let first = store.write(&id, "k", StateValue::Int(1)).unwrap();
        assert!(first.changed);
        assert_eq!(first.revision, 1);

        let idempotent = store.write(&id, "k", StateValue::Int(1)).unwrap();
        assert!(!idempotent.changed, "byte-equivalent write is a no-op");
        assert_eq!(
            idempotent.revision, 1,
            "idempotent writes must not bump revision"
        );

        let changed = store.write(&id, "k", StateValue::Int(2)).unwrap();
        assert!(changed.changed);
        assert_eq!(changed.revision, 2);

        let deleted = store.delete(&id, "k").unwrap();
        assert!(deleted.changed);
        assert_eq!(deleted.revision, 3);

        let missing_delete = store.delete(&id, "k").unwrap();
        assert!(!missing_delete.changed);
        assert_eq!(
            missing_delete.revision, 3,
            "deleting a missing key is a no-op"
        );

        let snapshot = store.read(&id, "k").unwrap();
        assert_eq!(snapshot.value, None);
        assert_eq!(
            snapshot.revision, 3,
            "reads report the current extension revision"
        );
    }

    pub(crate) fn value_round_trips(store: &dyn StateStore) {
        let id = ExtensionId::new("io.github.test.round-trip").unwrap();
        let cases = [
            StateValue::None,
            StateValue::Bool(false),
            StateValue::Bool(true),
            StateValue::Int(0),
            StateValue::Int(-1),
            StateValue::Int(i64::MIN),
            StateValue::Int(i64::MAX),
            StateValue::Float(0.0),
            StateValue::Float(-0.0),
            StateValue::Float(1.5),
            StateValue::Float(f64::MIN_POSITIVE),
            StateValue::Float(1e300),
            StateValue::Float(-123.456e-10),
            StateValue::Text(String::new()),
            StateValue::Text("hello, 世界 🎉".into()),
            StateValue::Bytes(Vec::new()),
            StateValue::Bytes(vec![0x00, 0xff, 0x42, 0x00]),
        ];
        for (index, value) in cases.iter().enumerate() {
            let key = format!("k{index}");
            store.write(&id, &key, value.clone()).unwrap();
            let snapshot = store.read(&id, &key).unwrap();
            assert_eq!(
                snapshot.value.as_ref(),
                Some(value),
                "value for '{key}' must round-trip losslessly"
            );
        }

        for non_finite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let error = store
                .write(&id, "nan", StateValue::Float(non_finite))
                .unwrap_err();
            assert!(matches!(error, StateStoreError::InvalidValue(_)));
        }
    }

    pub(crate) fn quota_key_count_and_atomicity(store: &dyn StateStore) {
        let id = ExtensionId::new("io.github.test.key-count").unwrap();
        for index in 0..MAX_KEYS_PER_EXTENSION {
            store
                .write(&id, &format!("key-{index}"), StateValue::Int(index as i64))
                .unwrap();
        }
        let before = store.read(&id, "key-0").unwrap();
        let error = store
            .write(&id, "overflow-key", StateValue::Text("rejected".into()))
            .unwrap_err();
        assert!(matches!(error, StateStoreError::KeyCountLimit));

        let after = store.read(&id, "key-0").unwrap();
        assert_eq!(
            before, after,
            "rejected writes must leave values and revision unchanged"
        );
        let overflow = store.read(&id, "overflow-key").unwrap();
        assert_eq!(
            overflow.value, None,
            "rejected writes must not create the key"
        );
    }

    pub(crate) fn quota_value_size_boundary(store: &dyn StateStore) {
        let id = ExtensionId::new("io.github.test.value-size").unwrap();
        let payload_at_limit = vec![0xAB; MAX_VALUE_BYTES - 5];
        store
            .write(&id, "k", StateValue::Bytes(payload_at_limit.clone()))
            .unwrap();
        let payload_over_limit = vec![0xAB; MAX_VALUE_BYTES - 4];
        let error = store
            .write(&id, "k", StateValue::Bytes(payload_over_limit))
            .unwrap_err();
        assert!(matches!(
            error,
            StateStoreError::ValueSizeLimit { actual, .. } if actual == MAX_VALUE_BYTES + 1
        ));
        let snapshot = store.read(&id, "k").unwrap();
        assert_eq!(
            snapshot.value,
            Some(StateValue::Bytes(payload_at_limit)),
            "rejected replacement must leave the stored value untouched"
        );
        assert_eq!(snapshot.revision, 1);
    }

    pub(crate) fn quota_byte_budget_and_replacement_delta(store: &dyn StateStore) {
        let id = ExtensionId::new("io.github.test.byte-budget").unwrap();
        let mut fills = 0;
        while fills < MAX_KEYS_PER_EXTENSION {
            let key = format!("a{fills:03}");
            match store.write(&id, &key, StateValue::Bytes(vec![0xCD; 32768])) {
                Ok(_) => fills += 1,
                Err(StateStoreError::ByteBudgetExceeded { .. }) => break,
                Err(error) => panic!("unexpected fill failure: {error}"),
            }
        }
        assert!(
            fills < MAX_KEYS_PER_EXTENSION,
            "byte budget must be reachable with per-key values"
        );
        let rejected = store.write(
            &id,
            &format!("a{fills:03}"),
            StateValue::Bytes(vec![0xCD; 32768]),
        );
        assert!(matches!(
            rejected,
            Err(StateStoreError::ByteBudgetExceeded { .. })
        ));

        let before = store.read(&id, "a000").unwrap();
        let error = store
            .write(
                &id,
                "a000",
                StateValue::Bytes(vec![0xCD; MAX_VALUE_BYTES - 5]),
            )
            .unwrap_err();
        assert!(
            matches!(error, StateStoreError::ByteBudgetExceeded { .. }),
            "a replacement delta past the budget must be rejected"
        );
        let after = store.read(&id, "a000").unwrap();
        assert_eq!(before, after, "rejected replacement delta must be atomic");
    }

    pub(crate) fn corrupt_records_fail_closed(
        store: &dyn StateStore,
        corrupt: impl Fn(&ExtensionId),
    ) {
        let id = ExtensionId::new("io.github.test.corrupt").unwrap();
        store
            .write(&id, "good", StateValue::Text("healthy".into()))
            .unwrap();
        corrupt(&id);
        let error = store.read(&id, "good").unwrap_err();
        assert!(matches!(error, StateStoreError::Corrupt(_)));
        let error = store.write(&id, "good", StateValue::Int(1)).unwrap_err();
        assert!(
            matches!(error, StateStoreError::Corrupt(_)),
            "writes over corrupt records must fail closed"
        );
        let error = store.delete(&id, "good").unwrap_err();
        assert!(
            matches!(error, StateStoreError::Corrupt(_)),
            "deletes of corrupt records must fail closed"
        );
    }

    pub(crate) fn delete_all_is_namespace_exact_and_resets_revision(store: &dyn StateStore) {
        let alice = ExtensionId::new("io.github.alice.delete").unwrap();
        let bob = ExtensionId::new("io.github.bob.delete").unwrap();
        store.write(&alice, "a1", StateValue::Int(1)).unwrap();
        store.write(&alice, "a2", StateValue::Int(2)).unwrap();
        store.write(&bob, "b1", StateValue::Int(3)).unwrap();

        store.delete_all(&alice).unwrap();

        let a1 = store.read(&alice, "a1").unwrap();
        assert_eq!(a1.value, None);
        assert_eq!(
            a1.revision, 0,
            "Delete policy must reset the next revision to 0"
        );
        let a2 = store.read(&alice, "a2").unwrap();
        assert_eq!(a2.value, None);
        let b1 = store.read(&bob, "b1").unwrap();
        assert_eq!(
            b1.value,
            Some(StateValue::Int(3)),
            "sibling namespace must survive"
        );
        assert_eq!(b1.revision, 1);
    }

    pub(crate) fn concurrent_writes_preserve_committed_revisions(store: Arc<dyn StateStore>) {
        let id = ExtensionId::new("io.github.test.concurrent").unwrap();
        let workers = 8;
        let barrier = Arc::new(std::sync::Barrier::new(workers));
        let mut handles = Vec::new();
        for worker in 0..workers {
            let store = store.clone();
            let barrier = barrier.clone();
            let id = id.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                store
                    .write(
                        &id,
                        &format!("key-{worker}"),
                        StateValue::Int(worker as i64),
                    )
                    .expect("concurrent state write must commit")
            }));
        }
        let mutations: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().expect("writer thread must not panic"))
            .collect();
        assert!(mutations.iter().all(|mutation| mutation.changed));
        assert_eq!(
            store.read(&id, "key-0").unwrap().revision,
            workers as u64,
            "all committed writes must be reflected in the durable revision"
        );
        for worker in 0..workers {
            assert_eq!(
                store.read(&id, &format!("key-{worker}")).unwrap().value,
                Some(StateValue::Int(worker as i64))
            );
        }
    }
}

#[cfg(test)]
mod fake_contract_tests {
    use super::contract_tests as contract;
    use super::*;
    use std::sync::Arc;

    fn corrupt_injector(store: &FakeStateStore) -> impl Fn(&ExtensionId) {
        move |extension_id| {
            let mut inner = store.inner.lock().unwrap();
            let location = (extension_id.to_string(), "good".to_owned());
            let record = inner.values.get_mut(&location).unwrap();
            record[0] = 0x06;
        }
    }

    #[test]
    fn isolation() {
        let store = FakeStateStore::new();
        contract::isolation(&store);
    }

    #[test]
    fn key_limits() {
        let store = FakeStateStore::new();
        contract::key_limits(&store);
    }

    #[test]
    fn revision_semantics() {
        let store = FakeStateStore::new();
        contract::revision_semantics(&store);
    }

    #[test]
    fn value_round_trips() {
        let store = FakeStateStore::new();
        contract::value_round_trips(&store);
    }

    #[test]
    fn quota_key_count_and_atomicity() {
        let store = FakeStateStore::new();
        contract::quota_key_count_and_atomicity(&store);
    }

    #[test]
    fn quota_value_size_boundary() {
        let store = FakeStateStore::new();
        contract::quota_value_size_boundary(&store);
    }

    #[test]
    fn quota_byte_budget_and_replacement_delta() {
        let store = FakeStateStore::new();
        contract::quota_byte_budget_and_replacement_delta(&store);
    }

    #[test]
    fn corrupt_records_fail_closed() {
        let store = FakeStateStore::new();
        contract::corrupt_records_fail_closed(&store, corrupt_injector(&store));
    }

    #[test]
    fn delete_all_is_namespace_exact_and_resets_revision() {
        let store = FakeStateStore::new();
        contract::delete_all_is_namespace_exact_and_resets_revision(&store);
    }

    #[test]
    fn concurrent_writes_preserve_committed_revisions() {
        contract::concurrent_writes_preserve_committed_revisions(Arc::new(FakeStateStore::new()));
    }
}

#[cfg(test)]
mod heed_contract_tests {
    use super::contract_tests as contract;
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn open_store() -> (TempDir, HeedStateStore) {
        let dir = TempDir::new().expect("temp dir");
        let store = HeedStateStore::open(dir.path()).expect("store must open");
        (dir, store)
    }

    #[test]
    fn isolation() {
        let (_dir, store) = open_store();
        contract::isolation(&store);
    }

    #[test]
    fn key_limits() {
        let (_dir, store) = open_store();
        contract::key_limits(&store);
    }

    #[test]
    fn revision_semantics() {
        let (_dir, store) = open_store();
        contract::revision_semantics(&store);
    }

    #[test]
    fn value_round_trips() {
        let (_dir, store) = open_store();
        contract::value_round_trips(&store);
    }

    #[test]
    fn quota_key_count_and_atomicity() {
        let (_dir, store) = open_store();
        contract::quota_key_count_and_atomicity(&store);
    }

    #[test]
    fn quota_value_size_boundary() {
        let (_dir, store) = open_store();
        contract::quota_value_size_boundary(&store);
    }

    #[test]
    fn quota_byte_budget_and_replacement_delta() {
        let (_dir, store) = open_store();
        contract::quota_byte_budget_and_replacement_delta(&store);
    }

    #[test]
    fn corrupt_records_fail_closed() {
        let (_dir, store) = open_store();
        contract::corrupt_records_fail_closed(&store, |extension_id| {
            store.inject_raw_record(extension_id, "good", &[0x06, 0x01, 0x02]);
        });
    }

    #[test]
    fn delete_all_is_namespace_exact_and_resets_revision() {
        let (_dir, store) = open_store();
        contract::delete_all_is_namespace_exact_and_resets_revision(&store);
    }

    #[test]
    fn concurrent_writes_preserve_committed_revisions() {
        let (_dir, store) = open_store();
        contract::concurrent_writes_preserve_committed_revisions(Arc::new(store));
    }

    #[test]
    fn state_survives_store_reopen() {
        let dir = TempDir::new().expect("temp dir");
        let id = ExtensionId::new("io.github.test.reopen").unwrap();
        {
            let store = HeedStateStore::open(dir.path()).unwrap();
            store
                .write(&id, "k", StateValue::Text("durable".into()))
                .unwrap();
            store.write(&id, "k2", StateValue::Int(9)).unwrap();
            let snapshot = store.read(&id, "k").unwrap();
            assert_eq!(snapshot.revision, 2);
        }
        {
            let store = HeedStateStore::open(dir.path()).unwrap();
            let snapshot = store.read(&id, "k").unwrap();
            assert_eq!(snapshot.value, Some(StateValue::Text("durable".into())));
            assert_eq!(snapshot.revision, 2);
            let snapshot = store.read(&id, "k2").unwrap();
            assert_eq!(snapshot.value, Some(StateValue::Int(9)));
        }
    }

    #[test]
    fn open_fails_closed_on_unusable_directory() {
        let dir = TempDir::new().expect("temp dir");
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, "a plain file").unwrap();
        let error = HeedStateStore::open(&blocked).unwrap_err();
        assert!(matches!(error, StateStoreError::BackendUnavailable(_)));
    }

    #[cfg(unix)]
    #[test]
    fn private_permissions_on_open() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().expect("temp dir");
        let _store = HeedStateStore::open(dir.path()).unwrap();
        assert_eq!(
            std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for name in ["data.mdb", "lock.mdb", "state.lock"] {
            let path = dir.path().join(name);
            if path.exists() {
                assert_eq!(
                    std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o600,
                    "{name} must be private"
                );
            }
        }
    }
}
