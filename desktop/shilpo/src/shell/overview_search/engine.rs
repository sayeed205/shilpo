use std::path::PathBuf;

use shilpo_services::{AppScanner, Application, ClipboardItem};
use shilpo_ui::IconName;

use super::{calculator, parser, ranking};
use crate::actions::ActionDescriptor;

#[derive(Debug, Clone)]
pub enum SearchResultIcon {
    AppIcon(Option<PathBuf>),
    Named(IconName),
    Initial(char),
}

#[derive(Debug, Clone)]
pub enum SearchIntent {
    LaunchApp(Application),
    InvokeAction(ActionDescriptor),
    CopyClipboard(ClipboardItem),
    CopyCalculation(String),
    ExecuteCommand(String),
    OpenWeb(String),
    OpenPath(PathBuf),
    OpenUri(String),
    CopyKeybinding(String),
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub icon: SearchResultIcon,
    pub title: String,
    pub description: String,
    pub result_type: String,
    pub activation_verb: String,
    pub intent: SearchIntent,
}

#[derive(Clone)]
pub struct OverviewSearch {
    scanner: AppScanner,
    recent_apps: Vec<String>,
    actions: Vec<ActionDescriptor>,
    clipboard_history: Vec<ClipboardItem>,
    keybindings: Vec<(String, String)>,
}

impl OverviewSearch {
    pub fn new(
        scanner: AppScanner,
        recent_apps: Vec<String>,
        actions: Vec<ActionDescriptor>,
        clipboard_history: Vec<ClipboardItem>,
        keybindings: Vec<(String, String)>,
    ) -> Self {
        Self {
            scanner,
            recent_apps,
            actions,
            clipboard_history,
            keybindings,
        }
    }

    pub fn search(&self, raw_query: &str) -> Vec<SearchResult> {
        let (mode, query) = parser::parse_query(raw_query);
        let mut results = Vec::new();

        match mode {
            parser::SearchMode::Default => {
                if let Some(path) = ranking::expand_path(raw_query) {
                    results.push(SearchResult {
                        icon: SearchResultIcon::Named(IconName::Folder),
                        title: path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("File")
                            .to_string(),
                        description: path.display().to_string(),
                        result_type: "Path".to_string(),
                        activation_verb: "Open path".to_string(),
                        intent: SearchIntent::OpenPath(path),
                    });
                } else if ranking::is_uri_spec(raw_query) {
                    let uri = raw_query.trim().to_string();
                    results.push(SearchResult {
                        icon: SearchResultIcon::Named(IconName::Star),
                        title: uri.clone(),
                        description: "Web or protocol URI".to_string(),
                        result_type: "URI".to_string(),
                        activation_verb: "Open link".to_string(),
                        intent: SearchIntent::OpenUri(uri),
                    });
                }

                let apps = ranking::rank_applications(&self.scanner, query, &self.recent_apps);
                for app in apps {
                    results.push(SearchResult {
                        icon: SearchResultIcon::AppIcon(app.icon_path.clone()),
                        title: app.name.clone(),
                        description: app.description.clone().unwrap_or_else(|| app.exec.clone()),
                        result_type: "Application".to_string(),
                        activation_verb: "Launch".to_string(),
                        intent: SearchIntent::LaunchApp(app),
                    });
                }

                let actions = ranking::rank_actions(&self.actions, query);
                for action in actions {
                    results.push(SearchResult {
                        icon: SearchResultIcon::Named(IconName::Settings),
                        title: action.label.clone(),
                        description: format!("System Action ({})", action.name),
                        result_type: "Action".to_string(),
                        activation_verb: "Run".to_string(),
                        intent: SearchIntent::InvokeAction(action),
                    });
                }

                // Keep the two universal fallbacks visible at the end of the
                // default result list instead of letting app matches consume
                // every slot.
                if !query.trim().is_empty() {
                    results.truncate(6);
                }

                // The default launcher mode also exposes useful fallbacks so
                // arbitrary text can become an immediate command or web query.
                if !query.trim().is_empty() {
                    let command = query.trim().to_string();
                    results.push(SearchResult {
                        icon: SearchResultIcon::Named(IconName::Terminal),
                        title: command.clone(),
                        description: "Run command".to_string(),
                        result_type: "Command".to_string(),
                        activation_verb: "Run".to_string(),
                        intent: SearchIntent::ExecuteCommand(command),
                    });
                }
                if !query.trim().is_empty() {
                    let query = query.trim();
                    let url = format!(
                        "https://www.google.com/search?q={}",
                        percent_encode_query(query)
                    );
                    results.push(SearchResult {
                        icon: SearchResultIcon::Named(IconName::Search),
                        title: query.to_string(),
                        description: "Search the web".to_string(),
                        result_type: "Web Search".to_string(),
                        activation_verb: "Search".to_string(),
                        intent: SearchIntent::OpenWeb(url),
                    });
                }

                results.truncate(8);
            }
            parser::SearchMode::Apps => {
                let apps = ranking::rank_applications(&self.scanner, query, &self.recent_apps);
                for app in apps {
                    results.push(SearchResult {
                        icon: SearchResultIcon::AppIcon(app.icon_path.clone()),
                        title: app.name.clone(),
                        description: app.description.clone().unwrap_or_else(|| app.exec.clone()),
                        result_type: "Application".to_string(),
                        activation_verb: "Launch".to_string(),
                        intent: SearchIntent::LaunchApp(app),
                    });
                }
                results.truncate(8);
            }
            parser::SearchMode::Actions => {
                let actions = ranking::rank_actions(&self.actions, query);
                for action in actions {
                    results.push(SearchResult {
                        icon: SearchResultIcon::Named(IconName::Settings),
                        title: action.label.clone(),
                        description: format!("System Action ({})", action.name),
                        result_type: "Action".to_string(),
                        activation_verb: "Run".to_string(),
                        intent: SearchIntent::InvokeAction(action),
                    });
                }
                results.truncate(8);
            }
            parser::SearchMode::Clipboard => {
                let q = query.to_lowercase();
                for item in &self.clipboard_history {
                    if q.is_empty() || item.text.to_lowercase().contains(&q) {
                        results.push(SearchResult {
                            icon: SearchResultIcon::Named(IconName::Star),
                            title: item.text.clone(),
                            description: format!("Copied at {}", item.timestamp),
                            result_type: "Clipboard".to_string(),
                            activation_verb: "Copy".to_string(),
                            intent: SearchIntent::CopyClipboard(item.clone()),
                        });
                        if results.len() >= 8 {
                            break;
                        }
                    }
                }
            }
            parser::SearchMode::Calculator => {
                if let Some(val) = calculator::evaluate_expression(query) {
                    results.push(SearchResult {
                        icon: SearchResultIcon::Named(IconName::Star),
                        title: val.clone(),
                        description: format!("= {}", query),
                        result_type: "Calculator".to_string(),
                        activation_verb: "Copy result".to_string(),
                        intent: SearchIntent::CopyCalculation(val),
                    });
                }
            }
            parser::SearchMode::Command => {
                if !query.trim().is_empty() && shilpo_services::find_terminal_emulator().is_some() {
                    let cmd = query.trim().to_string();
                    results.push(SearchResult {
                        icon: SearchResultIcon::Named(IconName::Terminal),
                        title: format!("$ {}", cmd),
                        description: "Execute shell command in terminal".to_string(),
                        result_type: "Command".to_string(),
                        activation_verb: "Run command".to_string(),
                        intent: SearchIntent::ExecuteCommand(cmd),
                    });
                }
            }
            parser::SearchMode::WebSearch => {
                if !query.trim().is_empty() {
                    let encoded = percent_encode_query(query.trim());
                    let url = format!("https://www.google.com/search?q={}", encoded);
                    results.push(SearchResult {
                        icon: SearchResultIcon::Named(IconName::Search),
                        title: format!("Search Google for \"{}\"", query.trim()),
                        description: url.clone(),
                        result_type: "Web Search".to_string(),
                        activation_verb: "Search".to_string(),
                        intent: SearchIntent::OpenWeb(url),
                    });
                }
            }
            parser::SearchMode::Keybindings => {
                let q = query.to_lowercase();
                for (shortcut, label) in &self.keybindings {
                    if q.is_empty()
                        || shortcut.to_lowercase().contains(&q)
                        || label.to_lowercase().contains(&q)
                    {
                        results.push(SearchResult {
                            icon: SearchResultIcon::Named(IconName::Star),
                            title: shortcut.clone(),
                            description: label.clone(),
                            result_type: "Keybinding".to_string(),
                            activation_verb: "Copy shortcut".to_string(),
                            intent: SearchIntent::CopyKeybinding(shortcut.clone()),
                        });
                        if results.len() >= 8 {
                            break;
                        }
                    }
                }
            }
        }

        results
    }
}

fn percent_encode_query(query: &str) -> String {
    let mut encoded = String::with_capacity(query.len());
    for byte in query.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else if byte == b' ' {
            encoded.push('+');
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::percent_encode_query;

    #[test]
    fn encodes_reserved_and_unicode_query_bytes() {
        assert_eq!(
            percent_encode_query("rust & gpui/日本語"),
            "rust+%26+gpui%2F%E6%97%A5%E6%9C%AC%E8%AA%9E"
        );
    }
}
