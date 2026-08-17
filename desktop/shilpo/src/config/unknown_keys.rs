use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::config::{
    merge::{key_offset, line_col_from_offset},
    provenance::format_key,
    source::ConfigSource,
    types::ShellConfig,
};

/// Stable machine-readable diagnostic code for unknown config keys.
///
/// Surfaced by the shell's structured logging and reserved for the public
/// `shilpo config validate/effective` CLI (#128).
pub const UNKNOWN_KEY_CODE: &str = "config.unknown_key";

/// A single unknown-key warning produced while scanning a file-backed source.
///
/// Warnings are non-fatal: the offending entry is ignored in the temporary
/// in-memory document only, and never sets a [`crate::config::RecoveryScope`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownConfigKey {
    /// Canonical full dotted path of the unknown entry (e.g. `bar.heigth`).
    pub path: String,
    /// File-backed source that contains the unknown entry.
    pub source: ConfigSource,
    /// 1-based line of the unknown key, when a source span is available.
    pub line: Option<usize>,
    /// 1-based column of the unknown key, when a source span is available.
    pub column: Option<usize>,
    /// The unknown key segment itself (e.g. `heigth`).
    pub key: String,
    /// Suggested canonical path, when a uniquely best sibling within edit
    /// distance `<= 3` exists.
    pub suggestion: Option<String>,
}

impl UnknownConfigKey {
    pub const CODE: &'static str = UNKNOWN_KEY_CODE;

    /// Stable machine-readable code for this warning.
    pub fn code(&self) -> &'static str {
        Self::CODE
    }

    /// `source:line:column`, or bare source when the location is unknown.
    pub fn location(&self) -> String {
        match (self.line, self.column) {
            (Some(line), Some(column)) => format!("{}:{line}:{column}", self.source),
            _ => self.source.to_string(),
        }
    }

    /// Pure human-readable form of this warning (path, source location, and
    /// optional suggestion) used by Doctor and the shell's log formatter.
    pub fn describe(&self) -> String {
        let mut message = format!("unknown config key '{}' at {}", self.path, self.location());
        if let Some(suggestion) = &self.suggestion {
            message.push_str(&format!(" (suggestion: '{}')", suggestion));
        }
        message
    }
}

/// Traversable view of the generated `ShellConfig::schema()`, resolved from
/// its JSON form. Distinguishes fixed object fields, arrays with a fixed
/// element shape, typed dynamic maps (`additionalProperties` referring to a
/// schema), fully open user data (`additionalProperties: true` or untyped),
/// and scalar leaves.
#[derive(Clone, Debug)]
enum SchemaShape {
    Object {
        fields: BTreeMap<String, SchemaShape>,
        additional: Option<Box<SchemaShape>>,
    },
    Array {
        item: Box<SchemaShape>,
    },
    /// Everything below is allowed; traversal stops without diagnostics.
    Open,
    /// Scalar value; traversal never descends.
    Leaf,
}

/// Build the authoritative schema shape for declarative config from
/// [`ShellConfig::schema()`] by resolving local `$defs` references.
fn root_shape() -> SchemaShape {
    let schema = ShellConfig::schema();
    let defs = schema
        .get("$defs")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    shape_of_value(schema.as_value(), &defs)
}

fn shape_of_value(
    schema: &serde_json::Value,
    defs: &serde_json::Map<String, serde_json::Value>,
) -> SchemaShape {
    if let Some(target) = schema
        .get("$ref")
        .and_then(serde_json::Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| defs.get(name))
    {
        return shape_of_value(target, defs);
    }
    // Local $refs must resolve; an unresolvable reference is treated as
    // fully open rather than inventing key names.
    if schema.get("$ref").is_some() {
        return SchemaShape::Open;
    }

    if let Some(alternatives) = schema
        .get("anyOf")
        .or_else(|| schema.get("oneOf"))
        .and_then(serde_json::Value::as_array)
    {
        if let Some(first) = alternatives.iter().find(|alt| !is_null_schema(alt)) {
            return shape_of_value(first, defs);
        }
        return SchemaShape::Leaf;
    }

    let types: Vec<&str> = match schema.get("type") {
        Some(serde_json::Value::String(single)) => vec![single.as_str()],
        Some(serde_json::Value::Array(many)) => {
            many.iter().filter_map(serde_json::Value::as_str).collect()
        }
        _ => Vec::new(),
    };

    if types.contains(&"object") {
        let mut fields = BTreeMap::new();
        if let Some(properties) = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
        {
            for (name, property) in properties {
                fields.insert(name.clone(), shape_of_value(property, defs));
            }
        }
        let additional = match schema.get("additionalProperties") {
            None | Some(serde_json::Value::Bool(false)) => None,
            Some(serde_json::Value::Bool(true)) => Some(Box::new(SchemaShape::Open)),
            Some(other) => Some(Box::new(shape_of_value(other, defs))),
        };
        SchemaShape::Object { fields, additional }
    } else if types.contains(&"array") {
        let item = schema
            .get("items")
            .map(|items| shape_of_value(items, defs))
            .unwrap_or(SchemaShape::Open);
        SchemaShape::Array {
            item: Box::new(item),
        }
    } else if !types.is_empty() {
        SchemaShape::Leaf
    } else {
        // Untyped schema (e.g. `serde_json::Value` fields) accepts any shape.
        SchemaShape::Open
    }
}

fn is_null_schema(schema: &serde_json::Value) -> bool {
    match schema.get("type") {
        Some(serde_json::Value::String(single)) => single == "null",
        Some(serde_json::Value::Array(many)) => many.iter().all(|t| t.as_str() == Some("null")),
        _ => false,
    }
}

/// Scan a parsed source document against the authoritative schema shape,
/// emitting one warning per unknown entry and removing the entry from the
/// temporary in-memory document only. User files are never modified.
///
/// Warnings are returned in document order; the caller merges sources in
/// precedence order so the final report follows deterministic
/// source-then-document ordering.
pub fn sanitize_document(
    doc: &mut DocumentMut,
    source: &ConfigSource,
    text: &str,
) -> Vec<UnknownConfigKey> {
    let shape = root_shape();
    let mut warnings = Vec::new();
    walk_item(doc.as_item_mut(), "", &shape, source, text, &mut warnings);
    warnings
}

fn walk_item(
    item: &mut Item,
    prefix: &str,
    shape: &SchemaShape,
    source: &ConfigSource,
    text: &str,
    warnings: &mut Vec<UnknownConfigKey>,
) {
    match item {
        Item::Table(table) => walk_table(table, prefix, shape, source, text, warnings),
        Item::ArrayOfTables(array) => {
            if let SchemaShape::Array { item } = shape {
                walk_array_of_tables(array, prefix, item, source, text, warnings);
            }
        }
        Item::Value(value) => walk_value(value, prefix, shape, source, text, warnings),
        Item::None => {}
    }
}

fn walk_value(
    value: &mut Value,
    prefix: &str,
    shape: &SchemaShape,
    source: &ConfigSource,
    text: &str,
    warnings: &mut Vec<UnknownConfigKey>,
) {
    match value {
        Value::InlineTable(table) => {
            walk_inline_table(table, prefix, shape, source, text, warnings);
        }
        Value::Array(array) => {
            if let SchemaShape::Array { item } = shape {
                walk_array(array, prefix, item, source, text, warnings);
            }
        }
        _ => {}
    }
}

fn walk_table(
    table: &mut Table,
    prefix: &str,
    shape: &SchemaShape,
    source: &ConfigSource,
    text: &str,
    warnings: &mut Vec<UnknownConfigKey>,
) {
    let SchemaShape::Object { fields, additional } = shape else {
        return;
    };
    let keys: Vec<String> = table.iter().map(|(key, _)| key.to_string()).collect();
    for key in keys {
        let key_span = table.get_key_value(&key).and_then(|(k, _)| k.span());
        let value_span = table.get(&key).and_then(|v| v.span());
        let subpath = format_key(prefix, &key);
        if let Some(field_shape) = fields.get(&key) {
            if let Some(item) = table.get_mut(&key) {
                walk_item(item, &subpath, field_shape, source, text, warnings);
            }
        } else if let Some(inner) = additional {
            if let Some(item) = table.get_mut(&key) {
                walk_item(item, &subpath, inner, source, text, warnings);
            }
        } else {
            table.remove(&key);
            let (line, column) = locate_with_fallback(key_span, value_span, &key, text);
            let suggestion = best_suggestion(&key, fields.keys().map(String::as_str))
                .map(|candidate| format_key(prefix, &candidate));
            warnings.push(UnknownConfigKey {
                path: subpath,
                source: source.clone(),
                line,
                column,
                key,
                suggestion,
            });
        }
    }
}

fn walk_inline_table(
    table: &mut InlineTable,
    prefix: &str,
    shape: &SchemaShape,
    source: &ConfigSource,
    text: &str,
    warnings: &mut Vec<UnknownConfigKey>,
) {
    let SchemaShape::Object { fields, additional } = shape else {
        return;
    };
    let keys: Vec<String> = table.iter().map(|(key, _)| key.to_string()).collect();
    for key in keys {
        let key_span = table.get_key_value(&key).and_then(|(k, _)| k.span());
        let value_span = table.get(&key).and_then(|v| v.span());
        let subpath = format_key(prefix, &key);
        if let Some(field_shape) = fields.get(&key) {
            if let Some(value) = table.get_mut(&key) {
                walk_value(value, &subpath, field_shape, source, text, warnings);
            }
        } else if let Some(inner) = additional {
            if let Some(value) = table.get_mut(&key) {
                walk_value(value, &subpath, inner, source, text, warnings);
            }
        } else {
            table.remove(&key);
            let (line, column) = locate_with_fallback(key_span, value_span, &key, text);
            let suggestion = best_suggestion(&key, fields.keys().map(String::as_str))
                .map(|candidate| format_key(prefix, &candidate));
            warnings.push(UnknownConfigKey {
                path: subpath,
                source: source.clone(),
                line,
                column,
                key,
                suggestion,
            });
        }
    }
}

fn walk_array(
    array: &mut Array,
    prefix: &str,
    item: &SchemaShape,
    source: &ConfigSource,
    text: &str,
    warnings: &mut Vec<UnknownConfigKey>,
) {
    for (index, element) in array.iter_mut().enumerate() {
        let subpath = format!("{prefix}[{index}]");
        walk_value(element, &subpath, item, source, text, warnings);
    }
}

fn walk_array_of_tables(
    array: &mut ArrayOfTables,
    prefix: &str,
    item: &SchemaShape,
    source: &ConfigSource,
    text: &str,
    warnings: &mut Vec<UnknownConfigKey>,
) {
    for (index, table) in array.iter_mut().enumerate() {
        let subpath = format!("{prefix}[{index}]");
        walk_table(table, &subpath, item, source, text, warnings);
    }
}

fn locate_with_fallback(
    key_span: Option<std::ops::Range<usize>>,
    value_span: Option<std::ops::Range<usize>>,
    key: &str,
    text: &str,
) -> (Option<usize>, Option<usize>) {
    let offset = key_span
        .map(|span| span.start)
        .or_else(|| value_span.map(|span| span.start))
        .or_else(|| key_offset(text, key));
    match offset {
        Some(offset) => {
            let (line, column) = line_col_from_offset(text, offset);
            (Some(line), Some(column))
        }
        None => (None, None),
    }
}

/// Classic Levenshtein edit distance (case-sensitive, deterministic).
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            current[j + 1] = if ca == cb {
                previous[j]
            } else {
                1 + previous[j].min(current[j]).min(previous[j + 1])
            };
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// Offer a suggestion only when the best candidate distance is `<= 3` and
/// uniquely best. Ties never produce a suggestion, so iteration order of the
/// candidate set cannot affect the outcome.
fn best_suggestion(
    unknown: &str,
    candidates: impl IntoIterator<Item = impl AsRef<str>>,
) -> Option<String> {
    let mut best_distance: Option<usize> = None;
    let mut best_candidate: Option<String> = None;
    let mut tied = false;

    for candidate in candidates {
        let distance = levenshtein_distance(unknown, candidate.as_ref());
        match best_distance {
            None => {
                best_distance = Some(distance);
                best_candidate = Some(candidate.as_ref().to_string());
            }
            Some(best) if distance < best => {
                best_distance = Some(distance);
                best_candidate = Some(candidate.as_ref().to_string());
                tied = false;
            }
            Some(best) if distance == best => tied = true,
            _ => {}
        }
    }

    match (best_distance, best_candidate) {
        (Some(distance), Some(candidate)) if distance <= 3 && !tied => Some(candidate),
        _ => None,
    }
}

/// Emit one structured `tracing::warn!` event per unknown key. The human text
/// comes from the pure [`UnknownConfigKey::describe`] formatter so it is
/// testable without a process-global tracing subscriber.
pub fn log_unknown_key_warnings(warnings: &[UnknownConfigKey]) {
    for warning in warnings {
        tracing::warn!(
            code = UnknownConfigKey::CODE,
            source = %warning.source,
            key = %warning.key,
            key_path = %warning.path,
            line = warning.line.unwrap_or(0),
            column = warning.column.unwrap_or(0),
            suggestion = warning.suggestion.as_deref().unwrap_or(""),
            "{}",
            warning.describe()
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn path() -> PathBuf {
        PathBuf::from("/tmp/test-config.toml")
    }

    fn source_primary() -> ConfigSource {
        ConfigSource::Primary { path: path() }
    }

    #[test]
    fn levenshtein_basic_distances() {
        assert_eq!(levenshtein_distance("baer", "bar"), 1);
        assert_eq!(levenshtein_distance("heigth", "height"), 2);
        assert_eq!(levenshtein_distance("", "height"), 6);
        assert_eq!(levenshtein_distance("bar", "bar"), 0);
        assert_eq!(levenshtein_distance("widgets", "exclusive_zone"), 13);
    }

    #[test]
    fn suggestion_only_within_threshold() {
        assert_eq!(
            best_suggestion("baer", ["bar", "height", "padding"]),
            Some("bar".to_string())
        );
        assert_eq!(
            best_suggestion("heigth", ["height", "width"]),
            Some("height".to_string())
        );
        // Distance 4 exceeds the threshold.
        assert_eq!(best_suggestion("abcdefgh", ["widgets"]), None);
        assert_eq!(best_suggestion("xyzabc", ["height", "padding"]), None);
    }

    #[test]
    fn suggestion_suppressed_on_tie() {
        assert_eq!(best_suggestion("x", ["a", "b", "zz"]), None);
        assert_eq!(best_suggestion("zz", ["ab", "ba", "xy"]), None);
    }

    #[test]
    fn suggestion_case_sensitive() {
        // "Height" differs only by case from "height": distance 1, unique.
        assert_eq!(
            best_suggestion("Height", ["height", "padding"]),
            Some("height".to_string())
        );
        // Two case-differing candidates are equally close: no suggestion.
        assert_eq!(best_suggestion("HEIGHT", ["height", "Height"]), None);
    }

    #[test]
    fn unknown_top_level_key_warns_with_suggestion() {
        let text = "version = 1\nbaer = 1\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert_eq!(warnings.len(), 1);
        let warning = &warnings[0];
        assert_eq!(warning.path, "baer");
        assert_eq!(warning.key, "baer");
        assert_eq!(warning.suggestion.as_deref(), Some("bar"));
        assert_eq!(warning.line, Some(2));
        assert_eq!(warning.column, Some(1));
        assert_eq!(warning.code(), "config.unknown_key");
        assert!(!doc.to_string().contains("baer"));
    }

    #[test]
    fn unknown_nested_key_warns_with_full_path_suggestion() {
        let text = "version = 1\n[bar]\nheigth = 32\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert_eq!(warnings.len(), 1);
        let warning = &warnings[0];
        assert_eq!(warning.path, "bar.heigth");
        assert_eq!(warning.key, "heigth");
        assert_eq!(warning.suggestion.as_deref(), Some("bar.height"));
        assert_eq!(warning.line, Some(3));
        assert_eq!(warning.column, Some(1));
        assert!(!doc.to_string().contains("heigth"));
    }

    #[test]
    fn distant_key_has_no_suggestion() {
        let text = "version = 1\n[bar]\nqqqqqqqq = 32\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].path, "bar.qqqqqqqq");
        assert_eq!(warnings[0].suggestion, None);
    }

    #[test]
    fn unknown_output_child_reports_typed_map_path() {
        let text = "version = 1\n[outputs.\"DP-1\"]\nheigth = 32\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert_eq!(warnings.len(), 1);
        let warning = &warnings[0];
        assert_eq!(warning.path, "outputs.\"DP-1\".heigth");
        assert_eq!(warning.key, "heigth");
        assert_eq!(
            warning.suggestion.as_deref(),
            Some("outputs.\"DP-1\".height")
        );
    }

    #[test]
    fn unknown_array_of_table_field_uses_indexed_path() {
        let text = "version = 1\n[[desktop.widgets]]\ninstance = \"w1\"\ncontribution = \"ext:io.github.alice.world-clock/desktop\"\nwidth = 100\nheight = 100\nheigth = 100\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert_eq!(warnings.len(), 1);
        let warning = &warnings[0];
        assert_eq!(warning.path, "desktop.widgets[0].heigth");
        assert_eq!(
            warning.suggestion.as_deref(),
            Some("desktop.widgets[0].height")
        );
    }

    #[test]
    fn unknown_array_of_inline_table_field_uses_indexed_path() {
        let text = "version = 1\n[bar]\nheight = 64\nwidgets = { start = [], center = [], end = [] }\n[desktop]\nwidgets = [{ instance = \"w1\", contribution = \"ext:io.github.alice.world-clock/desktop\", width = 100, height = 100, heigth = 100 }]\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].path, "desktop.widgets[0].heigth");
        assert_eq!(
            warnings[0].suggestion.as_deref(),
            Some("desktop.widgets[0].height")
        );
    }

    #[test]
    fn unknown_inline_table_key_is_removed_in_memory() {
        let text = "version = 1\n[bar]\nheight = 32\nmargin = { horizontal = 8, vertikal = 4 }\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].path, "bar.margin.vertikal");
        assert_eq!(
            warnings[0].suggestion.as_deref(),
            Some("bar.margin.vertical")
        );
        assert!(!doc.to_string().contains("vertikal"));
        assert!(doc.to_string().contains("horizontal"));
    }

    #[test]
    fn unknown_table_reports_only_highest_boundary() {
        let text = "version = 1\n[baer]\nheight = 32\nheigth = 40\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].path, "baer");
        assert_eq!(warnings[0].suggestion.as_deref(), Some("bar"));
        assert!(!doc.to_string().contains("baer"));
    }

    #[test]
    fn open_extension_settings_accept_arbitrary_data() {
        let text = "version = 1\n[extensions.settings.\"org.shilpo.weather\"]\nlocation = \"Kolkata\"\nunit = { name = \"metric\", deep = [1, 2, { deeper = true }] }\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert!(warnings.is_empty());
        assert!(doc.to_string().contains("org.shilpo.weather"));
    }

    #[test]
    fn open_extension_settings_accept_inline_objects() {
        let text = "version = 1\nextensions = { settings = { \"org.shilpo.weather\" = { location = \"Kolkata\" }, \"org.shilpo.notes\" = { anything = [1, {\"x.y\" = 2}] } } }\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert!(warnings.is_empty());
    }

    #[test]
    fn widget_settings_value_is_open() {
        let text = "version = 1\n[[desktop.widgets]]\ninstance = \"w1\"\ncontribution = \"ext:io.github.alice.world-clock/desktop\"\nwidth = 100\nheight = 100\n[desktop.widgets.settings]\narbitrary = { nested = [1] }\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert!(warnings.is_empty());
    }

    #[test]
    fn valid_siblings_remain_after_stripping() {
        let text = "version = 1\n[bar]\nheight = 32\nheigth = 64\npadding = 12\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert_eq!(warnings.len(), 1);
        let sanitized = doc.to_string();
        assert!(sanitized.contains("height = 32"));
        assert!(sanitized.contains("padding = 12"));
        assert!(!sanitized.contains("heigth"));
    }

    #[test]
    fn describe_formatter_is_deterministic() {
        let warning = UnknownConfigKey {
            path: "bar.heigth".into(),
            source: ConfigSource::Primary {
                path: PathBuf::from("/cfg/config.toml"),
            },
            line: Some(3),
            column: Some(1),
            key: "heigth".into(),
            suggestion: Some("bar.height".into()),
        };
        assert_eq!(
            warning.describe(),
            "unknown config key 'bar.heigth' at /cfg/config.toml:3:1 (suggestion: 'bar.height')"
        );

        let no_suggestion = UnknownConfigKey {
            suggestion: None,
            ..warning.clone()
        };
        assert_eq!(
            no_suggestion.describe(),
            "unknown config key 'bar.heigth' at /cfg/config.toml:3:1"
        );
    }

    #[test]
    fn dotted_quoted_dynamic_keys_use_canonical_formatter() {
        let text =
            "version = 1\n[outputs.\"DP-1\"]\nenabled = true\n[outputs.\"arc.y\"]\nheigth = 10\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let warnings = sanitize_document(&mut doc, &source_primary(), text);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].path, "outputs.\"arc.y\".heigth");
    }
}
