use crate::runtime::ShellRuntime;
use gpui::{
    AnyElement, App, AvailableSpace, IntoElement, ParentElement, Pixels, Size, Styled, Window, div,
    px,
};
use shilpo_ext_api::CanonicalId;
#[cfg(test)]
use shilpo_ext_api::{ViewNode, ViewStyle, ViewTree};

use super::{
    model::{CardCapabilities, CardChannel, CardOwnerId, CardSourceId},
    provider::CardProvider,
};

pub(crate) struct ExtensionMenuCardProvider {
    pub owner_id: CardOwnerId,
    pub menu_canonical_id: CanonicalId,
    pub _bar_widget_canonical_id: CanonicalId,
    measured_size: std::sync::Arc<std::sync::Mutex<Option<Size<Pixels>>>>,
}

impl ExtensionMenuCardProvider {
    pub fn new(menu_canonical_id: CanonicalId, bar_widget_canonical_id: CanonicalId) -> Self {
        let owner_id = CardOwnerId::new(menu_canonical_id.to_string());
        Self {
            owner_id,
            menu_canonical_id,
            _bar_widget_canonical_id: bar_widget_canonical_id,
            measured_size: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl CardProvider for ExtensionMenuCardProvider {
    fn owner_id(&self) -> CardOwnerId {
        self.owner_id.clone()
    }

    fn capabilities(&self) -> CardCapabilities {
        CardCapabilities {
            hover: false,
            click: true,
        }
    }

    fn source_available(&self, _source: &CardSourceId, cx: &App) -> bool {
        ShellRuntime::extension_view(cx, &self.menu_canonical_id).is_some()
    }

    fn preferred_size(
        &self,
        _channel: CardChannel,
        _source: &CardSourceId,
        cx: &App,
    ) -> Size<Pixels> {
        self.measured_size
            .lock()
            .expect("extension menu measurement lock is not poisoned")
            .unwrap_or_else(|| Size {
                width: cx
                    .primary_display()
                    .map(|display| display.bounds().size.width)
                    .unwrap_or(px(1.0)),
                height: cx
                    .primary_display()
                    .map(|display| display.bounds().size.height)
                    .unwrap_or(px(1.0)),
            })
    }

    fn render_content(
        &self,
        _channel: CardChannel,
        source: &CardSourceId,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let Some(tree) = ShellRuntime::extension_view(cx, &self.menu_canonical_id) else {
            return div().into_any_element();
        };

        let mut element = crate::shell::bar::ext_view_adapter::render_ext_view_tree(
            &self.menu_canonical_id,
            Some(&source.instance_id),
            &tree,
            window,
            cx,
        );

        let intrinsic = element.layout_as_root(AvailableSpace::min_size(), window, cx);
        let max_width = (window.bounds().size.width - px(32.0)).max(px(1.0));
        let max_height = (window.bounds().size.height - px(32.0)).max(px(1.0));
        let mut container = div().p_4().max_w(max_width).max_h(max_height);
        if intrinsic.width > max_width {
            container.style().overflow.x = Some(gpui::Overflow::Scroll);
        }
        if intrinsic.height > max_height {
            container.style().overflow.y = Some(gpui::Overflow::Scroll);
        }
        let content = container.child(element).into_any_element();
        let measured = Size {
            width: intrinsic.width.min(max_width) + px(32.0),
            height: intrinsic.height.min(max_height) + px(32.0),
        };
        let mut cached = self
            .measured_size
            .lock()
            .expect("extension menu measurement lock is not poisoned");
        if *cached != Some(measured) {
            *cached = Some(measured);
            let source = source.clone();
            window.defer(cx, move |_, cx| {
                super::adapter::CardCoordinator::dispatch(
                    cx,
                    super::model::CardRequest::Reposition { source },
                );
            });
        }
        content.into_any_element()
    }
}

#[cfg(test)]
pub fn measure_view_tree_intrinsic(tree: &ViewTree, font_size: f32) -> Size<Pixels> {
    measure_view_node_intrinsic(&tree.root, font_size)
}

#[cfg(test)]
pub fn measure_view_node_intrinsic(node: &ViewNode, font_size: f32) -> Size<Pixels> {
    use shilpo_ext_api::*;
    match node {
        ViewNode::Text(t) => {
            let fs = t.font_size.unwrap_or(font_size);
            let char_w = fs * 0.55;
            let line_h = fs * 1.35;
            let lines: Vec<&str> = t.content.lines().collect();
            let max_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let num_lines = lines.len().max(1);
            let width = max_chars as f32 * char_w;
            let height = num_lines as f32 * line_h;
            apply_style_constraints(
                Size {
                    width: px(width),
                    height: px(height),
                },
                t.style.as_ref(),
            )
        }
        ViewNode::Icon(i) => {
            let sz = i.size.unwrap_or(16.0);
            apply_style_constraints(
                Size {
                    width: px(sz),
                    height: px(sz),
                },
                i.style.as_ref(),
            )
        }
        ViewNode::Image(img) => {
            let w = img.width.unwrap_or(48.0);
            let h = img.height.unwrap_or(48.0);
            apply_style_constraints(
                Size {
                    width: px(w),
                    height: px(h),
                },
                img.style.as_ref(),
            )
        }
        ViewNode::Button(btn) => {
            let char_w = font_size * 0.55;
            let label_w = btn.label.chars().count() as f32 * char_w;
            let w = label_w + 24.0;
            let h = font_size * 1.35 + 8.0;
            apply_style_constraints(
                Size {
                    width: px(w),
                    height: px(h),
                },
                btn.style.as_ref(),
            )
        }
        ViewNode::IconButton(ibtn) => {
            let w = 32.0;
            let h = 32.0;
            apply_style_constraints(
                Size {
                    width: px(w),
                    height: px(h),
                },
                ibtn.style.as_ref(),
            )
        }
        ViewNode::Toggle(t) => {
            let w = 36.0;
            let h = 20.0;
            apply_style_constraints(
                Size {
                    width: px(w),
                    height: px(h),
                },
                t.style.as_ref(),
            )
        }
        ViewNode::Slider(s) => {
            let w = s.style.as_ref().and_then(|st| st.width).unwrap_or(180.0);
            let h = 24.0;
            apply_style_constraints(
                Size {
                    width: px(w),
                    height: px(h),
                },
                s.style.as_ref(),
            )
        }
        ViewNode::TextInput(i) => {
            let p_len = i.placeholder.as_deref().map_or(0, str::len);
            let v_len = i.value.len();
            let len = p_len.max(v_len).max(10);
            let w = (len as f32 * (font_size * 0.55) + 24.0).max(160.0);
            let h = 32.0;
            apply_style_constraints(
                Size {
                    width: px(w),
                    height: px(h),
                },
                i.style.as_ref(),
            )
        }
        ViewNode::Badge(b) => {
            let char_w = font_size * 0.5;
            let w = b.label.chars().count() as f32 * char_w + 12.0;
            let h = font_size * 1.2;
            apply_style_constraints(
                Size {
                    width: px(w),
                    height: px(h),
                },
                b.style.as_ref(),
            )
        }
        ViewNode::Progress(p) => {
            let w = p.style.as_ref().and_then(|st| st.width).unwrap_or(160.0);
            let h = 8.0;
            apply_style_constraints(
                Size {
                    width: px(w),
                    height: px(h),
                },
                p.style.as_ref(),
            )
        }
        ViewNode::LoadingIndicator(_) => Size {
            width: px(24.0),
            height: px(24.0),
        },
        ViewNode::Spacer(s) => {
            let w = s.size.unwrap_or(8.0);
            let h = s.size.unwrap_or(8.0);
            Size {
                width: px(w),
                height: px(h),
            }
        }
        ViewNode::Divider => Size {
            width: px(100.0),
            height: px(1.0),
        },
        ViewNode::List(l) => {
            let mut sum_h = 0.0f32;
            let mut max_w = 0.0f32;
            for child in &l.items {
                let sz = measure_view_node_intrinsic(child, font_size);
                sum_h += sz.height.as_f32();
                max_w = max_w.max(sz.width.as_f32());
            }
            apply_style_constraints(
                Size {
                    width: px(max_w),
                    height: px(sum_h),
                },
                l.style.as_ref(),
            )
        }
        ViewNode::Container(c) => {
            let padding = c.style.as_ref().and_then(|s| s.padding).unwrap_or(0.0);
            let margin = c.style.as_ref().and_then(|s| s.margin).unwrap_or(0.0);
            let gap = c.gap.unwrap_or(0.0);

            let mut child_sizes = Vec::with_capacity(c.children.len());
            for child in &c.children {
                child_sizes.push(measure_view_node_intrinsic(child, font_size));
            }

            let (calc_w, calc_h) = match c.direction {
                ContainerDirection::Row => {
                    let mut sum_w = 0.0f32;
                    let mut max_h = 0.0f32;
                    for (idx, sz) in child_sizes.iter().enumerate() {
                        sum_w += sz.width.as_f32();
                        if idx > 0 {
                            sum_w += gap;
                        }
                        max_h = max_h.max(sz.height.as_f32());
                    }
                    (sum_w, max_h)
                }
                ContainerDirection::Column => {
                    let mut max_w = 0.0f32;
                    let mut sum_h = 0.0f32;
                    for (idx, sz) in child_sizes.iter().enumerate() {
                        max_w = max_w.max(sz.width.as_f32());
                        sum_h += sz.height.as_f32();
                        if idx > 0 {
                            sum_h += gap;
                        }
                    }
                    (max_w, sum_h)
                }
                ContainerDirection::Stack => {
                    let mut max_w = 0.0f32;
                    let mut max_h = 0.0f32;
                    for sz in &child_sizes {
                        max_w = max_w.max(sz.width.as_f32());
                        max_h = max_h.max(sz.height.as_f32());
                    }
                    (max_w, max_h)
                }
                ContainerDirection::Grid { columns } => {
                    let cols = columns.max(1) as usize;
                    let num_children = child_sizes.len();
                    let rows = (num_children + cols - 1) / cols.max(1);
                    let mut col_widths = vec![0.0f32; cols];
                    let mut row_heights = vec![0.0f32; rows.max(1)];
                    for (idx, sz) in child_sizes.iter().enumerate() {
                        let c_idx = idx % cols;
                        let r_idx = idx / cols;
                        col_widths[c_idx] = col_widths[c_idx].max(sz.width.as_f32());
                        row_heights[r_idx] = row_heights[r_idx].max(sz.height.as_f32());
                    }
                    let total_w: f32 =
                        col_widths.iter().sum::<f32>() + gap * (cols.saturating_sub(1) as f32);
                    let total_h: f32 =
                        row_heights.iter().sum::<f32>() + gap * (rows.saturating_sub(1) as f32);
                    (total_w, total_h)
                }
            };

            let content_w = calc_w + 2.0 * padding + 2.0 * margin;
            let content_h = calc_h + 2.0 * padding + 2.0 * margin;

            apply_style_constraints(
                Size {
                    width: px(content_w),
                    height: px(content_h),
                },
                c.style.as_ref(),
            )
        }
    }
}

#[cfg(test)]
fn apply_style_constraints(size: Size<Pixels>, style: Option<&ViewStyle>) -> Size<Pixels> {
    let mut w = size.width.as_f32();
    let mut h = size.height.as_f32();
    if let Some(s) = style {
        if let Some(sw) = s.width {
            w = sw;
        }
        if let Some(sh) = s.height {
            h = sh;
        }
        if let Some(min_w) = s.min_width {
            w = w.max(min_w);
        }
        if let Some(max_w) = s.max_width {
            w = w.min(max_w);
        }
        if let Some(min_h) = s.min_height {
            h = h.max(min_h);
        }
        if let Some(max_h) = s.max_height {
            h = h.min(max_h);
        }
        if let Some(pad) = s.padding {
            if s.width.is_none() {
                w += 2.0 * pad;
            }
            if s.height.is_none() {
                h += 2.0 * pad;
            }
        }
        if let Some(mar) = s.margin {
            if s.width.is_none() {
                w += 2.0 * mar;
            }
            if s.height.is_none() {
                h += 2.0 * mar;
            }
        }
    }
    Size {
        width: px(w),
        height: px(h),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_ext_api::*;

    #[test]
    fn test_intrinsic_measurement_basic() {
        let tree = ViewTree::new(ViewNode::Container(ContainerNode {
            direction: ContainerDirection::Column,
            children: vec![
                ViewNode::Text(TextNode {
                    content: "Hello World".into(),
                    style: None,
                    font_size: Some(16.0),
                    bold: Some(true),
                }),
                ViewNode::Button(ButtonNode {
                    label: "Click Me".into(),
                    event_id: "btn-click".into(),
                    style: None,
                }),
            ],
            style: Some(ViewStyle {
                padding: Some(8.0),
                ..Default::default()
            }),
            gap: Some(4.0),
            align_items: None,
            justify_content: None,
            wrap: false,
            event_id: None,
        }));

        let size = measure_view_tree_intrinsic(&tree, 14.0);
        assert!(size.width.as_f32() > 0.0);
        assert!(size.height.as_f32() > 0.0);
    }

    #[test]
    fn test_card_provider_capabilities() {
        let ext_id = ExtensionId::new("io.github.test.weather").unwrap();
        let menu_id =
            CanonicalId::new(ext_id.clone(), ContributionId::new("weather-menu").unwrap());
        let widget_id = CanonicalId::new(ext_id, ContributionId::new("weather").unwrap());
        let provider = ExtensionMenuCardProvider::new(menu_id, widget_id.clone());
        assert_eq!(provider._bar_widget_canonical_id, widget_id);
        let caps = provider.capabilities();
        assert!(!caps.hover);
        assert!(caps.click);
    }
}
