pub mod adapter;
pub mod catalog;
pub mod circuit_breaker;
pub mod cli;
pub mod effects;
pub mod events;
pub mod manifest;
pub mod view;
pub mod wasm;

pub use adapter::{
    DispatchResult, ExtensionHost, ExtensionRuntime, GuestExtension, HostError, InMemoryRuntime,
    RuntimeBudget, RuntimeError, RuntimeFailureKind,
};
pub use catalog::{
    CURRENT_SHILPO_VERSION, CatalogError, CatalogExtension, CatalogPaths, ExtensionCatalog,
    ExtensionCatalogSnapshot, ExtensionUpdate, InstallationReceipt, InstalledExtension,
    InstalledVersionReceipt, PackageSignature, RegistryIndex, RegistryRelease, RegistrySource,
    ReleaseChannel, SignedRegistryIndex, StoredGrants, TrustState, UpdateState,
    default_extension_config_dir, default_extension_data_dir, generate_signing_key,
    package_signature_path, sign_package, sign_registry_index, sign_release,
};
pub use circuit_breaker::{CircuitBreaker, DiagnosticCode, DiagnosticLevel, ExtensionDiagnostic};
pub use cli::{
    DevelopmentRegistration, ExtensionCli, ExtensionCliResult, default_extension_state_dir,
    development_registrations,
};
pub use effects::{
    AuthorizedHostEffect, AuthorizedHostEffectKind, AuthorizedHttpRequest, HostEffect,
    WallpaperSource,
};
pub use events::{EventKind, ExtensionEvent};
pub use manifest::{
    CanonicalId, Capability, CapabilityKind, ContributionId, ExtensionId, ExtensionManifest,
    ManifestError,
};
pub use view::{
    ContainerDirection, ContainerNode, LoadingIndicatorNode, SemanticColorToken, TextNode,
    ViewLimits, ViewNode, ViewStyle, ViewTree, ViewValidationError,
};
pub use wasm::{WasmModule, WasmRuntime};

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tar::Archive;

    const MANIFEST: &str = r#"
        id = "io.github.alice.world-clock"
        name = "World Clock"
        version = "1.0.0"

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

    const VALID_COMPONENT: &str = r#"
        (component
          (core module $module
            (memory (export "memory") 1)
            (global $heap (mut i32) (i32.const 4096))
            (func (export "cabi_realloc")
              (param i32 i32 i32 i32) (result i32)
              global.get $heap
              global.get $heap
              local.get 3
              i32.add
              global.set $heap)
            (data (i32.const 0) "[]")
            (data (i32.const 16) "null")
            (func (export "on-event") (param i32 i32) (result i32)
              i32.const 64
              i32.const 0
              i32.store
              i32.const 64
              i32.const 2
              i32.store offset=4
              i32.const 64)
            (func (export "view") (param i32 i32) (result i32)
              i32.const 72
              i32.const 16
              i32.store
              i32.const 72
              i32.const 4
              i32.store offset=4
              i32.const 72))
          (core instance $instance (instantiate $module))
          (alias core export $instance "memory" (core memory $memory))
          (alias core export $instance "cabi_realloc" (core func $realloc))
          (alias core export $instance "on-event" (core func $on-event-core))
          (alias core export $instance "view" (core func $view-core))
          (type $on-event-type
            (func (param "event-json" string) (result string)))
          (type $view-type
            (func (param "contribution-id" string) (result string)))
          (func $on-event (type $on-event-type)
            (canon lift
              (core func $on-event-core)
              (memory $memory)
              (realloc $realloc)))
          (func $view (type $view-type)
            (canon lift
              (core func $view-core)
              (memory $memory)
              (realloc $realloc)))
          (export "on-event" (func $on-event))
          (export "view" (func $view)))
    "#;

    const RUNAWAY_COMPONENT: &str = r#"
        (component
          (core module $module
            (memory (export "memory") 1)
            (global $heap (mut i32) (i32.const 4096))
            (func (export "cabi_realloc")
              (param i32 i32 i32 i32) (result i32)
              global.get $heap
              global.get $heap
              local.get 3
              i32.add
              global.set $heap)
            (data (i32.const 16) "null")
            (func (export "on-event") (param i32 i32) (result i32)
              (loop $forever
                br $forever)
              unreachable)
            (func (export "view") (param i32 i32) (result i32)
              i32.const 72
              i32.const 16
              i32.store
              i32.const 72
              i32.const 4
              i32.store offset=4
              i32.const 72))
          (core instance $instance (instantiate $module))
          (alias core export $instance "memory" (core memory $memory))
          (alias core export $instance "cabi_realloc" (core func $realloc))
          (alias core export $instance "on-event" (core func $on-event-core))
          (alias core export $instance "view" (core func $view-core))
          (type $on-event-type
            (func (param "event-json" string) (result string)))
          (type $view-type
            (func (param "contribution-id" string) (result string)))
          (func $on-event (type $on-event-type)
            (canon lift
              (core func $on-event-core)
              (memory $memory)
              (realloc $realloc)))
          (func $view (type $view-type)
            (canon lift
              (core func $view-core)
              (memory $memory)
              (realloc $realloc)))
          (export "on-event" (func $on-event))
          (export "view" (func $view)))
    "#;

    struct ClockGuest;

    impl GuestExtension for ClockGuest {
        fn on_event(&mut self, _: &ExtensionEvent) -> Vec<HostEffect> {
            vec![
                HostEffect::ShowNotification {
                    title: "Updated".into(),
                    body: "Clock updated".into(),
                    icon: None,
                },
                HostEffect::HttpRequest {
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
            AuthorizedHostEffectKind::NonHttp(HostEffect::ShowNotification { .. })
        ));
        assert_eq!(result.rejected.len(), 1);
        assert!(matches!(result.rejected[0], HostEffect::HttpRequest { .. }));
        assert_eq!(
            host.diagnostics().last().map(|diagnostic| diagnostic.code),
            Some(DiagnosticCode::CapabilityDenied)
        );
    }

    struct SingleEffectGuest(HostEffect);

    impl GuestExtension for SingleEffectGuest {
        fn on_event(&mut self, _: &ExtensionEvent) -> Vec<HostEffect> {
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
            Box::new(SingleEffectGuest(HostEffect::HttpRequest {
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
        if let AuthorizedHostEffectKind::HttpRequest(req) = result.accepted[0].kind() {
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
                Box::new(SingleEffectGuest(HostEffect::HttpRequest {
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
                Box::new(SingleEffectGuest(HostEffect::HttpRequest {
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
            HostEffect::HttpRequest {
                request_id: "1".into(),
                url: "http://api.example.com/clock/current".into(),
                method: "GET".into(),
            },
            // non-GET method
            HostEffect::HttpRequest {
                request_id: "2".into(),
                url: "https://api.example.com/clock/current".into(),
                method: "POST".into(),
            },
            // embedded credentials
            HostEffect::HttpRequest {
                request_id: "3".into(),
                url: "https://user:pass@api.example.com/clock/current".into(),
                method: "GET".into(),
            },
            // fragments
            HostEffect::HttpRequest {
                request_id: "4".into(),
                url: "https://api.example.com/clock/current#section".into(),
                method: "GET".into(),
            },
            // relative/schemeless
            HostEffect::HttpRequest {
                request_id: "5".into(),
                url: "/clock/current".into(),
                method: "GET".into(),
            },
            // missing host
            HostEffect::HttpRequest {
                request_id: "6".into(),
                url: "https:///clock/current".into(),
                method: "GET".into(),
            },
        ];

        for effect in bad_effects {
            let mut host = ExtensionHost::<InMemoryRuntime>::default();
            host.register(
                manifest.clone(),
                Box::new(SingleEffectGuest(effect)),
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
                HostEffect::HttpRequest {
                    request_id: "diff1".into(),
                    url: "https://api.example.com@evil.example/clock/current".into(),
                    method: "GET".into(),
                },
                false, // should be rejected since evil.example is not granted
            ),
            // explicit default port
            (
                HostEffect::HttpRequest {
                    request_id: "diff2".into(),
                    url: "https://api.example.com:443/clock/current".into(),
                    method: "GET".into(),
                },
                true, // allowed matching api.example.com host
            ),
            // percent-encoded path
            (
                HostEffect::HttpRequest {
                    request_id: "diff3".into(),
                    url: "https://api.example.com/%63lock/current".into(),
                    method: "GET".into(),
                },
                false, // Url::path() /%63lock/current does not match /clock/* without secondary decoding
            ),
            // query string included
            (
                HostEffect::HttpRequest {
                    request_id: "diff4".into(),
                    url: "https://api.example.com/clock/current?foo=bar".into(),
                    method: "GET".into(),
                },
                true, // query string excluded from path pattern matching
            ),
            // backslash in authority/path
            (
                HostEffect::HttpRequest {
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
                Box::new(SingleEffectGuest(effect.clone())),
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
        let unsafe_view = ViewTree::new(ViewNode::Image(crate::view::ImageNode {
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
        assert!(capability.allows_effect(&HostEffect::ReadFile {
            path: "assets/icons/clock.svg".into(),
        }));
        assert!(!capability.allows_effect(&HostEffect::ReadFile {
            path: "data/private.json".into(),
        }));

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
        assert!(capability.allows_effect(&HostEffect::LocationRead));
        assert!(!capability.allows_effect(&HostEffect::ClipboardRead));
    }

    #[test]
    fn schema_generation_describes_the_public_manifest_contract() {
        let schema = ExtensionManifest::schema_json().unwrap();
        assert!(schema.contains("\"ExtensionManifest\""));
        assert!(schema.contains("\"events:subscribe\""));
        assert!(schema.contains("\"network:http\""));

        let generated: serde_json::Value = serde_json::from_str(&schema).unwrap();
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../schema/extension-v1.schema.json")).unwrap();
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
        let error = WasmRuntime::validate_module(b"(component)").unwrap_err();
        assert_eq!(error.kind(), RuntimeFailureKind::Load);
        assert!(
            error
                .message()
                .contains("missing required component export")
        );

        let id = ExtensionId::new("io.github.test.wasm").unwrap();
        let mut runtime = WasmRuntime::new().unwrap();
        runtime
            .load(
                &id,
                WasmModule::from_bytes(VALID_COMPONENT.as_bytes()),
                RuntimeBudget::default(),
            )
            .unwrap();

        for _ in 0..2 {
            assert_eq!(
                runtime
                    .dispatch(&id, &ExtensionEvent::ShellStarted, RuntimeBudget::default(),)
                    .unwrap(),
                Vec::<HostEffect>::new()
            );
            assert_eq!(
                runtime.view(&id, "bar", RuntimeBudget::default()).unwrap(),
                None
            );
        }

        let replacement_error = runtime
            .replace(
                &id,
                WasmModule::from_bytes(b"(component)"),
                RuntimeBudget::default(),
            )
            .unwrap_err();
        assert_eq!(replacement_error.kind(), RuntimeFailureKind::Load);
        assert_eq!(
            runtime
                .dispatch(&id, &ExtensionEvent::ShellStarted, RuntimeBudget::default())
                .unwrap(),
            Vec::<HostEffect>::new()
        );

        let invalid_id = ExtensionId::new("io.github.test.invalid-output").unwrap();
        let invalid_component = VALID_COMPONENT.replacen(
            "(data (i32.const 0) \"[]\")",
            "(data (i32.const 0) \"!!\")",
            1,
        );
        runtime
            .load(
                &invalid_id,
                WasmModule::from_bytes(invalid_component.into_bytes()),
                RuntimeBudget::default(),
            )
            .unwrap();
        let error = runtime
            .dispatch(
                &invalid_id,
                &ExtensionEvent::ShellStarted,
                RuntimeBudget::default(),
            )
            .unwrap_err();
        assert_eq!(error.kind(), RuntimeFailureKind::InvalidOutput);
    }

    #[test]
    fn wasm_adapter_enforces_fuel_and_deadline_budgets() {
        let id = ExtensionId::new("io.github.test.runaway").unwrap();
        let mut runtime = WasmRuntime::new().unwrap();
        let budget = RuntimeBudget {
            fuel: 1_000,
            deadline: Duration::from_secs(1),
            ..RuntimeBudget::default()
        };
        runtime
            .load(
                &id,
                WasmModule::from_bytes(RUNAWAY_COMPONENT.as_bytes()),
                budget,
            )
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
                WasmModule::from_bytes(RUNAWAY_COMPONENT.as_bytes()),
                timeout_budget,
            )
            .unwrap();
        let error = runtime
            .dispatch(&timeout_id, &ExtensionEvent::ShellStarted, timeout_budget)
            .unwrap_err();
        assert_eq!(error.kind(), RuntimeFailureKind::Timeout);

        let memory_id = ExtensionId::new("io.github.test.memory").unwrap();
        let memory_budget = RuntimeBudget {
            max_memory_bytes: 64 * 1024,
            ..RuntimeBudget::default()
        };
        let oversized_component = VALID_COMPONENT.replacen(
            "(memory (export \"memory\") 1)",
            "(memory (export \"memory\") 2)",
            1,
        );
        let error = runtime
            .load(
                &memory_id,
                WasmModule::from_bytes(oversized_component.into_bytes()),
                memory_budget,
            )
            .unwrap_err();
        assert_eq!(error.kind(), RuntimeFailureKind::MemoryLimit);
    }

    struct FailingRuntime;

    impl ExtensionRuntime for FailingRuntime {
        type Module = ();

        fn load(
            &mut self,
            _: &ExtensionId,
            _: Self::Module,
            _: RuntimeBudget,
        ) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn replace(
            &mut self,
            _: &ExtensionId,
            _: Self::Module,
            _: RuntimeBudget,
        ) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn unload(&mut self, _: &ExtensionId) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn dispatch(
            &mut self,
            _: &ExtensionId,
            _: &ExtensionEvent,
            _: RuntimeBudget,
        ) -> Result<Vec<HostEffect>, RuntimeError> {
            Err(RuntimeError::with_kind(
                RuntimeFailureKind::Trap,
                "guest trapped",
            ))
        }

        fn view(
            &mut self,
            _: &ExtensionId,
            _: &str,
            _: RuntimeBudget,
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

            [library]
            path = "extension.wasm"

            [[contributions.settings_pages]]
            id = "settings"
            name = "Settings"
            schema = "settings.schema.json"
        "#;
        fs::write(extension_dir.join("extension.toml"), manifest_content).unwrap();
        fs::write(extension_dir.join("extension.wasm"), VALID_COMPONENT).unwrap();
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
