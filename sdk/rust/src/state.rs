//! Typed state helper for the canonical `state` host import.
//!
//! Provides namespaced key-value state storage with atomic watch snapshots.

use crate::bindings::shilpo::extension::state::{StateMutation, StateSnapshot, WatchRegistration};
use crate::bindings::shilpo::extension::types::{DataValue, Error};

/// Typed state helper for key-value storage and reactive watches.
///
/// # Examples
///
/// ```rust
/// use shilpo_ext_sdk::prelude::*;
///
/// // Write state value
/// let mutation = State::write("count", 42i64).expect("write state");
/// assert!(mutation.revision >= 1);
///
/// // Read state value
/// let value = State::read("count").expect("read state");
/// assert_eq!(value, Some(DataValue::IntValue(42)));
///
/// // Delete key
/// let del = State::delete("count").expect("delete state");
/// assert!(del.changed);
/// ```
use crate::data::IntoDataValue;

/// Typed state helper for key-value storage and reactive watches.
pub struct State;

impl State {
    /// Reads the stored value for `key`. Returns `Ok(None)` if the key does not exist.
    pub fn read(key: &str) -> Result<Option<DataValue>, Error> {
        let snapshot = imp::read(key)?;
        match snapshot.value {
            Some(DataValue::None) | None => Ok(None),
            Some(other) => Ok(Some(other)),
        }
    }

    /// Reads the full snapshot (value and monotonic revision) for `key`.
    pub fn read_snapshot(key: &str) -> Result<StateSnapshot, Error> {
        imp::read(key)
    }

    /// Writes `value` for `key`. Returns the state mutation outcome.
    pub fn write(key: &str, value: impl IntoDataValue) -> Result<StateMutation, Error> {
        imp::write(key, &value.into_data_value())
    }

    /// Deletes `key`. Returns the state mutation outcome.
    pub fn delete(key: &str) -> Result<StateMutation, Error> {
        imp::delete(key)
    }

    /// Atomically registers a reactive watch on `key` and returns the registration snapshot.
    pub fn watch(key: &str) -> Result<WatchRegistration, Error> {
        imp::watch(key)
    }

    /// Removes an active watch registration by its runtime watch ID.
    pub fn unwatch(watch_id: u64) -> Result<(), Error> {
        imp::unwatch(watch_id)
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use crate::bindings::shilpo::extension::state as wit_state;

    pub fn read(key: &str) -> Result<StateSnapshot, Error> {
        wit_state::read(key)
    }

    pub fn write(key: &str, value: &DataValue) -> Result<StateMutation, Error> {
        wit_state::write(key, value)
    }

    pub fn delete(key: &str) -> Result<StateMutation, Error> {
        wit_state::delete(key)
    }

    pub fn watch(key: &str) -> Result<WatchRegistration, Error> {
        wit_state::watch(key)
    }

    pub fn unwatch(watch_id: u64) -> Result<(), Error> {
        wit_state::unwatch(watch_id)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    static FAKE_STORE: Mutex<Option<HashMap<String, (DataValue, u64)>>> = Mutex::new(None);
    static REVISION: Mutex<u64> = Mutex::new(1);
    static FAKE_ERROR: Mutex<Option<Error>> = Mutex::new(None);

    #[doc(hidden)]
    pub fn set_fake_error(err: Option<Error>) {
        *FAKE_ERROR.lock().unwrap() = err;
    }

    #[doc(hidden)]
    pub fn reset_fake_store() {
        *FAKE_STORE.lock().unwrap() = Some(HashMap::new());
        *REVISION.lock().unwrap() = 1;
        *FAKE_ERROR.lock().unwrap() = None;
    }

    pub fn read(key: &str) -> Result<StateSnapshot, Error> {
        if let Some(err) = FAKE_ERROR.lock().unwrap().clone() {
            return Err(err);
        }
        let mut guard = FAKE_STORE.lock().unwrap();
        let store = guard.get_or_insert_with(HashMap::new);
        let rev = *REVISION.lock().unwrap();
        match store.get(key) {
            Some((val, val_rev)) => Ok(StateSnapshot {
                value: Some(val.clone()),
                revision: *val_rev,
            }),
            None => Ok(StateSnapshot {
                value: None,
                revision: rev,
            }),
        }
    }

    pub fn write(key: &str, value: &DataValue) -> Result<StateMutation, Error> {
        if let Some(err) = FAKE_ERROR.lock().unwrap().clone() {
            return Err(err);
        }
        let mut guard = FAKE_STORE.lock().unwrap();
        let store = guard.get_or_insert_with(HashMap::new);
        let mut rev_guard = REVISION.lock().unwrap();
        *rev_guard += 1;
        let new_rev = *rev_guard;
        let changed = store
            .insert(key.to_string(), (value.clone(), new_rev))
            .map(|(v, _)| v)
            != Some(value.clone());
        Ok(StateMutation {
            changed,
            revision: new_rev,
        })
    }

    pub fn delete(key: &str) -> Result<StateMutation, Error> {
        if let Some(err) = FAKE_ERROR.lock().unwrap().clone() {
            return Err(err);
        }
        let mut guard = FAKE_STORE.lock().unwrap();
        let store = guard.get_or_insert_with(HashMap::new);
        let mut rev_guard = REVISION.lock().unwrap();
        *rev_guard += 1;
        let new_rev = *rev_guard;
        let changed = store.remove(key).is_some();
        Ok(StateMutation {
            changed,
            revision: new_rev,
        })
    }

    pub fn watch(key: &str) -> Result<WatchRegistration, Error> {
        if let Some(err) = FAKE_ERROR.lock().unwrap().clone() {
            return Err(err);
        }
        let snap = read(key)?;
        Ok(WatchRegistration {
            watch_id: 1,
            snapshot: snap,
        })
    }

    pub fn unwatch(_watch_id: u64) -> Result<(), Error> {
        if let Some(err) = FAKE_ERROR.lock().unwrap().clone() {
            return Err(err);
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use imp::{reset_fake_store, set_fake_error};
