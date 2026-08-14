//! Tree flattening and canonical ViewTree assembly.

use crate::bindings::shilpo::extension::view::{
    BadgeNode, ButtonNode, ContainerNode, IconButtonNode, IconNode, ImageNode, ListNode,
    LoadingIndicatorNode, ProgressNode, SliderNode, SpacerNode, TextInputNode, TextNode,
    ToggleNode, ViewNode, ViewTree,
};
use crate::builder::nodes::{IntoViewNode, NodeSpec};

/// Flattens a root node specification tree into a canonical indexed [`ViewTree`].
///
/// Preserves child order deterministically, sets the root index to 0, and
/// populates child index arrays for container and list nodes.
///
/// # Examples
///
/// ```rust
/// use shilpo_ext_sdk::prelude::*;
///
/// let tree = build_view_tree(
///     row()
///         .gap(8.0)
///         .child(icon("star").size(16.0))
///         .child(text("Favorite"))
/// );
///
/// assert_eq!(tree.root, 0);
/// assert_eq!(tree.nodes.len(), 3);
/// ```
pub fn build_view_tree(root: impl IntoViewNode) -> ViewTree {
    let mut nodes = Vec::new();
    let root_spec = root.into_node_spec();
    flatten_node_spec(root_spec, &mut nodes);
    ViewTree { nodes, root: 0 }
}

fn flatten_node_spec(spec: NodeSpec, nodes: &mut Vec<ViewNode>) -> u32 {
    let current_index = nodes.len() as u32;

    match spec {
        NodeSpec::Container(c) => {
            nodes.push(ViewNode::Divider);
            let mut child_indices = Vec::with_capacity(c.children.len());
            for child in c.children {
                child_indices.push(flatten_node_spec(child, nodes));
            }
            nodes[current_index as usize] = ViewNode::Container(ContainerNode {
                direction: c.direction,
                children: child_indices,
                style: c.style,
                gap: c.gap,
                align_items: c.align_items,
                justify_content: c.justify_content,
                wrap: c.wrap,
                event_id: c.event_id,
            });
        }
        NodeSpec::List(l) => {
            nodes.push(ViewNode::Divider);
            let mut item_indices = Vec::with_capacity(l.items.len());
            for item in l.items {
                item_indices.push(flatten_node_spec(item, nodes));
            }
            nodes[current_index as usize] = ViewNode::List(ListNode {
                items: item_indices,
                style: l.style,
            });
        }
        NodeSpec::Text(t) => {
            nodes.push(ViewNode::Text(TextNode {
                content: t.content,
                font_size: t.font_size,
                bold: t.bold,
                style: t.style,
            }));
        }
        NodeSpec::Icon(i) => {
            nodes.push(ViewNode::Icon(IconNode {
                name: i.name,
                size: i.size,
                style: i.style,
            }));
        }
        NodeSpec::Image(im) => {
            nodes.push(ViewNode::Image(ImageNode {
                asset_path: im.asset_path,
                width: im.width,
                height: im.height,
                style: im.style,
            }));
        }
        NodeSpec::Button(b) => {
            nodes.push(ViewNode::Button(ButtonNode {
                label: b.label,
                event_id: b.event_id,
                style: b.style,
            }));
        }
        NodeSpec::IconButton(ib) => {
            nodes.push(ViewNode::IconButton(IconButtonNode {
                icon_name: ib.icon_name,
                event_id: ib.event_id,
                style: ib.style,
            }));
        }
        NodeSpec::Toggle(tog) => {
            nodes.push(ViewNode::Toggle(ToggleNode {
                value: tog.value,
                event_id: tog.event_id,
                style: tog.style,
            }));
        }
        NodeSpec::Slider(s) => {
            nodes.push(ViewNode::Slider(SliderNode {
                value: s.value,
                min: s.min,
                max: s.max,
                event_id: s.event_id,
                style: s.style,
            }));
        }
        NodeSpec::TextInput(ti) => {
            nodes.push(ViewNode::TextInput(TextInputNode {
                placeholder: ti.placeholder,
                value: ti.value,
                event_id: ti.event_id,
                style: ti.style,
            }));
        }
        NodeSpec::Spacer(sp) => {
            nodes.push(ViewNode::Spacer(SpacerNode { size: sp.size }));
        }
        NodeSpec::Divider => {
            nodes.push(ViewNode::Divider);
        }
        NodeSpec::Badge(bg) => {
            nodes.push(ViewNode::Badge(BadgeNode {
                label: bg.label,
                style: bg.style,
            }));
        }
        NodeSpec::Progress(p) => {
            nodes.push(ViewNode::Progress(ProgressNode {
                value: p.value,
                style: p.style,
            }));
        }
        NodeSpec::LoadingIndicator(li) => {
            nodes.push(ViewNode::LoadingIndicator(LoadingIndicatorNode {
                size: li.size,
                color: li.color,
                style: li.style,
            }));
        }
    }

    current_index
}
