use shilpo_ext_sdk::prelude::*;

#[derive(Default)]
struct TestLifecycleExtension {
    activated: bool,
    deactivated: bool,
    last_event: Option<String>,
    should_fail: bool,
}

impl Extension for TestLifecycleExtension {
    fn activate(&mut self, _activation: Activation) -> Result<(), Error> {
        if self.should_fail {
            return Err(Error {
                kind: ErrorKind::Unauthorized,
                message: "activation unauthorized".into(),
            });
        }
        self.activated = true;
        Ok(())
    }

    fn deactivate(&mut self, _reason: DeactivateReason) -> Result<(), Error> {
        self.deactivated = true;
        Ok(())
    }

    fn on_event(&mut self, event: ExtensionEvent) -> Result<(), Error> {
        if let ExtensionEvent::TimerFired(t) = event {
            self.last_event = Some(t.name);
        }
        Ok(())
    }

    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
        if contribution_id == "widget" {
            Ok(Some(view! {
                row {
                    text("Widget content"),
                }
            }))
        } else {
            Ok(None)
        }
    }
}

#[test]
fn test_extension_lifecycle_and_error_propagation() {
    let mut ext = TestLifecycleExtension::default();
    let dummy_activation = Activation {
        id: "act-1".into(),
        origin: shilpo_ext_sdk::bindings::shilpo::extension::types::ActivationOrigin::ShellStartup,
        extension_id: "org.example.test".into(),
        contribution_id: None,
        input: None,
        caller: None,
        deadline: None,
    };

    // 1. Successful activation
    assert!(ext.activate(dummy_activation.clone()).is_ok());
    assert!(ext.activated);

    // 2. Event dispatch
    let timer_event = ExtensionEvent::TimerFired(
        shilpo_ext_sdk::bindings::shilpo::extension::events::TimerEvent {
            name: "tick".into(),
        },
    );
    assert!(ext.on_event(timer_event).is_ok());
    assert_eq!(ext.last_event.as_deref(), Some("tick"));

    // 3. View dispatch
    let view_res = ext.view("widget").unwrap();
    assert!(view_res.is_some());
    let missing_res = ext.view("nonexistent").unwrap();
    assert!(missing_res.is_none());

    // 4. Deactivation
    assert!(ext.deactivate(DeactivateReason::UserRequested).is_ok());
    assert!(ext.deactivated);

    // 5. Error propagation without panic
    let mut failing_ext = TestLifecycleExtension {
        should_fail: true,
        ..Default::default()
    };
    let err = failing_ext.activate(dummy_activation).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Unauthorized);
    assert_eq!(err.message, "activation unauthorized");
}

#[test]
fn callback_panics_become_typed_internal_errors() {
    let err = shilpo_ext_sdk::extension::invoke_callback::<()>(|| {
        panic!("private extension detail");
    })
    .expect_err("panic must not cross the SDK callback boundary");
    assert_eq!(err.kind, ErrorKind::Internal);
    assert_eq!(err.message, "extension callback panicked");
}
