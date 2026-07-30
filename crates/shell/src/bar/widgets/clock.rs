use chrono::{
    DateTime, Local,
    format::{Item, StrftimeItems},
};
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_ui::{ActiveTheme, StyledExt};
use std::sync::Arc;

pub fn format_clock(now: &DateTime<Local>, fmt: Option<&str>) -> String {
    if let Some(formatted) = fmt
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|trimmed| !StrftimeItems::new(trimmed).any(|item| matches!(item, Item::Error)))
        .map(|trimmed| now.format(trimmed).to_string())
    {
        return formatted;
    }
    now.format("%H:%M").to_string()
}

pub fn format_date(now: &DateTime<Local>) -> String {
    now.format("%a, %d %b").to_string()
}

/// Primary time pill widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct ClockWidget {
    id: ElementId,
    time_str: String,
    style: StyleRefinement,
}

impl ClockWidget {
    pub fn new(id: impl Into<ElementId>, time_str: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            time_str: time_str.into(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for ClockWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ClockWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .h(px(24.))
            .px(px(8.))
            .rounded_full()
            .bg(cx.theme().primary)
            .text_color(cx.theme().on_primary)
            .text_size(px(13.))
            .font_features(gpui::FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
            .font_bold()
            .flex()
            .items_center()
            .justify_center()
            .refine_style(&self.style)
            .child(self.time_str)
    }
}

/// Secondary date text widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct DateWidget {
    id: ElementId,
    date_str: String,
    style: StyleRefinement,
}

impl DateWidget {
    pub fn new(id: impl Into<ElementId>, date_str: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            date_str: date_str.into(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for DateWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for DateWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .px(px(4.))
            .text_color(cx.theme().on_surface_variant)
            .text_size(px(12.))
            .font_medium()
            .flex()
            .items_center()
            .justify_center()
            .refine_style(&self.style)
            .child(self.date_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_format_clock_and_date_defaults() {
        let naive = NaiveDate::from_ymd_opt(2026, 7, 30)
            .unwrap()
            .and_hms_opt(17, 53, 0)
            .unwrap();
        let dt = naive.and_local_timezone(Local).unwrap();
        assert_eq!(format_clock(&dt, None), "17:53");
        assert_eq!(format_date(&dt), "Thu, 30 Jul");
    }

    #[test]
    fn test_custom_and_fallback_clock_formatting() {
        let naive = NaiveDate::from_ymd_opt(2026, 7, 30)
            .unwrap()
            .and_hms_opt(17, 53, 0)
            .unwrap();
        let dt = naive.and_local_timezone(Local).unwrap();
        assert_eq!(format_clock(&dt, Some("%I:%M %p")), "05:53 PM");
        assert_eq!(format_clock(&dt, Some("%Q")), "17:53");
        assert_eq!(format_clock(&dt, Some("")), "17:53");
        assert_eq!(format_clock(&dt, Some("   ")), "17:53");
    }
}
