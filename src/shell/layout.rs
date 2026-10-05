use amane::Service;

use crate::config::Settings;

#[derive(Debug, PartialEq)]
struct BarSpace {
    reserved: f32,
    reserved_top: f32,
}

pub(crate) fn height() -> f32 {
    Settings::read().number("bar_height")
}

pub(crate) fn reserved() -> f32 {
    let auto_hide = Settings::read().flag("bar_auto_hide");

    if auto_hide {
        return calculate(0.0, "top", true).reserved;
    }

    calculate(height(), "top", false).reserved
}

pub(crate) fn reserved_top() -> f32 {
    let bottom = Settings::read().text("bar_position") == "bottom";

    if bottom {
        return calculate(0.0, "bottom", false).reserved_top;
    }

    let reserved = reserved();

    calculate(reserved, "top", false).reserved_top
}

fn calculate(height: f32, position: &str, auto_hide: bool) -> BarSpace {
    let reserved = if auto_hide { 0.0 } else { height };
    let reserved_top = if position == "bottom" { 0.0 } else { reserved };

    BarSpace {
        reserved,
        reserved_top,
    }
}

#[cfg(test)]
mod tests {
    use super::{BarSpace, calculate};

    #[test]
    fn top_bar_reserves_a_nondefault_height_unless_hidden() {
        assert_eq!(
            calculate(53.5, "top", false),
            BarSpace {
                reserved: 53.5,
                reserved_top: 53.5,
            },
        );
        assert_eq!(
            calculate(53.5, "top", true),
            BarSpace {
                reserved: 0.0,
                reserved_top: 0.0,
            },
        );
    }

    #[test]
    fn bottom_bar_reserves_space_below_but_not_above() {
        assert_eq!(
            calculate(53.5, "bottom", false),
            BarSpace {
                reserved: 53.5,
                reserved_top: 0.0,
            },
        );
        assert_eq!(
            calculate(53.5, "bottom", true),
            BarSpace {
                reserved: 0.0,
                reserved_top: 0.0,
            },
        );
    }

    #[test]
    fn unknown_position_keeps_the_existing_top_bar_interpretation() {
        assert_eq!(
            calculate(53.5, "unknown", false),
            BarSpace {
                reserved: 53.5,
                reserved_top: 53.5,
            },
        );
        assert_eq!(
            calculate(53.5, "unknown", true),
            BarSpace {
                reserved: 0.0,
                reserved_top: 0.0,
            },
        );
    }
}
