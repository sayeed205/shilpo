use crate::config::{
    provenance::{ConfigProvenance, format_key},
    source::{ConfigSource, SourceLocation},
    types::{ConfigDiagnostic, ConfigError, ShellConfig},
};
use std::fs;
use toml_edit::{DocumentMut, Item, Table};

pub fn line_col_from_offset(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let mut line = 1;
    let mut col = 1;
    for (i, b) in text.as_bytes().iter().enumerate() {
        if i >= offset {
            break;
        }
        if *b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub(crate) fn key_offset(text: &str, key: &str) -> Option<usize> {
    let quoted = format!("\"{key}\"");
    text.lines().enumerate().find_map(|(line, value)| {
        let trimmed = value.trim_start();
        let candidate = trimmed
            .strip_prefix(key)
            .or_else(|| trimmed.strip_prefix(&quoted))?;
        if candidate.trim_start().starts_with('=') {
            Some(value.len() - trimmed.len())
                .map(|column| text.lines().take(line).map(|l| l.len() + 1).sum::<usize>() + column)
        } else {
            None
        }
    })
}

pub fn initial_merged_document() -> (DocumentMut, ConfigProvenance) {
    let default_config = ShellConfig::default();
    let default_toml = toml::to_string_pretty(&default_config).expect("default config toml string");
    let doc: DocumentMut = default_toml.parse().expect("parse default config toml");

    let mut provenance = ConfigProvenance::new();
    let defaults_loc = SourceLocation::defaults();
    record_provenance_tree(doc.as_item(), "", &defaults_loc, &mut provenance);

    (doc, provenance)
}

/// Read and parse a file-backed source document, keeping the original text
/// alongside so span offsets can be converted into 1-based line/column pairs.
pub fn read_source_document(path: &std::path::Path) -> Result<(DocumentMut, String), ConfigError> {
    let text = fs::read_to_string(path).map_err(|source_err| ConfigError::Io {
        path: path.to_path_buf(),
        source: source_err,
    })?;

    let doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: path.display().to_string(),
                message: e.to_string(),
                span: e.span(),
            },
        })?;

    Ok((doc, text))
}

pub fn merge_source(
    acc_doc: &mut DocumentMut,
    provenance: &mut ConfigProvenance,
    source: &ConfigSource,
    path: &std::path::Path,
) -> Result<(), ConfigError> {
    let (doc, text) = read_source_document(path)?;
    merge_document(acc_doc, provenance, source, &doc, &text);
    Ok(())
}

/// Merge an already-parsed source document (e.g. a sanitized in-memory copy)
/// into the accumulated candidate, recording provenance for every leaf.
pub fn merge_document(
    acc_doc: &mut DocumentMut,
    provenance: &mut ConfigProvenance,
    source: &ConfigSource,
    doc: &DocumentMut,
    text: &str,
) {
    merge_items(
        acc_doc.as_item_mut(),
        doc.as_item(),
        "",
        source,
        text,
        provenance,
    );
}

fn merge_items(
    acc: &mut Item,
    new_item: &Item,
    prefix: &str,
    source: &ConfigSource,
    text: &str,
    provenance: &mut ConfigProvenance,
) {
    if let (Item::Value(toml_edit::Value::InlineTable(acc_table)), Item::Table(new_table)) =
        (&*acc, new_item)
    {
        let converted = acc_table
            .iter()
            .map(|(key, value)| (key.to_owned(), value.clone()))
            .collect::<Vec<_>>();
        *acc = Item::Table(Table::new());
        if let Item::Table(acc_table) = acc {
            for (key, value) in converted {
                acc_table.insert(&key, Item::Value(value));
            }
            merge_tables(acc_table, new_table, prefix, source, text, provenance);
        }
        return;
    }
    if let (Item::Table(acc_table), Item::Value(toml_edit::Value::InlineTable(new_table))) =
        (&mut *acc, new_item)
    {
        let converted = new_table
            .iter()
            .map(|(key, value)| (key.to_owned(), value.clone()))
            .collect::<Vec<_>>();
        let converted_table =
            converted
                .into_iter()
                .fold(Table::new(), |mut table, (key, value)| {
                    table.insert(&key, Item::Value(value));
                    table
                });
        merge_tables(
            acc_table,
            &converted_table,
            prefix,
            source,
            text,
            provenance,
        );
        return;
    }
    match (acc, new_item) {
        (Item::Table(acc_table), Item::Table(new_table)) => {
            merge_tables(acc_table, new_table, prefix, source, text, provenance);
        }
        (
            Item::Value(toml_edit::Value::InlineTable(acc_table)),
            Item::Value(toml_edit::Value::InlineTable(new_table)),
        ) => {
            merge_inline_tables(acc_table, new_table, prefix, source, text, provenance);
        }
        (acc_item, new_val) => {
            // Non-table replaces table or scalar replaces scalar
            let offset = new_val.span().map(|r| r.start);
            let loc = if let Some(off) = offset {
                let (line, col) = line_col_from_offset(text, off);
                SourceLocation {
                    source: source.clone(),
                    line: Some(line),
                    column: Some(col),
                }
            } else {
                SourceLocation {
                    source: source.clone(),
                    line: None,
                    column: None,
                }
            };

            provenance.remove_prefix(prefix);
            *acc_item = new_val.clone();
            record_provenance_tree(new_val, prefix, &loc, provenance);
        }
    }
}

#[cfg(test)]
mod inline_table_regression {
    use super::*;

    #[test]
    fn empty_regular_table_preserves_inline_table_leaves_and_provenance() {
        let base: DocumentMut = "v = { a = false }".parse().unwrap();
        let overlay: DocumentMut = "[v]".parse().unwrap();
        let source = ConfigSource::Primary {
            path: "base.toml".into(),
        };
        let overlay_source = ConfigSource::Overrides {
            path: "overlay.toml".into(),
        };
        let mut provenance = ConfigProvenance::new();
        record_provenance_tree(
            base.as_item(),
            "",
            &SourceLocation {
                source: source.clone(),
                line: None,
                column: None,
            },
            &mut provenance,
        );
        let mut merged = base.clone();
        merge_document(
            &mut merged,
            &mut provenance,
            &overlay_source,
            &overlay,
            &overlay.to_string(),
        );
        assert_eq!(merged["v"]["a"].as_bool(), Some(false));
        assert_eq!(
            provenance.get("v.a").map(|location| &location.source),
            Some(&source)
        );
    }
}

fn merge_tables(
    acc_table: &mut Table,
    new_table: &Table,
    prefix: &str,
    source: &ConfigSource,
    text: &str,
    provenance: &mut ConfigProvenance,
) {
    for (key, new_val) in new_table.iter() {
        let subpath = format_key(prefix, key);
        let key_span = new_table
            .get_key_value(key)
            .and_then(|(k, _)| k.span())
            .map(|span| span.start..span.end);
        let val_span = new_val.span();
        let offset = key_span
            .or(val_span)
            .map(|r| r.start)
            .or_else(|| key_offset(text, key));

        let loc = if let Some(off) = offset {
            let (line, col) = line_col_from_offset(text, off);
            SourceLocation {
                source: source.clone(),
                line: Some(line),
                column: Some(col),
            }
        } else {
            SourceLocation {
                source: source.clone(),
                line: None,
                column: None,
            }
        };

        if let Some(acc_val) = acc_table.get_mut(key) {
            if is_table_like(acc_val) && is_table_like(new_val) {
                merge_items(acc_val, new_val, &subpath, source, text, provenance);
            } else {
                provenance.remove_prefix(&subpath);
                acc_table.insert(key, new_val.clone());
                record_provenance_tree(new_val, &subpath, &loc, provenance);
            }
        } else {
            provenance.remove_prefix(&subpath);
            acc_table.insert(key, new_val.clone());
            record_provenance_tree(new_val, &subpath, &loc, provenance);
        }
    }
}

fn merge_inline_tables(
    acc_table: &mut toml_edit::InlineTable,
    new_table: &toml_edit::InlineTable,
    prefix: &str,
    source: &ConfigSource,
    text: &str,
    provenance: &mut ConfigProvenance,
) {
    for (key, new_val) in new_table.iter() {
        let subpath = format_key(prefix, key);
        let val_span = new_val.span();
        let offset = val_span.map(|r| r.start);

        let loc = if let Some(off) = offset {
            let (line, col) = line_col_from_offset(text, off);
            SourceLocation {
                source: source.clone(),
                line: Some(line),
                column: Some(col),
            }
        } else {
            SourceLocation {
                source: source.clone(),
                line: None,
                column: None,
            }
        };

        provenance.remove_prefix(&subpath);
        let new_item = Item::Value(new_val.clone());
        acc_table.insert(key, new_val.clone());
        record_provenance_tree(&new_item, &subpath, &loc, provenance);
    }
}

fn is_table_like(item: &Item) -> bool {
    item.is_table() || matches!(item, Item::Value(toml_edit::Value::InlineTable(_)))
}

pub fn record_provenance_tree(
    item: &Item,
    prefix: &str,
    loc: &SourceLocation,
    provenance: &mut ConfigProvenance,
) {
    match item {
        Item::Table(table) => {
            for (k, v) in table.iter() {
                let subpath = format_key(prefix, k);
                record_provenance_tree(v, &subpath, loc, provenance);
            }
        }
        Item::Value(toml_edit::Value::InlineTable(table)) => {
            for (k, v) in table.iter() {
                let subpath = format_key(prefix, k);
                let val_item = Item::Value(v.clone());
                record_provenance_tree(&val_item, &subpath, loc, provenance);
            }
        }
        Item::Value(toml_edit::Value::Array(array)) => {
            if !prefix.is_empty() {
                provenance.set(prefix, loc.clone());
            }
            for (idx, elem) in array.iter().enumerate() {
                let subpath = format!("{prefix}[{idx}]");
                let elem_item = Item::Value(elem.clone());
                record_provenance_tree(&elem_item, &subpath, loc, provenance);
            }
        }
        Item::ArrayOfTables(array) => {
            if !prefix.is_empty() {
                provenance.set(prefix, loc.clone());
            }
            for (idx, table) in array.iter().enumerate() {
                let subpath = format!("{prefix}[{idx}]");
                let table_item = Item::Table(table.clone());
                record_provenance_tree(&table_item, &subpath, loc, provenance);
            }
        }
        _ => {
            if !prefix.is_empty() {
                provenance.set(prefix, loc.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::BTreeMap;
    use toml_edit::{DocumentMut, Item, Value};

    fn arb_key() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9_]{0,8}"
    }

    fn arb_scalar_value() -> impl Strategy<Value = Value> {
        prop_oneof![
            any::<bool>().prop_map(Value::from),
            (-1000i64..=1000i64).prop_map(Value::from),
            "[a-zA-Z0-9 _-]{0,20}".prop_map(Value::from),
        ]
    }

    fn arb_toml_item(depth: u32) -> impl Strategy<Value = Item> {
        if depth == 0 {
            arb_scalar_value().prop_map(Item::Value).boxed()
        } else {
            prop_oneof![
                arb_scalar_value().prop_map(Item::Value),
                prop::collection::vec(arb_scalar_value(), 0..=3)
                    .prop_map(|v| Item::Value(Value::Array(v.into_iter().collect()))),
                prop::collection::btree_map(arb_key(), arb_scalar_value(), 0..=3).prop_map(|map| {
                    let mut table = toml_edit::InlineTable::new();
                    for (key, value) in map {
                        table.insert(&key, value);
                    }
                    Item::Value(Value::InlineTable(table))
                }),
                arb_table_item(depth - 1),
            ]
            .boxed()
        }
    }

    fn arb_table_item(depth: u32) -> impl Strategy<Value = Item> {
        prop::collection::btree_map(arb_key(), arb_toml_item(depth - 1), 0..=3).prop_map(|map| {
            let mut table = toml_edit::Table::new();
            for (k, v) in map {
                table.insert(&k, v);
            }
            Item::Table(table)
        })
    }

    fn arb_toml_doc() -> impl Strategy<Value = DocumentMut> {
        prop::collection::btree_map(arb_key(), arb_toml_item(2), 0..=4).prop_map(|map| {
            let mut doc = DocumentMut::new();
            for (k, v) in map {
                doc.insert(&k, v);
            }
            doc
        })
    }

    fn doc_to_toml_val(doc: &DocumentMut) -> toml::Value {
        let text = doc.to_string();
        if text.trim().is_empty() {
            toml::Value::Table(toml::Table::new())
        } else {
            match text.parse::<toml::Table>() {
                Ok(table) => toml::Value::Table(table),
                Err(e) => panic!(
                    "failed to parse doc string into toml::Table:\n---\n{text}\n---\nError: {e}"
                ),
            }
        }
    }

    fn do_merge(
        base: &DocumentMut,
        overlay: &DocumentMut,
        base_src: &ConfigSource,
        overlay_src: &ConfigSource,
    ) -> (DocumentMut, ConfigProvenance) {
        let mut acc = base.clone();
        let mut prov = ConfigProvenance::new();
        let defaults_loc = SourceLocation {
            source: base_src.clone(),
            line: None,
            column: None,
        };
        record_provenance_tree(acc.as_item(), "", &defaults_loc, &mut prov);

        let overlay_text = overlay.to_string();
        merge_document(&mut acc, &mut prov, overlay_src, overlay, &overlay_text);
        (acc, prov)
    }

    fn remove_owned_prefix(owners: &mut BTreeMap<String, ConfigSource>, prefix: &str) {
        let dotted = format!("{prefix}.");
        let indexed = format!("{prefix}[");
        owners.retain(|path, _| {
            path != prefix && !path.starts_with(&dotted) && !path.starts_with(&indexed)
        });
    }

    fn record_reference_owners(
        value: &toml::Value,
        prefix: &str,
        source: &ConfigSource,
        owners: &mut BTreeMap<String, ConfigSource>,
    ) {
        match value {
            toml::Value::Table(table) => {
                for (key, child) in table {
                    let path = format_key(prefix, key);
                    record_reference_owners(child, &path, source, owners);
                }
            }
            toml::Value::Array(values) => {
                if !prefix.is_empty() {
                    owners.insert(prefix.to_owned(), source.clone());
                }
                for (index, child) in values.iter().enumerate() {
                    record_reference_owners(child, &format!("{prefix}[{index}]"), source, owners);
                }
            }
            _ if !prefix.is_empty() => {
                owners.insert(prefix.to_owned(), source.clone());
            }
            _ => {}
        }
    }

    fn reference_merge(
        base: &mut toml::Value,
        overlay: &toml::Value,
        prefix: &str,
        source: &ConfigSource,
        owners: &mut BTreeMap<String, ConfigSource>,
    ) {
        match (base, overlay) {
            (toml::Value::Table(base), toml::Value::Table(overlay)) => {
                for (key, value) in overlay {
                    let path = format_key(prefix, key);
                    if let Some(current) = base.get_mut(key) {
                        reference_merge(current, value, &path, source, owners);
                    } else {
                        base.insert(key.clone(), value.clone());
                        remove_owned_prefix(owners, &path);
                        record_reference_owners(value, &path, source, owners);
                    }
                }
            }
            (base, overlay) => {
                *base = overlay.clone();
                remove_owned_prefix(owners, prefix);
                record_reference_owners(overlay, prefix, source, owners);
            }
        }
    }

    proptest! {
        #[test]
        fn test_toml_semantic_round_trip_prop(doc in arb_toml_doc()) {
            let text1 = doc.to_string();
            let parsed1: DocumentMut = text1.parse().expect("parse text1");
            let text2 = parsed1.to_string();
            let parsed2: DocumentMut = text2.parse().expect("parse text2");

            let val1 = doc_to_toml_val(&parsed1);
            let val2 = doc_to_toml_val(&parsed2);
            prop_assert_eq!(val1, val2);
        }

        #[test]
        fn test_merge_empty_right_identity_prop(doc in arb_toml_doc()) {
            let empty_doc = DocumentMut::new();
            let src1 = ConfigSource::Primary { path: "primary.toml".into() };
            let src2 = ConfigSource::Overrides { path: "override.toml".into() };

            let (merged, _prov) = do_merge(&doc, &empty_doc, &src1, &src2);
            prop_assert_eq!(doc_to_toml_val(&merged), doc_to_toml_val(&doc));
        }

        #[test]
        fn test_merge_right_biased_replacement_prop(
            base in arb_toml_doc(),
            overlay in arb_toml_doc(),
        ) {
            let src1 = ConfigSource::Primary { path: "primary.toml".into() };
            let src2 = ConfigSource::Overrides { path: "override.toml".into() };

            let (merged, _prov) = do_merge(&base, &overlay, &src1, &src2);

            for (key, overlay_item) in overlay.as_table().iter() {
                if let Some(_base_item) = base.get(key).filter(|b| !is_table_like(b) || !is_table_like(overlay_item)) {
                    let m_item = merged.get(key).expect("key present in merged");
                    let m_str = m_item.to_string();
                    let o_str = overlay_item.to_string();
                    prop_assert_eq!(m_str.trim(), o_str.trim());
                }
            }
        }

        #[test]
        fn test_merge_disjoint_keys_retained_prop(
            doc1 in arb_toml_doc(),
            doc2 in arb_toml_doc(),
        ) {
            let src1 = ConfigSource::Primary { path: "primary.toml".into() };
            let src2 = ConfigSource::Overrides { path: "override.toml".into() };

            let (merged, _) = do_merge(&doc1, &doc2, &src1, &src2);

            for (key, _) in doc1.as_table().iter() {
                prop_assert!(merged.contains_key(key));
            }
            for (key, _) in doc2.as_table().iter() {
                prop_assert!(merged.contains_key(key));
            }
        }

        #[test]
        fn test_merge_associativity_prop(
            doc_a in arb_toml_doc(),
            doc_b in arb_toml_doc(),
            doc_c in arb_toml_doc(),
        ) {
            let src_a = ConfigSource::Primary { path: "a.toml".into() };
            let src_b = ConfigSource::Fragment { path: "b.toml".into() };
            let src_c = ConfigSource::Overrides { path: "c.toml".into() };

            let (ab, _) = do_merge(&doc_a, &doc_b, &src_a, &src_b);
            let (abc1, _) = do_merge(&ab, &doc_c, &src_b, &src_c);

            let (bc, _) = do_merge(&doc_b, &doc_c, &src_b, &src_c);
            let (abc2, _) = do_merge(&doc_a, &bc, &src_a, &src_c);

            let val1 = doc_to_toml_val(&abc1);
            let val2 = doc_to_toml_val(&abc2);
            prop_assert_eq!(val1, val2);
        }

        #[test]
        fn test_provenance_points_to_latest_source_and_no_stale_ancestors_prop(
            base in arb_toml_doc(),
            overlay in arb_toml_doc(),
        ) {
            let src1 = ConfigSource::Primary { path: "primary.toml".into() };
            let src2 = ConfigSource::Overrides { path: "override.toml".into() };

            let (merged, prov) = do_merge(&base, &overlay, &src1, &src2);

            let mut expected = doc_to_toml_val(&base);
            let overlay_value = doc_to_toml_val(&overlay);
            let mut expected_owners = BTreeMap::new();
            record_reference_owners(&expected, "", &src1, &mut expected_owners);
            reference_merge(&mut expected, &overlay_value, "", &src2, &mut expected_owners);

            let actual_owners = prov
                .map
                .iter()
                .map(|(path, location)| (path.clone(), location.source.clone()))
                .collect::<BTreeMap<_, _>>();
            prop_assert_eq!(actual_owners, expected_owners);
            prop_assert_eq!(doc_to_toml_val(&merged), expected);
        }
    }
}
