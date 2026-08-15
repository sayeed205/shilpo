use std::fs;
use tempfile::tempdir;

use shilpo_ext_runtime::{
    InspectionPolicy, LintDiagnostic, LintSeverity, inspect_extension, validate_png_bytes,
    validate_svg_bytes,
};

#[test]
fn test_missing_manifest() {
    let dir = tempdir().unwrap();
    let report = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );

    assert!(!report.passed);
    assert_eq!(report.error_count, 1);
    let diag = &report.diagnostics[0];
    assert_eq!(diag.rule_id, "manifest.missing");
    assert_eq!(diag.severity, LintSeverity::Error);
    assert_eq!(diag.path.as_deref(), Some("extension.toml"));
    assert!(diag.remediation.is_some());
}

#[test]
fn test_invalid_toml_syntax_with_line_and_col() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        "schema_version = 1\nid = \"foo\"\n[invalid toml here\n",
    )
    .unwrap();

    let report = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    assert!(!report.passed);
    assert_eq!(report.error_count, 1);
    let diag = &report.diagnostics[0];
    assert_eq!(diag.rule_id, "manifest.syntax");
    assert_eq!(diag.severity, LintSeverity::Error);
    assert_eq!(diag.path.as_deref(), Some("extension.toml"));
    assert!(diag.line.is_some());
    assert!(diag.column.is_some());
}

#[test]
fn test_unsupported_schema_and_api_version() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 99
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"
api_version = "2.0.0"
"#,
    )
    .unwrap();

    let report = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    assert!(!report.passed);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "manifest.unsupported-schema")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "manifest.unsupported-api-version")
    );
}

#[test]
fn test_incompatible_shilpo_version() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"
api_version = "0.1.0"
min_shilpo_version = "99.0.0"
"#,
    )
    .unwrap();

    let report = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    assert!(!report.passed);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "manifest.incompatible-shilpo-version")
    );
}

#[test]
fn test_duplicate_contribution_ids() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"

[[contributions.bar_widgets]]
id = "my-widget"
name = "Widget 1"

[[contributions.bar_widgets]]
id = "my-widget"
name = "Widget 2"
"#,
    )
    .unwrap();

    let report = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    assert!(!report.passed);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "contribution.duplicate-id")
    );
}

#[test]
fn test_invalid_contribution_references() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"

[[contributions.bar_menus]]
id = "my-menu"
name = "Menu"
bar_widget = "non-existent-widget"

[[contributions.keyboard_shortcuts]]
id = "my-shortcut"
name = "Shortcut"
action = "non-existent-action"
"#,
    )
    .unwrap();

    let report = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    assert!(!report.passed);
    let refs = report
        .diagnostics
        .iter()
        .filter(|d| d.rule_id == "contribution.invalid-reference")
        .count();
    assert_eq!(refs, 2);
}

#[test]
fn test_invalid_desktop_widget_bounds() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"

[[contributions.desktop_widgets]]
id = "my-widget"
name = "Widget"
min_width = 500
default_width = 300
"#,
    )
    .unwrap();

    let report = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    assert!(!report.passed);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "contribution.invalid-bounds")
    );
}

#[test]
fn test_capabilities_and_subscriptions_rules() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"

[[capabilities]]
kind = "network:http"
hosts = ["*"]

[[capabilities]]
kind = "filesystem:read"
paths = ["/"]

[[capabilities]]
kind = "actions:invoke"
actions = ["*"]

[[subscriptions]]
event = "theme_changed"
"#,
    )
    .unwrap();

    let report = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    // Errors: invalid wildcard in actions:invoke, invalid wildcard in secrets, missing subscription grant for theme_changed
    assert!(!report.passed);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "capability.invalid-wildcard")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "capability.missing-subscription-grant")
    );
    // Warnings: broad network and broad filesystem
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "capability.broad-network-scope")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "capability.broad-filesystem-scope")
    );
}

#[test]
fn test_deny_warnings_flag() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"

[[capabilities]]
kind = "network:http"
hosts = ["*"]
"#,
    )
    .unwrap();

    let report_allow = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    assert!(report_allow.passed);
    assert_eq!(report_allow.error_count, 0);
    assert_eq!(report_allow.warning_count, 1);

    let report_deny = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: true,
        },
    );
    assert!(!report_deny.passed);
    assert_eq!(report_deny.error_count, 0);
    assert_eq!(report_deny.warning_count, 1);
}

#[test]
fn test_project_config_validation() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("shilpo-ext.json"),
        r#"{ "language": "rust", "entry": "src/non_existent.rs" }"#,
    )
    .unwrap();

    let report = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    assert!(!report.passed);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "config.entry-missing")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "config.language-mismatch")
    );
}

#[test]
fn test_settings_schema_validation() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"

[[contributions.settings_pages]]
id = "settings"
name = "Settings"
schema = "schemas/invalid.json"
"#,
    )
    .unwrap();

    fs::create_dir_all(dir.path().join("schemas")).unwrap();
    fs::write(
        dir.path().join("schemas/invalid.json"),
        r#"{ "type": "object", "properties": { "num": { "type": "number", "default": "not-a-number" } } }"#,
    )
    .unwrap();

    let report = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    assert!(!report.passed);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "settings.invalid-defaults")
    );
}

#[test]
fn test_png_and_svg_deep_validation() {
    // Valid PNG minimal header and chunks
    let mut valid_png = Vec::new();
    valid_png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    // IHDR (length 13)
    valid_png.extend_from_slice(&13u32.to_be_bytes());
    valid_png.extend_from_slice(b"IHDR");
    valid_png.extend_from_slice(&100u32.to_be_bytes()); // width 100
    valid_png.extend_from_slice(&100u32.to_be_bytes()); // height 100
    valid_png.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit truecolor
    valid_png.extend_from_slice(&0u32.to_be_bytes()); // dummy CRC
    // IEND (length 0)
    valid_png.extend_from_slice(&0u32.to_be_bytes());
    valid_png.extend_from_slice(b"IEND");
    valid_png.extend_from_slice(&0u32.to_be_bytes()); // dummy CRC

    assert!(validate_png_bytes(&valid_png).is_ok());

    // Invalid PNG (wrong signature)
    assert!(validate_png_bytes(b"NOT A PNG").is_err());

    // Valid SVG
    let valid_svg = r#"<?xml version="1.0"?>
<!-- comment -->
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
    <circle cx="50" cy="50" r="40" />
</svg>"#;
    assert!(validate_svg_bytes(valid_svg.as_bytes()).is_ok());

    // Invalid SVG (root not svg)
    let invalid_svg =
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Not SVG</h1></body></html>"#;
    assert!(validate_svg_bytes(invalid_svg.as_bytes()).is_err());
}

#[test]
fn test_wasm_artifact_lint_vs_check() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"

[library]
path = "extension.wasm"
"#,
    )
    .unwrap();

    // In Lint mode: missing wasm is an informational diagnostic (wasm.not-built), lint passes!
    let report_lint = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    assert!(report_lint.passed);
    assert_eq!(report_lint.error_count, 0);
    assert_eq!(report_lint.info_count, 2); // manifest.valid and wasm.not-built
    assert!(
        report_lint
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "wasm.not-built" && d.severity == LintSeverity::Info)
    );

    // In Check mode: missing wasm is an error (file.missing), check fails!
    let report_check = inspect_extension(dir.path(), InspectionPolicy::Check);
    assert!(!report_check.passed);
    assert!(
        report_check
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "file.missing" && d.severity == LintSeverity::Error)
    );
}

#[test]
fn test_read_only_guarantee() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test"
version = "0.1.0"
"#,
    )
    .unwrap();

    let entries_before: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let _ = inspect_extension(
        dir.path(),
        InspectionPolicy::Lint {
            deny_warnings: false,
        },
    );
    let entries_after: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();

    assert_eq!(entries_before, entries_after);
}

#[test]
fn test_deterministic_diagnostic_sorting() {
    let d1 = LintDiagnostic::warning("rule.b", "message 2").with_path("z_file.txt");
    let d2 = LintDiagnostic::error("rule.a", "message 1").with_path("a_file.txt");
    let d3 = LintDiagnostic::info("rule.c", "message 3")
        .with_path("a_file.txt")
        .with_line_col(10, 5);
    let d4 = LintDiagnostic::info("rule.c", "message 3")
        .with_path("a_file.txt")
        .with_line_col(5, 2);

    let mut list = [d1, d2, d3, d4];
    list.sort();

    assert_eq!(list[0].path.as_deref(), Some("a_file.txt"));
    assert_eq!(list[0].line, None);
    assert_eq!(list[1].line, Some(5));
    assert_eq!(list[2].line, Some(10));
    assert_eq!(list[3].path.as_deref(), Some("z_file.txt"));
}
