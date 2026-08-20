use shilpo_m3e::IconName;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchMode {
    Default,
    Apps,
    Actions,
    Clipboard,
    Calculator,
    Command,
    WebSearch,
    Keybindings,
}

impl SearchMode {
    /// Returns the fallback default icon for this search mode.
    pub fn default_icon(&self) -> IconName {
        match self {
            Self::Apps | Self::Command => IconName::Terminal,
            Self::Actions => IconName::Settings,
            Self::Clipboard | Self::Calculator | Self::Keybindings => IconName::Star,
            Self::WebSearch | Self::Default => IconName::Search,
        }
    }
}

pub fn parse_query(raw: &str) -> (SearchMode, &str) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (SearchMode::Default, "");
    }

    if let Some(rest) = trimmed.strip_prefix('>') {
        (SearchMode::Apps, rest.trim_start())
    } else if let Some(rest) = trimmed.strip_prefix('/') {
        (SearchMode::Actions, rest.trim_start())
    } else if let Some(rest) = trimmed.strip_prefix(';') {
        (SearchMode::Clipboard, rest.trim_start())
    } else if let Some(rest) = trimmed.strip_prefix('=') {
        (SearchMode::Calculator, rest.trim_start())
    } else if let Some(rest) = trimmed.strip_prefix('$') {
        (SearchMode::Command, rest.trim_start())
    } else if let Some(rest) = trimmed.strip_prefix('?') {
        (SearchMode::WebSearch, rest.trim_start())
    } else if let Some(rest) = trimmed.strip_prefix('<') {
        (SearchMode::Keybindings, rest.trim_start())
    } else if is_implicit_calculator_expression(trimmed) {
        (SearchMode::Calculator, trimmed)
    } else {
        (SearchMode::Default, trimmed)
    }
}

fn is_implicit_calculator_expression(expr: &str) -> bool {
    let first = match expr.chars().next() {
        Some(c) => c,
        None => return false,
    };
    if !(first.is_ascii_digit() || first == '(' || first == '-' || first == '+') {
        return false;
    }
    // Must contain at least one math operator or digit expression
    expr.chars().all(|c| {
        c.is_ascii_digit()
            || matches!(
                c,
                '+' | '-' | '*' | '/' | '%' | '^' | '(' | ')' | '.' | ' ' | ','
            )
    }) && expr
        .chars()
        .any(|c| matches!(c, '+' | '-' | '*' | '/' | '%' | '^'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query_prefixes() {
        assert_eq!(parse_query(">firefox"), (SearchMode::Apps, "firefox"));
        assert_eq!(parse_query("/toggle"), (SearchMode::Actions, "toggle"));
        assert_eq!(parse_query(";hello"), (SearchMode::Clipboard, "hello"));
        assert_eq!(parse_query("=2+2"), (SearchMode::Calculator, "2+2"));
        assert_eq!(parse_query("$ls -la"), (SearchMode::Command, "ls -la"));
        assert_eq!(
            parse_query("?rust lang"),
            (SearchMode::WebSearch, "rust lang")
        );
        assert_eq!(parse_query("<super+"), (SearchMode::Keybindings, "super+"));
    }

    #[test]
    fn test_implicit_calculator() {
        assert_eq!(parse_query("2 + 2"), (SearchMode::Calculator, "2 + 2"));
        assert_eq!(
            parse_query("10 * (5 - 3)"),
            (SearchMode::Calculator, "10 * (5 - 3)")
        );
        assert_eq!(parse_query("hello 2"), (SearchMode::Default, "hello 2"));
    }
}
