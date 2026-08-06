use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt};
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};

pub fn get_cat_icon(frame: usize) -> IconName {
    match frame % 5 {
        0 => IconName::Cat0,
        1 => IconName::Cat1,
        2 => IconName::Cat2,
        3 => IconName::Cat3,
        4 => IconName::Cat4,
        _ => IconName::Cat0,
    }
}

/// RunCat-animated system monitor widget for Shilpo UI.
#[derive(IntoElement)]
pub struct SysInfoWidget {
    id: ElementId,
    frame_index: usize,
    #[allow(dead_code)]
    cpu_percent: u8,
    #[allow(dead_code)]
    ram_percent: u8,
    style: StyleRefinement,
}

impl SysInfoWidget {
    pub fn new(
        id: impl Into<ElementId>,
        frame_index: usize,
        cpu_percent: u8,
        ram_percent: u8,
    ) -> Self {
        Self {
            id: id.into(),
            frame_index,
            cpu_percent,
            ram_percent,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for SysInfoWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SysInfoWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let cat_icon = get_cat_icon(self.frame_index);

        div()
            .id(self.id)
            .px_2()
            .py_0p5()
            .rounded_full()
            .bg(cx.theme().surface_container_high)
            .text_color(cx.theme().on_surface)
            .flex()
            .items_center()
            .justify_center()
            .child(Icon::new(cat_icon).size(px(24.)))
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_cat_icon_cycling() {
        assert_eq!(get_cat_icon(0), IconName::Cat0);
        assert_eq!(get_cat_icon(1), IconName::Cat1);
        assert_eq!(get_cat_icon(2), IconName::Cat2);
        assert_eq!(get_cat_icon(3), IconName::Cat3);
        assert_eq!(get_cat_icon(4), IconName::Cat4);
        assert_eq!(get_cat_icon(5), IconName::Cat0);
    }
}
