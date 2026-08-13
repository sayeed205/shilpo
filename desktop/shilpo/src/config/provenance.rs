use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::source::SourceLocation;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigProvenance {
    pub map: BTreeMap<String, SourceLocation>,
}

impl ConfigProvenance {
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    pub fn get(&self, path: &str) -> Option<&SourceLocation> {
        if let Some(loc) = self.map.get(path) {
            return Some(loc);
        }
        // Check for array item or child property fallback to parent path
        let mut cur = path;
        while let Some(idx) = cur.rfind('.').or_else(|| cur.rfind('[')) {
            cur = &cur[..idx];
            if let Some(loc) = self.map.get(cur) {
                return Some(loc);
            }
        }
        None
    }

    pub fn set(&mut self, path: impl Into<String>, loc: SourceLocation) {
        self.map.insert(path.into(), loc);
    }

    pub fn remove_prefix(&mut self, prefix: &str) {
        let prefix_dot = format!("{prefix}.");
        let prefix_bracket = format!("{prefix}[");
        self.map.retain(|k, _| {
            k != prefix && !k.starts_with(&prefix_dot) && !k.starts_with(&prefix_bracket)
        });
    }
}

pub fn format_key(prefix: &str, key: &str) -> String {
    let formatted_key =
        if key.contains('-') || key.contains('.') || key.contains(' ') || key.contains('"') {
            format!("\"{key}\"")
        } else {
            key.to_string()
        };

    if prefix.is_empty() {
        formatted_key
    } else {
        format!("{prefix}.{formatted_key}")
    }
}
