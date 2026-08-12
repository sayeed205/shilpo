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

fn key_offset(text: &str, key: &str) -> Option<usize> {
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

pub fn merge_source(
    acc_doc: &mut DocumentMut,
    provenance: &mut ConfigProvenance,
    source: &ConfigSource,
    path: &std::path::Path,
) -> Result<(), ConfigError> {
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

    merge_items(
        acc_doc.as_item_mut(),
        doc.as_item(),
        "",
        source,
        &text,
        provenance,
    );

    Ok(())
}

fn merge_items(
    acc: &mut Item,
    new_item: &Item,
    prefix: &str,
    source: &ConfigSource,
    text: &str,
    provenance: &mut ConfigProvenance,
) {
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
