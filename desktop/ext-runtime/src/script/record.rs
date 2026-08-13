use serde::{Deserialize, Serialize};
use shilpo_ext_api::{
    Alignment, ContainerDirection, ContainerNode, ContributionId, IconNode, TextNode, ViewLimits,
    ViewNode, ViewTree,
};

use super::manifest::ScriptManifest;

pub const MAX_RECORD_BYTES: usize = 1_048_576; // 1 MiB

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScriptRecord {
    pub schema_version: u32,
    pub contribution: ContributionId,
    #[serde(flatten)]
    pub payload: ScriptRecordPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScriptRecordPayload {
    View {
        view: ViewTree,
    },
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tooltip: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
    },
}

pub fn decode_and_validate_record(
    json_bytes: &[u8],
    manifest: &ScriptManifest,
) -> Result<(ContributionId, ViewTree), String> {
    if json_bytes.len() > MAX_RECORD_BYTES {
        return Err(format!(
            "record size ({} bytes) exceeds the 1 MiB limit",
            json_bytes.len()
        ));
    }

    let json_str = std::str::from_utf8(json_bytes)
        .map_err(|e| format!("invalid UTF-8 in script record: {e}"))?;

    let record = serde_json::from_str::<ScriptRecord>(json_str)
        .map_err(|e| format!("invalid JSON script record: {e}"))?;

    if record.schema_version != 1 {
        return Err(format!(
            "unsupported record schema_version {}; expected 1",
            record.schema_version
        ));
    }

    let valid_contrib = manifest
        .contributions
        .bar_widgets
        .iter()
        .any(|w| w.id == record.contribution);
    if !valid_contrib {
        return Err(format!(
            "unknown contribution ID '{}' in script record",
            record.contribution
        ));
    }

    let view_tree = match record.payload {
        ScriptRecordPayload::View { view } => view,
        ScriptRecordPayload::Text { text, icon, .. } => {
            let root = if let Some(icon_name) = icon {
                if icon_name.trim().is_empty() {
                    return Err("icon name cannot be empty".into());
                }
                ViewNode::Container(ContainerNode {
                    direction: ContainerDirection::Row,
                    children: vec![
                        ViewNode::Icon(IconNode {
                            name: icon_name,
                            size: None,
                            style: None,
                        }),
                        ViewNode::Text(TextNode {
                            content: text,
                            font_size: None,
                            bold: None,
                            style: None,
                        }),
                    ],
                    style: None,
                    gap: Some(6.0),
                    align_items: Some(Alignment::Center),
                    justify_content: None,
                    wrap: false,
                    event_id: None,
                })
            } else {
                ViewNode::Text(TextNode {
                    content: text,
                    font_size: None,
                    bold: None,
                    style: None,
                })
            };
            ViewTree::new(root)
        }
    };

    view_tree
        .validate_read_only(ViewLimits::default())
        .map_err(|e| format!("invalid view tree in script record: {e}"))?;

    Ok((record.contribution, view_tree))
}
