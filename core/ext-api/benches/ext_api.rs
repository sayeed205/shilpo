//! Benchmarks for the cross-platform extension contract.
//!
//! Two hot paths dominate this crate: manifest parsing and validation, which
//! runs for every extension at discovery/install time, and view-tree decoding
//! plus validation, which runs on every frame an extension pushes to the shell.

use shilpo_ext_api::{
    Alignment, BadgeNode, ButtonNode, Capability, ContainerDirection, ContainerNode,
    ContributionId, EventKind, ExtensionId, ExtensionManifest, IconNode, ListNode, ProgressNode,
    SemanticColorToken, SliderNode, SpacerNode, TextNode, ToggleNode, ViewLimits, ViewNode,
    ViewStyle, ViewTree, valid_virtual_path_pattern, wildcard_matches,
};

fn main() {
    divan::main();
}

/// The canonical showcase manifest: every contribution family and a
/// least-privilege capability set.
const SHOWCASE_MANIFEST: &str = r#"
id = "org.shilpo.example"
name = "Shilpo Showcase Extension"
version = "0.1.0"
schema_version = 1
api_version = "0.1.0"
min_shilpo_version = "0.1.0"
authors = ["Shilpo Team <team@shilpo.org>"]
description = "Canonical showcase extension demonstrating all Shilpo contribution families."
repository = "https://github.com/sayeed205/shilpo"
license = "MIT"

[library]
path = "extension.wasm"

[[contributions.bar_widgets]]
id = "status-bar"
name = "Showcase Status Bar"
description = "Compact bar widget showing extension status and quick actions."

[[contributions.bar_menus]]
id = "status-menu"
name = "Showcase Status Menu"
bar_widget = "status-bar"

[[contributions.desktop_widgets]]
id = "system-card"
name = "Showcase Desktop Card"
default_width = 320
default_height = 240
min_width = 200
min_height = 150

[[contributions.settings_pages]]
id = "preferences"
name = "Showcase Preferences"
schema = "settings.schema.json"

[[contributions.side_panels]]
id = "side-panel"
name = "Showcase Side Panel"

[[contributions.search_providers]]
id = "search-commands"
name = "Showcase Search"

[[contributions.actions]]
id = "toggle-power"
name = "Toggle Showcase Mode"

[[contributions.keyboard_shortcuts]]
id = "shortcut-toggle"
name = "Toggle Mode Shortcut"
action = "toggle-power"
default_binding = "Super+Shift+S"

[[contributions.background_tasks]]
id = "sync-task"
name = "Showcase Sync Task"

[[contributions.wallpaper_providers]]
id = "solid-wallpapers"
name = "Showcase Wallpapers"
modes = ["manual", "slideshow"]
targets = ["global", "workspace"]

[[subscriptions]]
event = "palette_generated"

[[subscriptions]]
event = "wallpaper_changed"

[[capabilities]]
kind = "events:subscribe"
events = ["palette_generated", "wallpaper_changed"]

[[capabilities]]
kind = "theme:read"

[[capabilities]]
kind = "notifications:show"

[[capabilities]]
kind = "clipboard:read"

[[capabilities]]
kind = "wallpaper:read"

[[capabilities]]
kind = "actions:invoke"
actions = ["toggle-power"]

[[capabilities]]
kind = "filesystem:read"
paths = ["assets/**", "data/cache"]

[[capabilities]]
kind = "network:http"
hosts = ["api.example.com"]
paths = ["/showcase"]
"#;

/// The smallest manifest the contract accepts, for the fixed parsing cost.
const MINIMAL_MANIFEST: &str = r#"
id = "io.github.alice.world-clock"
name = "World Clock"
version = "1.0.0"
schema_version = 1
api_version = "0.1.0"

[[contributions.bar_widgets]]
id = "bar"
name = "World Clock"
"#;

mod manifest {
    use super::*;

    /// Discovery cost for one extension: TOML decode plus the full validation
    /// pass (ID formats, duplicate contributions, capability scopes).
    #[divan::bench]
    fn parse_and_validate_showcase() -> ExtensionManifest {
        ExtensionManifest::from_toml(divan::black_box(SHOWCASE_MANIFEST)).unwrap()
    }

    #[divan::bench]
    fn parse_and_validate_minimal() -> ExtensionManifest {
        ExtensionManifest::from_toml(divan::black_box(MINIMAL_MANIFEST)).unwrap()
    }

    /// Validation alone, isolated from the TOML decode.
    #[divan::bench]
    fn validate_only(bencher: divan::Bencher) {
        let manifest = ExtensionManifest::from_toml(SHOWCASE_MANIFEST).unwrap();
        bencher.bench(|| divan::black_box(&manifest).validate().unwrap());
    }

    /// The JSON Schema published for manifest authoring tools.
    #[divan::bench]
    fn schema_json() -> String {
        ExtensionManifest::schema_json().unwrap()
    }

    /// Capability lookup, hit once per delivered event.
    #[divan::bench]
    fn capability_allows_event(bencher: divan::Bencher) {
        let manifest = ExtensionManifest::from_toml(SHOWCASE_MANIFEST).unwrap();
        let capabilities: &[Capability] = &manifest.capabilities;
        bencher.bench(|| {
            divan::black_box(capabilities)
                .iter()
                .any(|capability| capability.allows_event(EventKind::WallpaperChanged))
        });
    }
}

mod ids {
    use super::*;

    #[divan::bench]
    fn extension_id() -> ExtensionId {
        ExtensionId::new(divan::black_box("io.github.alice.world-clock")).unwrap()
    }

    #[divan::bench]
    fn contribution_id() -> ContributionId {
        ContributionId::new(divan::black_box("status-bar")).unwrap()
    }

    #[divan::bench]
    fn canonical_id_from_str() -> shilpo_ext_api::CanonicalId {
        divan::black_box("io.github.alice.world-clock/status-bar")
            .parse()
            .unwrap()
    }
}

mod sandbox {
    use super::*;

    /// Host/path wildcard matching, evaluated on every guarded host call.
    #[divan::bench(args = ["assets/icons/weather-sun.png", "data/cache/2026/08/06/report.json"])]
    fn wildcard(bencher: divan::Bencher, value: &str) {
        bencher.bench(|| wildcard_matches(divan::black_box("assets/**"), divan::black_box(value)));
    }

    #[divan::bench(args = ["assets/icons/sun.png", "user/state/db", "../escape/passwd"])]
    fn virtual_path_pattern(bencher: divan::Bencher, value: &str) {
        bencher.bench(|| valid_virtual_path_pattern(divan::black_box(value)));
    }
}

mod view {
    use super::*;

    /// A realistic bar-menu card: nested containers, a list of rows, and the
    /// interactive nodes an extension typically ships.
    fn menu_card(rows: usize) -> ViewTree {
        let mut items = Vec::with_capacity(rows);
        for index in 0..rows {
            items.push(ViewNode::Container(ContainerNode {
                direction: ContainerDirection::Row,
                children: vec![
                    ViewNode::Icon(IconNode {
                        name: "weather-clear".to_owned(),
                        size: Some(16.0),
                        style: None,
                    }),
                    ViewNode::Text(TextNode {
                        content: format!("Row {index} — status nominal"),
                        font_size: Some(13.0),
                        bold: Some(false),
                        style: Some(ViewStyle {
                            color: Some(SemanticColorToken::OnSurfaceVariant),
                            ..ViewStyle::default()
                        }),
                    }),
                    ViewNode::Spacer(SpacerNode { size: Some(8.0) }),
                    ViewNode::Badge(BadgeNode {
                        label: format!("{index}"),
                        style: None,
                    }),
                    ViewNode::Toggle(ToggleNode {
                        value: index % 2 == 0,
                        event_id: format!("row-{index}-toggle"),
                        style: None,
                    }),
                ],
                style: None,
                gap: Some(6.0),
                align_items: Some(Alignment::Center),
                justify_content: None,
                wrap: false,
                event_id: Some(format!("row-{index}")),
            }));
        }

        ViewTree::new(ViewNode::Container(ContainerNode {
            direction: ContainerDirection::Column,
            children: vec![
                ViewNode::Text(TextNode {
                    content: "Showcase Status Menu".to_owned(),
                    font_size: Some(16.0),
                    bold: Some(true),
                    style: None,
                }),
                ViewNode::Divider,
                ViewNode::List(ListNode { items, style: None }),
                ViewNode::Progress(ProgressNode {
                    value: 0.42,
                    style: None,
                }),
                ViewNode::Slider(SliderNode {
                    value: 0.5,
                    min: 0.0,
                    max: 1.0,
                    event_id: "brightness".to_owned(),
                    style: None,
                }),
                ViewNode::Button(ButtonNode {
                    label: "Refresh".to_owned(),
                    event_id: "refresh".to_owned(),
                    style: Some(ViewStyle {
                        padding: Some(8.0),
                        corner_radius: Some(12.0),
                        background: Some(SemanticColorToken::SurfaceContainer),
                        ..ViewStyle::default()
                    }),
                }),
            ],
            style: Some(ViewStyle {
                padding: Some(12.0),
                width: Some(320.0),
                max_width: Some(480.0),
                ..ViewStyle::default()
            }),
            gap: Some(8.0),
            align_items: Some(Alignment::Stretch),
            justify_content: None,
            wrap: false,
            event_id: None,
        }))
    }

    const ROW_COUNTS: &[usize] = &[4, 64];

    /// The per-frame safety pass: depth, node count, text budget, event-ID
    /// uniqueness, and style coherence.
    #[divan::bench(args = ROW_COUNTS)]
    fn validate(bencher: divan::Bencher, rows: usize) {
        let tree = menu_card(rows);
        let limits = ViewLimits::default();
        bencher.bench(|| {
            divan::black_box(&tree)
                .validate(divan::black_box(limits))
                .unwrap()
        });
    }

    /// Decoding a view tree pushed by an extension.
    #[divan::bench(args = ROW_COUNTS)]
    fn deserialize(bencher: divan::Bencher, rows: usize) {
        let encoded = serde_json::to_string(&menu_card(rows)).unwrap();
        bencher.bench(|| serde_json::from_str::<ViewTree>(divan::black_box(&encoded)).unwrap());
    }

    /// Encoding a view tree, as the SDK side does.
    #[divan::bench(args = ROW_COUNTS)]
    fn serialize(bencher: divan::Bencher, rows: usize) {
        let tree = menu_card(rows);
        bencher.bench(|| serde_json::to_string(divan::black_box(&tree)).unwrap());
    }

    /// Decode plus validate: the complete host-side ingestion of one frame.
    #[divan::bench(args = ROW_COUNTS)]
    fn decode_and_validate(bencher: divan::Bencher, rows: usize) {
        let encoded = serde_json::to_string(&menu_card(rows)).unwrap();
        let limits = ViewLimits::default();
        bencher.bench(|| {
            let tree: ViewTree = serde_json::from_str(divan::black_box(&encoded)).unwrap();
            tree.validate(limits).unwrap();
            tree
        });
    }
}
