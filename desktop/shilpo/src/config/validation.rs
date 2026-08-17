use crate::config::{
    provenance::ConfigProvenance,
    source::SourceLocation,
    types::{ConfigDiagnostic, ShellConfig},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryScope {
    RejectValue,
    RetainPreviousComponent,
    RejectCandidate,
}

pub fn classify_diagnostic(diagnostic: &ConfigDiagnostic) -> RecoveryScope {
    let path = &diagnostic.path;
    if path == "version" {
        return RecoveryScope::RejectCandidate;
    }
    if path == "theme.font_family"
        || path == "theme.corner_radius_scale"
        || path == "bar.height"
        || path == "bar.padding"
        || path == "bar.widget_spacing"
        || path == "bar.margin.horizontal"
        || path == "bar.margin.vertical"
        || path == "capture.default_selection"
        || path == "clipboard.history_limit"
    {
        return RecoveryScope::RejectValue;
    }
    if path.starts_with("theme.") {
        return RecoveryScope::RetainPreviousComponent;
    }
    if path.starts_with("bar.") {
        return RecoveryScope::RetainPreviousComponent;
    }
    if path.starts_with("desktop.") {
        return RecoveryScope::RetainPreviousComponent;
    }
    if path.starts_with("extensions.") {
        return RecoveryScope::RetainPreviousComponent;
    }
    if path.starts_with("outputs.") || path == "outputs" {
        return RecoveryScope::RetainPreviousComponent;
    }
    if path.starts_with("startup.") {
        return RecoveryScope::RetainPreviousComponent;
    }
    if path.starts_with("capture.") {
        return RecoveryScope::RetainPreviousComponent;
    }
    if path.starts_with("clipboard.") {
        return RecoveryScope::RetainPreviousComponent;
    }
    RecoveryScope::RejectCandidate
}

pub fn apply_scoped_recovery(
    candidate: &mut ShellConfig,
    provenance: &mut ConfigProvenance,
    fallback_config: &ShellConfig,
    fallback_provenance: &ConfigProvenance,
    diagnostics: &[ConfigDiagnostic],
) -> RecoveryScope {
    let mut has_retain_component = false;

    for diagnostic in diagnostics {
        let scope = classify_diagnostic(diagnostic);
        if scope == RecoveryScope::RejectCandidate {
            return RecoveryScope::RejectCandidate;
        }
        if scope == RecoveryScope::RetainPreviousComponent {
            has_retain_component = true;
        }
    }

    // Apply recovery for each diagnostic
    for diagnostic in diagnostics {
        let path = &diagnostic.path;
        let scope = classify_diagnostic(diagnostic);

        match scope {
            RecoveryScope::RejectValue => {
                match path.as_str() {
                    "theme.font_family" => {
                        candidate.theme.font_family = fallback_config.theme.font_family.clone();
                    }
                    "theme.corner_radius_scale" => {
                        candidate.theme.corner_radius_scale =
                            fallback_config.theme.corner_radius_scale;
                    }
                    "bar.height" => {
                        candidate.bar.height = fallback_config.bar.height;
                    }
                    "bar.padding" => {
                        candidate.bar.padding = fallback_config.bar.padding;
                    }
                    "bar.widget_spacing" => {
                        candidate.bar.widget_spacing = fallback_config.bar.widget_spacing;
                    }
                    "bar.margin.horizontal" => {
                        candidate.bar.margin.horizontal = fallback_config.bar.margin.horizontal;
                    }
                    "bar.margin.vertical" => {
                        candidate.bar.margin.vertical = fallback_config.bar.margin.vertical;
                    }
                    "capture.default_selection" => {
                        candidate.capture.default_selection =
                            fallback_config.capture.default_selection.clone();
                    }
                    "clipboard.history_limit" => {
                        candidate.clipboard.history_limit = fallback_config.clipboard.history_limit;
                    }
                    _ => {}
                }
                restore_provenance_path(provenance, fallback_provenance, path);
            }
            RecoveryScope::RetainPreviousComponent => {
                let component = path.split('.').next().unwrap_or(path.as_str());
                match component {
                    "theme" => {
                        candidate.theme = fallback_config.theme.clone();
                    }
                    "bar" => {
                        candidate.bar = fallback_config.bar.clone();
                    }
                    "desktop" => {
                        candidate.desktop = fallback_config.desktop.clone();
                    }
                    "extensions" => {
                        candidate.extensions = fallback_config.extensions.clone();
                    }
                    "outputs" => {
                        candidate.outputs = fallback_config.outputs.clone();
                    }
                    "startup" => {
                        candidate.startup = fallback_config.startup.clone();
                    }
                    "capture" => {
                        candidate.capture = fallback_config.capture.clone();
                    }
                    "clipboard" => {
                        candidate.clipboard = fallback_config.clipboard.clone();
                    }
                    _ => {}
                }
                restore_provenance_component(provenance, fallback_provenance, component);
            }
            RecoveryScope::RejectCandidate => unreachable!(),
        }
    }

    // Revalidate candidate after scoped recovery pass
    if candidate.validate().is_ok() {
        if has_retain_component {
            RecoveryScope::RetainPreviousComponent
        } else {
            RecoveryScope::RejectValue
        }
    } else {
        RecoveryScope::RejectCandidate
    }
}

fn restore_provenance_path(
    provenance: &mut ConfigProvenance,
    fallback_provenance: &ConfigProvenance,
    path: &str,
) {
    provenance.remove_prefix(path);
    if let Some(loc) = fallback_provenance.get(path) {
        provenance.set(path, loc.clone());
    } else {
        provenance.set(path, SourceLocation::defaults());
    }
}

fn restore_provenance_component(
    provenance: &mut ConfigProvenance,
    fallback_provenance: &ConfigProvenance,
    component: &str,
) {
    provenance.remove_prefix(component);
    let prefix_dot = format!("{component}.");
    let prefix_bracket = format!("{component}[");
    for (k, loc) in &fallback_provenance.map {
        if k == component || k.starts_with(&prefix_dot) || k.starts_with(&prefix_bracket) {
            provenance.set(k.clone(), loc.clone());
        }
    }
}
