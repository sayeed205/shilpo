pub mod effects;
pub mod events;
pub mod id;
pub mod manifest;
pub mod view;

#[allow(clippy::too_many_arguments)]
pub mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "extension",
    });
}

pub use effects::{HostOperation, WallpaperSource};
pub use events::{EventKind, ExtensionEvent};
pub use id::{CanonicalId, ContributionId, ExtensionId, IdError};
pub use manifest::{
    ActionContribution, BackgroundTaskContribution, BarWidgetContribution, Capability,
    CapabilityKind, Contributions, DesktopWidgetContribution, ExtensionManifest,
    LauncherProviderContribution, LibraryConfig, ManifestError, SUPPORTED_API_VERSION,
    SUPPORTED_SCHEMA_VERSION, SettingsPageContribution, SidePanelContribution, Subscription,
    valid_virtual_path_pattern, wildcard_matches,
};
pub use view::{
    BadgeNode, ButtonNode, ContainerDirection, ContainerNode, IconButtonNode, IconNode, ImageNode,
    ListNode, LoadingIndicatorNode, ProgressNode, SemanticColorToken, SliderNode, SpacerNode,
    TextInputNode, TextNode, ToggleNode, ViewLimits, ViewNode, ViewStyle, ViewTree,
    ViewValidationError,
};

#[cfg(test)]
mod contract_tests {
    use super::*;

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
    "#;

    #[test]
    fn manifest_validation_rejects_invalid_ids_and_duplicate_contributions() {
        let invalid_id =
            MANIFEST.replace("io.github.alice.world-clock", "IO.github.alice.world clock");
        assert!(matches!(
            ExtensionManifest::from_toml(&invalid_id),
            Err(ManifestError::ParseError(_))
        ));

        let duplicate = MANIFEST.replace(
            "[[subscriptions]]",
            "[[contributions.actions]]\nid = \"bar\"\nname = \"Duplicate\"\n\n[[subscriptions]]",
        );
        assert!(matches!(
            ExtensionManifest::from_toml(&duplicate),
            Err(ManifestError::Validation(message)) if message.contains("duplicate contribution")
        ));
    }

    #[test]
    fn view_validation_rejects_unsafe_and_overdeep_trees() {
        let unsafe_view = ViewTree::new(ViewNode::Image(ImageNode {
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
    fn manifest_schema_fixture_matches_the_contract_owner() {
        let schema = serde_json::from_str::<serde_json::Value>(
            &ExtensionManifest::schema_json().expect("manifest schema should serialize"),
        )
        .expect("generated schema should be valid JSON");
        let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
            "../schema/extension-v1.schema.json"
        ))
        .expect("checked-in schema should be valid JSON");
        assert_eq!(schema, fixture);
    }

    #[test]
    fn wit_package_resolves_without_errors() {
        let mut resolve = wit_parser::Resolve::default();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("wit");
        let (pkg_id, _) = resolve
            .push_dir(&path)
            .expect("WIT package directory must resolve");
        let pkg = &resolve.packages[pkg_id];
        assert_eq!(pkg.name.namespace, "shilpo");
        assert_eq!(pkg.name.name, "extension");
        assert_eq!(
            pkg.name.version,
            Some(semver::Version::parse("0.1.0").unwrap())
        );
    }
}
