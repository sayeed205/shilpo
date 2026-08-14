use shilpo_ext_sdk::prelude::*;
use shilpo_ext_sdk::state::{reset_fake_store, set_fake_error};

#[test]
fn test_state_operations_and_typed_error_propagation() {
    reset_fake_store();

    // 1. Initial read should be None
    let initial = State::read("user_name").expect("read");
    assert_eq!(initial, None);

    // 2. Write state
    let m1 = State::write("user_name", "Alice").expect("write");
    assert!(m1.changed);
    assert!(m1.revision >= 1);

    // 3. Read state
    let val = State::read("user_name").expect("read").unwrap();
    assert_eq!(val, DataValue::TextValue("Alice".into()));
    assert_eq!(val.as_str(), Some("Alice"));

    // 4. Read snapshot
    let snap = State::read_snapshot("user_name").expect("snapshot");
    assert_eq!(snap.value, Some(DataValue::TextValue("Alice".into())));
    assert_eq!(snap.revision, m1.revision);

    // 5. Watch state
    let watch = State::watch("user_name").expect("watch");
    assert_eq!(watch.watch_id, 1);
    assert_eq!(
        watch.snapshot.value,
        Some(DataValue::TextValue("Alice".into()))
    );

    // 6. Unwatch state
    assert!(State::unwatch(watch.watch_id).is_ok());

    // 7. Delete state
    let del = State::delete("user_name").expect("delete");
    assert!(del.changed);
    assert_eq!(State::read("user_name").expect("read"), None);

    // 8. Typed error propagation
    set_fake_error(Some(Error {
        kind: ErrorKind::BackendUnavailable,
        message: "storage full".into(),
    }));

    let err = State::write("key", 100i64).unwrap_err();
    assert_eq!(err.kind, ErrorKind::BackendUnavailable);
    assert_eq!(err.message, "storage full");

    let read_err = State::read("key").unwrap_err();
    assert_eq!(read_err.kind, ErrorKind::BackendUnavailable);

    // Clean up
    reset_fake_store();
}
