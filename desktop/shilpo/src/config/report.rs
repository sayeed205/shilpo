use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::{
    provenance::ConfigProvenance, resolver::ConfigSnapshot, source::SourceLocation,
    types::ShellConfig,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectiveWithOriginsReport {
    pub effective: ShellConfig,
    pub origins: BTreeMap<String, SourceLocation>,
}

impl EffectiveWithOriginsReport {
    pub fn from_snapshot(snapshot: &ConfigSnapshot) -> Self {
        Self {
            effective: snapshot.config.clone(),
            origins: snapshot.provenance.map.clone(),
        }
    }

    pub fn new(effective: ShellConfig, provenance: ConfigProvenance) -> Self {
        Self {
            effective,
            origins: provenance.map,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("json serialization of effective origins report")
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("Effective Configuration with Provenance Report\n");
        out.push_str("==============================================\n\n");
        for (path, loc) in &self.origins {
            let line_col_str = match (loc.line, loc.column) {
                (Some(line), Some(col)) => format!(":{line}:{col}"),
                _ => String::new(),
            };
            out.push_str(&format!("{path} => {}{line_col_str}\n", loc.source));
        }
        out
    }
}
