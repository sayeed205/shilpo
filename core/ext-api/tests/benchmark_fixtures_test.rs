use shilpo_ext_api::{
    CanonicalId, ContainerDirection, ContainerNode, ContributionId, ExtensionId, ViewLimits,
    ViewNode, ViewTree,
};

fn build_divider_tree(total_nodes: usize) -> ViewTree {
    assert!(total_nodes >= 1);
    if total_nodes == 1 {
        ViewTree::new(ViewNode::Divider)
    } else {
        let child_count = total_nodes - 1;
        let children = (0..child_count).map(|_| ViewNode::Divider).collect();
        ViewTree::new(ViewNode::Container(ContainerNode {
            direction: ContainerDirection::Column,
            children,
            style: None,
            gap: None,
            align_items: None,
            justify_content: None,
            wrap: false,
            event_id: None,
        }))
    }
}

#[test]
fn test_view_tree_benchmark_sizes_and_rejection() {
    let limits = ViewLimits::default();

    // Verify valid benchmark tree sizes: 1, 64, 256, 1024
    for &size in &[1, 64, 256, 1_024] {
        let tree = build_divider_tree(size);
        assert!(
            tree.validate(limits).is_ok(),
            "Tree of size {size} must pass validation with default limits (max_nodes = {})",
            limits.max_nodes
        );
    }

    // Verify exact 1025-node rejection tree
    let rejection_tree = build_divider_tree(1_025);
    let result = rejection_tree.validate(limits);
    assert!(
        result.is_err(),
        "Tree of size 1025 must exceed default max_nodes limit of 1024"
    );
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("maximum node count"),
        "Error message should mention node count limit: {error_msg}"
    );
}

#[test]
fn test_benchmark_identity_samples_consistency() {
    // Valid extension IDs
    for id_str in &[
        "io.github.app",
        "com.example.weather-widget",
        "org.shilpo.long-subdomain-123.extension-package-name.v1-release-candidate",
    ] {
        assert!(
            ExtensionId::new(*id_str).is_ok(),
            "Expected valid: {id_str}"
        );
    }

    // Invalid extension IDs
    for id_str in &[
        "two.segments",
        "IO.GITHUB.APP",
        "org.shilpo.invalid_underscore",
        "org.-invalid.app",
    ] {
        assert!(
            ExtensionId::new(*id_str).is_err(),
            "Expected invalid: {id_str}"
        );
    }

    // Valid contribution IDs
    for id_str in &[
        "clock",
        "weather-widget_v1",
        "a-very-long-contribution-identifier-with-many-words-and-subparts",
    ] {
        assert!(
            ContributionId::new(*id_str).is_ok(),
            "Expected valid: {id_str}"
        );
    }

    // Invalid contribution IDs
    for id_str in &["-leading-dash", "InvalidCaps", "slash/forbidden", ""] {
        assert!(
            ContributionId::new(*id_str).is_err(),
            "Expected invalid: {id_str}"
        );
    }

    // Valid canonical IDs
    for id_str in &[
        "io.github.app/clock",
        "com.example.weather/widget_v1",
        "org.shilpo.long-subdomain-123.extension-package-name/a-very-long-contribution-identifier",
    ] {
        assert!(
            id_str.parse::<CanonicalId>().is_ok(),
            "Expected valid: {id_str}"
        );
    }

    // Invalid canonical IDs
    for id_str in &[
        "io.github.app.clock",
        "io.github.app/clock/extra",
        "invalid/clock",
        "io.github.app/-invalid",
    ] {
        assert!(
            id_str.parse::<CanonicalId>().is_err(),
            "Expected invalid: {id_str}"
        );
    }
}
