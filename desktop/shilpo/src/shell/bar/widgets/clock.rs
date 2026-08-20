use std::sync::Arc;

use chrono::{
    DateTime, Local,
    format::{Item, StrftimeItems},
};
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_m3e::{ActiveTheme, StyledExt};

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
    vertical: bool,
    style: StyleRefinement,
}

impl ClockWidget {
    pub fn new(id: impl Into<ElementId>, time_str: impl Into<String>, vertical: bool) -> Self {
        Self {
            id: id.into(),
            time_str: time_str.into(),
            vertical,
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
        if !self.vertical {
            return div()
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
                .child(self.time_str);
        }

        let mut time_lines = Vec::new();
        if self.time_str.contains(':') {
            for part in self.time_str.split(':') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    time_lines.push(trimmed.to_string());
                }
            }
        } else if self.time_str.contains(' ') {
            for part in self.time_str.split_whitespace() {
                time_lines.push(part.to_string());
            }
        } else {
            time_lines.push(self.time_str.clone());
        }

        div()
            .id(self.id)
            .w(px(26.))
            .py(px(4.))
            .px(px(2.))
            .rounded_full()
            .bg(cx.theme().primary)
            .text_color(cx.theme().on_primary)
            .text_size(px(11.))
            .line_height(px(12.))
            .font_features(gpui::FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
            .font_bold()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .refine_style(&self.style)
            .children(time_lines.into_iter().map(|line| div().child(line)))
    }
}

/// Secondary date text widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct DateWidget {
    id: ElementId,
    date_str: String,
    vertical: bool,
    style: StyleRefinement,
}

impl DateWidget {
    pub fn new(id: impl Into<ElementId>, date_str: impl Into<String>, vertical: bool) -> Self {
        Self {
            id: id.into(),
            date_str: date_str.into(),
            vertical,
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
        if !self.vertical {
            return div()
                .id(self.id)
                .px(px(4.))
                .text_color(cx.theme().on_surface_variant)
                .text_size(px(12.))
                .font_medium()
                .flex()
                .items_center()
                .justify_center()
                .refine_style(&self.style)
                .child(self.date_str);
        }

        let date_parts: Vec<String> = self
            .date_str
            .split([',', ' '])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();

        div()
            .id(self.id)
            .w(px(26.))
            .py(px(2.))
            .text_color(cx.theme().on_surface_variant)
            .text_size(px(10.))
            .line_height(px(11.))
            .font_bold()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .refine_style(&self.style)
            .children(date_parts.into_iter().map(|part| div().child(part)))
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

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

    #[test]
    fn test_clock_and_date_vertical_widgets() {
        let clock = ClockWidget::new("clock", "18:34", true);
        assert!(clock.vertical);
        let date = DateWidget::new("date", "Tue, 04 Aug", true);
        assert!(date.vertical);
    }
}
