pub mod adapter;
pub mod catalog;
pub mod circuit_breaker;
pub mod cli;
pub mod effects;
pub mod wasm;
pub mod worker;

pub use adapter::{
    DispatchResult, ExtensionHost, ExtensionRuntime, GuestExtension, HostError, InMemoryRuntime,
    RuntimeBudget, RuntimeError, RuntimeFailureKind,
};
pub use catalog::{
    CURRENT_SHILPO_VERSION, CatalogError, CatalogExtension, CatalogPaths, ExtensionCatalog,
    ExtensionCatalogSnapshot, ExtensionUpdate as CatalogExtensionUpdate, InstallationReceipt,
    InstalledExtension, InstalledVersionReceipt, PackageSignature, RegistryIndex, RegistryRelease,
    RegistrySource, ReleaseChannel, SignedRegistryIndex, StoredGrants, TrustState, UpdateState,
    default_extension_config_dir, default_extension_data_dir, generate_signing_key,
    package_signature_path, sign_package, sign_registry_index, sign_release,
};
pub use circuit_breaker::{CircuitBreaker, DiagnosticCode, DiagnosticLevel, ExtensionDiagnostic};
pub use cli::{
    DevelopmentRegistration, ExtensionCli, ExtensionCliResult, default_extension_state_dir,
    development_registrations, follow_log, source_command, write_signing_key,
};
pub use effects::{
    AuthorizedHostOperation, AuthorizedHostOperationKind, AuthorizedHttpRequest,
    CanonicalHttpTarget, capability_allows_http_target, capability_allows_operation,
};
pub use wasm::{WasmModule, WasmRuntime};
pub use worker::{
    ActiveSource, ContributionDescriptor, ContributionInstance, ContributionSurface,
    ExtensionChanges, ExtensionCommand, ExtensionEngine, ExtensionGeneration, ExtensionSession,
    ExtensionSnapshot, ExtensionUpdate, FrameReader, HostGeneration, HostMessage, MAX_FRAME_SIZE,
    MAX_QUEUE_BOUND, PROTOCOL_VERSION, ProcessCodecError, ReplaceableEvent, WorkerMessage,
    WorkerPayload, read_frame, recv_host_message, recv_worker_message,
    recv_worker_message_nonblocking, run_extension_host, send_host_message, send_worker_message,
    write_frame,
};

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use shilpo_ext_api::{
        CanonicalId, Capability, ContainerDirection, ContainerNode, EventKind, ExtensionEvent,
        ExtensionId, ExtensionManifest, HostOperation, ManifestError, TextNode, ViewLimits,
        ViewNode, ViewTree,
    };
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tar::Archive;

    const MANIFEST: &str = r#"
        id = "io.github.alice.world-clock"
        name = "World Clock"
        version = "1.0.0"
        schema_version = 1
        api_version = "0.1.0"

        [[contributions.bar_widgets]]
        id = "bar"
        name = "World Clock"

        [[subscriptions]]
        event = "timer_fired"

        [[capabilities]]
        kind = "events:subscribe"
        events = ["timer_fired"]

        [[capabilities]]
        kind = "notifications:show"

        [[capabilities]]
        kind = "network:http"
        hosts = ["api.example.com"]
        paths = ["/clock/*"]
    "#;

    const VALID_CORE_WAT: &str = r#"
        (module
          (memory (export "memory") 1)
          (global $heap (mut i32) (i32.const 4096))
          (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
            global.get $heap
            global.get $heap
            local.get 3
            i32.add
            global.set $heap)
          (func (export "activate") (param i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i64) (result i32)
            i32.const 0)
          (func (export "deactivate") (param i32) (result i32)
            i32.const 0)
          (func (export "on-event") (param i32 i32 i64 i32 i64 i32 i64 i32 i32 i32 i32) (result i32)
            i32.const 0)
          (func (export "view") (param i32 i32) (result i32)
            i32.const 0))
    "#;

    const RUNAWAY_CORE_WAT: &str = r#"
        (module
          (memory (export "memory") 1)
          (global $heap (mut i32) (i32.const 4096))
          (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
            global.get $heap
            global.get $heap
            local.get 3
            i32.add
            global.set $heap)
          (func (export "activate") (param i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i64) (result i32)
            i32.const 0)
          (func (export "deactivate") (param i32) (result i32)
            i32.const 0)
          (func (export "on-event") (param i32 i32 i64 i32 i64 i32 i64 i32 i32 i32 i32) (result i32)
            (loop $forever
              br $forever)
            unreachable)
          (func (export "view") (param i32 i32) (result i32)
            i32.const 0))
    "#;

    fn test_component_bytes(core_wat: &str) -> Vec<u8> {
        let mut core_wasm = wat::parse_str(core_wat).expect("core WAT must parse");
        let wit_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core/ext-api/wit");
        let mut resolve = wit_parser::Resolve::default();
        let (pkg_id, _) = resolve
            .push_dir(&wit_path)
            .expect("WIT package must resolve");
        let world_id = resolve
            .select_world(&[pkg_id], Some("extension"))
            .expect("world extension must exist");
        let metadata = wit_component::metadata::encode(
            &resolve,
            world_id,
            wit_component::StringEncoding::UTF8,
            None,
        )
        .expect("metadata encode");

        let name = "component-type:extension";
        let mut section = Vec::new();
        section.push(0x00);
        let mut payload = Vec::new();
        payload.push(name.len() as u8);
        payload.extend_from_slice(name.as_bytes());
        payload.extend_from_slice(&metadata);
        let mut len = payload.len();
        while len >= 0x80 {
            section.push(len as u8 | 0x80);
            len >>= 7;
        }
        section.push(len as u8);
        section.extend(payload);
        core_wasm.extend(section);

        wit_component::ComponentEncoder::default()
            .validate(true)
            .module(&core_wasm)
            .expect("module set must succeed")
            .encode()
            .expect("component encoding must succeed")
    }

    struct ClockGuest;

    impl GuestExtension for ClockGuest {
        fn on_event(&mut self, _: &ExtensionEvent) -> Vec<HostOperation> {
            vec![
                HostOperation::ShowNotification {
                    title: "Updated".into(),
                    body: "Clock updated".into(),
                    icon: None,
                },
                HostOperation::HttpRequest {
                    request_id: "weather".into(),
                    url: "https://evil.example/clock/current".into(),
                    method: "GET".into(),
                },
            ]
        }

        fn view(&self, contribution_id: &str) -> Option<ViewTree> {
            (contribution_id == "bar").then(|| {
                ViewTree::new(ViewNode::Text(TextNode {
                    content: "12:30".into(),
                    font_size: Some(14.0),
                    bold: None,
                    style: None,
                }))
            })
        }
    }

    fn notification_grant() -> Capability {
        Capability::NotificationsShow
    }

    fn event_grant() -> Capability {
        Capability::EventsSubscribe {
            events: vec![EventKind::TimerFired],
        }
    }

    #[test]
    fn host_contract_applies_subscriptions_and_scoped_grants() {
        let manifest = ExtensionManifest::from_toml(MANIFEST).unwrap();
        let extension_id = manifest.id.clone();
        let canonical: CanonicalId = "io.github.alice.world-clock/bar".parse().unwrap();
        let mut host = ExtensionHost::<InMemoryRuntime>::default();
        host.register(
            manifest,
            Box::new(ClockGuest),
            vec![
                event_grant(),
                notification_grant(),
                Capability::NetworkHttp {
                    hosts: vec!["api.example.com".into()],
                    paths: vec!["/clock/*".into()],
                },
            ],
        )
        .unwrap();

        assert!(host.render_view(&canonical).unwrap().is_some());
        let result = host
            .dispatch_event(
                &extension_id,
                &ExtensionEvent::TimerFired {
                    name: "refresh".into(),
                },
            )
            .unwrap();
        assert_eq!(result.accepted.len(), 1);
        assert!(matches!(
            result.accepted[0].kind(),
            AuthorizedHostOperationKind::NonHttp(HostOperation::ShowNotification { .. })
        ));
        assert_eq!(result.rejected.len(), 1);
        assert!(matches!(
            result.rejected[0],
            HostOperation::HttpRequest { .. }
        ));
        assert_eq!(
            host.diagnostics().last().map(|diagnostic| diagnostic.code),
            Some(DiagnosticCode::CapabilityDenied)
        );
    }

    struct SingleOperationGuest(HostOperation);

    impl GuestExtension for SingleOperationGuest {
        fn on_event(&mut self, _: &ExtensionEvent) -> Vec<HostOperation> {
            vec![self.0.clone()]
        }

        fn view(&self, _: &str) -> Option<ViewTree> {
            None
        }
    }

    #[test]
    fn host_http_authorization_uses_canonical_host_and_path() {
        let manifest = ExtensionManifest::from_toml(MANIFEST).unwrap();
        let extension_id = manifest.id.clone();
        let mut host = ExtensionHost::<InMemoryRuntime>::default();
        host.register(
            manifest,
            Box::new(SingleOperationGuest(HostOperation::HttpRequest {
                request_id: "req1".into(),
                url: "HTTPS://API.EXAMPLE.COM/clock/sub/../current".into(),
                method: "GET".into(),
            })),
            vec![Capability::NetworkHttp {
                hosts: vec!["api.example.com".into()],
                paths: vec!["/clock/*".into()],
            }],
        )
        .unwrap();

        let result = host
            .dispatch_event(&extension_id, &ExtensionEvent::ShellStarted)
            .unwrap();
        assert_eq!(result.accepted.len(), 1);
        assert_eq!(result.rejected.len(), 0);
        if let AuthorizedHostOperationKind::HttpRequest(req) = result.accepted[0].kind() {
            assert_eq!(req.request_id(), "req1");
            assert_eq!(req.url().as_str(), "https://api.example.com/clock/current");
            assert_eq!(req.url().host_str(), Some("api.example.com"));
            assert_eq!(req.url().path(), "/clock/current");
        } else {
            panic!("expected AuthorizedHttpRequest");
        }
    }

    #[test]
    fn host_http_authorization_intersects_manifest_and_grant_scopes() {
        let manifest = ExtensionManifest::from_toml(MANIFEST).unwrap();
        let extension_id = manifest.id.clone();

        // 1. Manifest allows /clock/*, but user grant only allows /clock/public/*
        let mut host1 = ExtensionHost::<InMemoryRuntime>::default();
        host1
            .register(
                manifest.clone(),
                Box::new(SingleOperationGuest(HostOperation::HttpRequest {
                    request_id: "req1".into(),
                    url: "https://api.example.com/clock/private/data".into(),
                    method: "GET".into(),
                })),
                vec![Capability::NetworkHttp {
                    hosts: vec!["api.example.com".into()],
                    paths: vec!["/clock/public/*".into()],
                }],
            )
            .unwrap();

        let res1 = host1
            .dispatch_event(&extension_id, &ExtensionEvent::ShellStarted)
            .unwrap();
        assert_eq!(res1.accepted.len(), 0);
        assert_eq!(res1.rejected.len(), 1);

        // 2. Grant allows all hosts, but manifest restricts to api.example.com
        let mut host2 = ExtensionHost::<InMemoryRuntime>::default();
        host2
            .register(
                manifest,
                Box::new(SingleOperationGuest(HostOperation::HttpRequest {
                    request_id: "req2".into(),
                    url: "https://other.example.com/clock/current".into(),
                    method: "GET".into(),
                })),
                vec![Capability::NetworkHttp {
                    hosts: vec!["*".into()],
                    paths: vec!["/clock/*".into()],
                }],
            )
            .unwrap();

        let res2 = host2
            .dispatch_event(&extension_id, &ExtensionEvent::ShellStarted)
            .unwrap();
        assert_eq!(res2.accepted.len(), 0);
        assert_eq!(res2.rejected.len(), 1);
    }

    #[test]
    fn host_http_authorization_rejects_unsupported_request_policy() {
        let manifest = ExtensionManifest::from_toml(MANIFEST).unwrap();
        let extension_id = manifest.id.clone();

        let bad_effects = vec![
            // non-HTTPS
            HostOperation::HttpRequest {
                request_id: "1".into(),
                url: "http://api.example.com/clock/current".into(),
                method: "GET".into(),
            },
            // non-GET method
            HostOperation::HttpRequest {
                request_id: "2".into(),
                url: "https://api.example.com/clock/current".into(),
                method: "POST".into(),
            },
            // embedded credentials
            HostOperation::HttpRequest {
                request_id: "3".into(),
                url: "https://user:pass@api.example.com/clock/current".into(),
                method: "GET".into(),
            },
            // fragments
            HostOperation::HttpRequest {
                request_id: "4".into(),
                url: "https://api.example.com/clock/current#section".into(),
                method: "GET".into(),
            },
            // relative/schemeless
            HostOperation::HttpRequest {
                request_id: "5".into(),
                url: "/clock/current".into(),
                method: "GET".into(),
            },
            // missing host
            HostOperation::HttpRequest {
                request_id: "6".into(),
                url: "https:///clock/current".into(),
                method: "GET".into(),
            },
        ];

        for effect in bad_effects {
            let mut host = ExtensionHost::<InMemoryRuntime>::default();
            host.register(
                manifest.clone(),
                Box::new(SingleOperationGuest(effect)),
                vec![Capability::NetworkHttp {
                    hosts: vec!["api.example.com".into()],
                    paths: vec!["/clock/*".into()],
                }],
            )
            .unwrap();

            let res = host
                .dispatch_event(&extension_id, &ExtensionEvent::ShellStarted)
                .unwrap();
            assert_eq!(
                res.accepted.len(),
                0,
                "expected rejection for unsupported policy"
            );
            assert_eq!(res.rejected.len(), 1);
        }
    }

    #[test]
    fn host_http_authorization_handles_parser_differentials_consistently() {
        let manifest = ExtensionManifest::from_toml(MANIFEST).unwrap();
        let extension_id = manifest.id.clone();

        let differential_effects = vec![
            // authority delimiter spoofing
            (
                HostOperation::HttpRequest {
                    request_id: "diff1".into(),
                    url: "https://api.example.com@evil.example/clock/current".into(),
                    method: "GET".into(),
                },
                false, // should be rejected since evil.example is not granted
            ),
            // explicit default port
            (
                HostOperation::HttpRequest {
                    request_id: "diff2".into(),
                    url: "https://api.example.com:443/clock/current".into(),
                    method: "GET".into(),
                },
                true, // allowed matching api.example.com host
            ),
            // percent-encoded path
            (
                HostOperation::HttpRequest {
                    request_id: "diff3".into(),
                    url: "https://api.example.com/%63lock/current".into(),
                    method: "GET".into(),
                },
                false, // Url::path() /%63lock/current does not match /clock/* without secondary decoding
            ),
            // query string included
            (
                HostOperation::HttpRequest {
                    request_id: "diff4".into(),
                    url: "https://api.example.com/clock/current?foo=bar".into(),
                    method: "GET".into(),
                },
                true, // query string excluded from path pattern matching
            ),
            // backslash in authority/path
            (
                HostOperation::HttpRequest {
                    request_id: "diff5".into(),
                    url: "https://api.example.com\\evil.example/clock/current".into(),
                    method: "GET".into(),
                },
                false, // rejected / invalid target
            ),
        ];

        for (effect, should_accept) in differential_effects {
            let mut host = ExtensionHost::<InMemoryRuntime>::default();
            host.register(
                manifest.clone(),
                Box::new(SingleOperationGuest(effect.clone())),
                vec![Capability::NetworkHttp {
                    hosts: vec!["api.example.com".into()],
                    paths: vec!["/clock/*".into()],
                }],
            )
            .unwrap();

            let res = host
                .dispatch_event(&extension_id, &ExtensionEvent::ShellStarted)
                .unwrap();
            if should_accept {
                assert_eq!(
                    res.accepted.len(),
                    1,
                    "expected acceptance for effect {:?}",
                    effect
                );
                assert_eq!(res.rejected.len(), 0);
            } else {
                assert_eq!(
                    res.accepted.len(),
                    0,
                    "expected rejection for effect {:?}",
                    effect
                );
                assert_eq!(res.rejected.len(), 1);
            }
        }
    }

    #[test]
    fn deserialization_cannot_bypass_id_validation() {
        let invalid =
            MANIFEST.replace("io.github.alice.world-clock", "IO.github.alice.world clock");
        assert!(matches!(
            ExtensionManifest::from_toml(&invalid),
            Err(ManifestError::ParseError(_))
        ));
    }

    #[test]
    fn manifest_rejects_duplicate_contribution_ids_across_surfaces() {
        let invalid = MANIFEST.replace(
            "[[subscriptions]]",
            "[[contributions.actions]]\nid = \"bar\"\nname = \"Duplicate\"\n\n[[subscriptions]]",
        );
        assert!(matches!(
            ExtensionManifest::from_toml(&invalid),
            Err(ManifestError::Validation(message)) if message.contains("duplicate contribution")
        ));
    }

    #[test]
    fn view_validation_rejects_unsafe_or_unbounded_content() {
        let unsafe_view = ViewTree::new(ViewNode::Image(shilpo_ext_api::ImageNode {
            asset_path: "../secret.png".into(),
            width: None,
            height: None,
            style: None,
        }));
        assert!(unsafe_view.validate(ViewLimits::default()).is_err());

        let deep_view = (0..4).fold(ViewNode::Divider, |child, _| {
            ViewNode::Container(ContainerNode {
                direction: ContainerDirection::Column,
                children: vec![child],
                style: None,
                gap: None,
            })
        });
        assert!(
            ViewTree::new(deep_view)
                .validate(ViewLimits {
                    max_depth: 3,
                    ..ViewLimits::default()
                })
                .is_err()
        );
    }

    #[test]
    fn filesystem_capability_enforces_its_virtual_path_scope() {
        let capability = Capability::FilesystemRead {
            paths: vec!["assets/icons/*".into()],
        };
        assert!(
            !capability_allows_operation(
                &capability,
                &HostOperation::ShowNotification {
                    title: "test".into(),
                    body: "test".into(),
                    icon: None,
                }
            )
        );

        let invalid = format!(
            "{MANIFEST}\n[[capabilities]]\nkind = \"filesystem:read\"\npaths = [\"../secrets/**\"]"
        );
        assert!(matches!(
            ExtensionManifest::from_toml(&invalid),
            Err(ManifestError::Validation(message)) if message.contains("invalid scope")
        ));
    }

    #[test]
    fn location_read_capability_allows_location_read_effect() {
        let capability = Capability::LocationRead;
        assert!(capability_allows_operation(
            &capability,
            &HostOperation::LocationRead
        ));
        assert!(!capability_allows_operation(
            &capability,
            &HostOperation::ClipboardWrite { text: "hi".into() }
        ));
    }

    #[test]
    fn schema_generation_describes_the_public_manifest_contract() {
        let schema = ExtensionManifest::schema_json().unwrap();
        assert!(schema.contains("\"ExtensionManifest\""));
        assert!(schema.contains("\"events:subscribe\""));
        assert!(schema.contains("\"network:http\""));

        let generated: serde_json::Value = serde_json::from_str(&schema).unwrap();
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../core/ext-api/schema/extension-v1.schema.json"
        ))
        .unwrap();
        assert_eq!(fixture, generated);
    }

    #[test]
    fn circuit_breaker_trips_after_max_failures() {
        let mut cb = CircuitBreaker::new(2);
        let id = ExtensionId::new("io.github.test.failing").unwrap();

        assert!(!cb.is_tripped(&id));
        cb.record_failure(&id, DiagnosticCode::RuntimeTrap, "first failure");
        assert!(!cb.is_tripped(&id));
        cb.record_success(&id);

        cb.record_failure(&id, DiagnosticCode::RuntimeTrap, "second failure");
        assert!(!cb.is_tripped(&id));
        cb.record_failure(&id, DiagnosticCode::RuntimeTrap, "third failure");
        assert!(cb.is_tripped(&id));

        cb.reset(&id);
        assert!(!cb.is_tripped(&id));
    }

    #[test]
    fn wasm_adapter_executes_the_component_contract_repeatedly() {
        let error = WasmRuntime::validate_module(b"not-a-wasm").unwrap_err();
        assert_eq!(error.kind(), RuntimeFailureKind::Load);

        let valid_component_bytes = test_component_bytes(VALID_CORE_WAT);
        let id = ExtensionId::new("io.github.test.wasm").unwrap();
        let mut runtime = WasmRuntime::new().unwrap();
        runtime
            .load(
                &id,
                WasmModule::from_bytes(valid_component_bytes.clone()),
                RuntimeBudget::default(),
            )
            .unwrap();

        for _ in 0..2 {
            assert_eq!(
                runtime
                    .dispatch(&id, &ExtensionEvent::ShellStarted, RuntimeBudget::default(),)
                    .unwrap(),
                Vec::<HostOperation>::new()
            );
            assert_eq!(
                runtime.view(&id, "bar", RuntimeBudget::default()).unwrap(),
                None
            );
        }

        let replacement_error = runtime
            .replace(
                &id,
                WasmModule::from_bytes(b"not-a-wasm"),
                RuntimeBudget::default(),
            )
            .unwrap_err();
        assert_eq!(replacement_error.kind(), RuntimeFailureKind::Load);
        assert_eq!(
            runtime
                .dispatch(&id, &ExtensionEvent::ShellStarted, RuntimeBudget::default())
                .unwrap(),
            Vec::<HostOperation>::new()
        );
    }

    #[test]
    fn wasm_adapter_enforces_fuel_and_deadline_budgets() {
        let runaway_bytes = test_component_bytes(RUNAWAY_CORE_WAT);
        let id = ExtensionId::new("io.github.test.runaway").unwrap();
        let mut runtime = WasmRuntime::new().unwrap();
        let budget = RuntimeBudget {
            fuel: 1_000,
            deadline: Duration::from_secs(1),
            ..RuntimeBudget::default()
        };
        runtime
            .load(&id, WasmModule::from_bytes(runaway_bytes.clone()), budget)
            .unwrap();
        let error = runtime
            .dispatch(&id, &ExtensionEvent::ShellStarted, budget)
            .unwrap_err();
        assert_eq!(error.kind(), RuntimeFailureKind::FuelExhausted);

        let timeout_id = ExtensionId::new("io.github.test.timeout").unwrap();
        let timeout_budget = RuntimeBudget {
            fuel: u64::MAX,
            deadline: Duration::from_millis(5),
            ..RuntimeBudget::default()
        };
        runtime
            .load(
                &timeout_id,
                WasmModule::from_bytes(runaway_bytes),
                timeout_budget,
            )
            .unwrap();
        let error = runtime
            .dispatch(&timeout_id, &ExtensionEvent::ShellStarted, timeout_budget)
            .unwrap_err();
        assert_eq!(error.kind(), RuntimeFailureKind::Timeout);
    }

    struct FailingRuntime;

    impl ExtensionRuntime for FailingRuntime {
        type Module = ();

        fn load(
            &mut self,
            _extension_id: &ExtensionId,
            _module: Self::Module,
            _budget: RuntimeBudget,
        ) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn replace(
            &mut self,
            _extension_id: &ExtensionId,
            _module: Self::Module,
            _budget: RuntimeBudget,
        ) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn unload(&mut self, _extension_id: &ExtensionId) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn compile_module(&self, _bytes: &[u8]) -> Result<Self::Module, String> {
            Ok(())
        }

        fn dispatch(
            &mut self,
            _extension_id: &ExtensionId,
            _event: &ExtensionEvent,
            _budget: RuntimeBudget,
        ) -> Result<Vec<HostOperation>, RuntimeError> {
            Err(RuntimeError::with_kind(
                RuntimeFailureKind::Trap,
                "forced failure",
            ))
        }

        fn view(
            &mut self,
            _extension_id: &ExtensionId,
            _contribution_id: &str,
            _budget: RuntimeBudget,
        ) -> Result<Option<ViewTree>, RuntimeError> {
            Ok(None)
        }
    }

    #[test]
    fn host_disables_an_extension_after_repeated_runtime_failures() {
        let manifest = ExtensionManifest::from_toml(
            r#"
                id = "io.github.test.failing-host"
                name = "Failing Host"
                version = "1.0.0"
            "#,
        )
        .unwrap();
        let id = manifest.id.clone();
        let mut host = ExtensionHost::new(FailingRuntime).with_failure_threshold(2);
        host.register(manifest, (), vec![]).unwrap();

        assert!(matches!(
            host.dispatch_event(&id, &ExtensionEvent::ShellStarted),
            Err(HostError::Runtime(_))
        ));
        assert!(!host.is_disabled(&id));
        assert!(matches!(
            host.dispatch_event(&id, &ExtensionEvent::ShellStarted),
            Err(HostError::Runtime(_))
        ));
        assert!(host.is_disabled(&id));
        assert!(matches!(
            host.dispatch_event(&id, &ExtensionEvent::ShellStarted),
            Err(HostError::Disabled(disabled)) if disabled == id
        ));
        assert_eq!(
            host.diagnostics().last().map(|diagnostic| diagnostic.code),
            Some(DiagnosticCode::CircuitOpen)
        );
    }

    #[test]
    fn extension_cli_validates_packages_and_persists_dev_reload_state() {
        let temp_dir = make_temp_dir("cli");
        let state_dir = temp_dir.join("state");
        let extension_dir = temp_dir.join("extension");
        fs::create_dir_all(&extension_dir).unwrap();

        let manifest_content = r#"
            id = "io.github.test.cli-sample"
            name = "CLI Sample"
            version = "1.0.0"
            schema_version = 1
            api_version = "0.1.0"

            [library]
            path = "extension.wasm"

            [[contributions.settings_pages]]
            id = "settings"
            name = "Settings"
            schema = "settings.schema.json"
        "#;
        fs::write(extension_dir.join("extension.toml"), manifest_content).unwrap();
        fs::write(
            extension_dir.join("extension.wasm"),
            test_component_bytes(VALID_CORE_WAT),
        )
        .unwrap();
        fs::write(
            extension_dir.join("settings.schema.json"),
            r#"{
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "city": { "type": "string", "default": "Kolkata" }
                }
            }"#,
        )
        .unwrap();
        fs::write(extension_dir.join("README.md"), "# CLI Sample").unwrap();

        let check_res = ExtensionCli::check(&extension_dir);
        assert!(check_res.success);
        assert_eq!(
            check_res.extension_id.as_deref(),
            Some("io.github.test.cli-sample")
        );

        let pack_out = temp_dir.join("dist");
        let pack_res = ExtensionCli::pack(&extension_dir, &pack_out);
        assert!(pack_res.success);
        let artifact = pack_res.artifact.unwrap();
        assert_eq!(
            artifact.file_name().and_then(|name| name.to_str()),
            Some("io.github.test.cli-sample-1.0.0.shilpo-ext")
        );
        let archive = fs::File::open(&artifact).unwrap();
        let mut archive = Archive::new(GzDecoder::new(archive));
        let entries = archive
            .entries()
            .unwrap()
            .map(|entry| entry.unwrap().path().unwrap().into_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            entries,
            BTreeSet::from([
                PathBuf::from("README.md"),
                PathBuf::from("extension.toml"),
                PathBuf::from("extension.wasm"),
                PathBuf::from("settings.schema.json"),
            ])
        );
        let second_pack_out = temp_dir.join("dist-again");
        let second_pack = ExtensionCli::pack(&extension_dir, &second_pack_out);
        assert!(second_pack.success);
        assert_eq!(
            fs::read(&artifact).unwrap(),
            fs::read(second_pack.artifact.unwrap()).unwrap()
        );

        let dev = ExtensionCli::dev(&extension_dir, &state_dir);
        assert!(dev.success);
        let id = ExtensionId::new("io.github.test.cli-sample").unwrap();
        let reload = ExtensionCli::reload(&id, &state_dir);
        assert!(reload.success);
        assert!(
            reload
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.contains("generation 2") })
        );
        let logs = ExtensionCli::logs(&id, &state_dir);
        assert!(logs.success);
        assert_eq!(logs.diagnostics.len(), 2);
        assert_eq!(ExtensionCli::list_dev(&state_dir).len(), 1);

        #[cfg(unix)]
        {
            fs::write(temp_dir.join("outside-license"), "secret").unwrap();
            std::os::unix::fs::symlink(
                temp_dir.join("outside-license"),
                extension_dir.join("LICENSE"),
            )
            .unwrap();
            let result = ExtensionCli::check(&extension_dir);
            assert!(!result.success);
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.starts_with("error[file.type]"))
            );
        }

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn extension_cli_rejects_invalid_settings_and_wasm() {
        let temp_dir = make_temp_dir("invalid-cli");
        fs::write(
            temp_dir.join("extension.toml"),
            r#"
                id = "io.github.test.invalid"
                name = "Invalid"
                version = "1.0.0"

                [library]
                path = "extension.wasm"

                [[contributions.settings_pages]]
                id = "settings"
                name = "Settings"
                schema = "settings.schema.json"
            "#,
        )
        .unwrap();
        fs::write(temp_dir.join("extension.wasm"), b"not wasm").unwrap();
        fs::write(
            temp_dir.join("settings.schema.json"),
            r#"{
                "type": "object",
                "required": ["city"],
                "properties": { "city": { "type": "string" } }
            }"#,
        )
        .unwrap();

        let result = ExtensionCli::check(&temp_dir);
        assert!(!result.success);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.starts_with("error[wasm.invalid]"))
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.starts_with("error[settings.defaults]"))
        );

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    fn make_temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("shilpo-ext-{name}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
