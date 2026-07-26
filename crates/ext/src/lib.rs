pub mod adapter;
pub mod effects;
pub mod events;
pub mod manifest;
pub mod view;

pub use adapter::{
    DispatchResult, ExtensionHost, ExtensionRuntime, GuestExtension, HostError, InMemoryRuntime,
    RuntimeError,
};
pub use effects::{HostEffect, WallpaperSource};
pub use events::{EventKind, ExtensionEvent};
pub use manifest::{
    CanonicalId, Capability, CapabilityKind, ContributionId, ExtensionId, ExtensionManifest,
    ManifestError,
};
pub use view::{
    ContainerDirection, ContainerNode, SemanticColorToken, TextNode, ViewLimits, ViewNode,
    ViewStyle, ViewTree, ViewValidationError,
};

#[cfg(test)]
mod tests {
    use super::*;

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
            result.accepted[0],
            HostEffect::ShowNotification { .. }
        ));
        assert_eq!(result.rejected.len(), 1);
        assert!(matches!(result.rejected[0], HostEffect::HttpRequest { .. }));
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
}
