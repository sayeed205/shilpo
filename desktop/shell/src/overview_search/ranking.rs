use crate::actions::ActionDescriptor;
use shilpo_services::{AppScanner, Application};
use std::path::PathBuf;

pub fn expand_path(query: &str) -> Option<PathBuf> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let expanded = if let Some(stripped) = q.strip_prefix("~/") {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(stripped))
    } else if q.starts_with('/') {
        Some(PathBuf::from(q))
    } else {
        None
    };

    if let Some(path) = expanded
        && path.exists()
    {
        Some(path)
    } else {
        None
    }
}

pub fn is_uri_spec(query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.starts_with("http://")
        || q.starts_with("https://")
        || q.starts_with("file://")
        || q.starts_with("mailto:")
        || q.starts_with("ssh://")
        || q.starts_with("ftp://")
}

#[derive(Debug, Clone)]
pub struct ScoredApp {
    pub app: Application,
    pub score: u32,
}

pub fn rank_applications(
    scanner: &AppScanner,
    query: &str,
    recent_apps: &[String],
) -> Vec<Application> {
    let q = query.trim().to_lowercase();
    let all_apps = scanner.applications();

    let mut seen_execs = std::collections::HashSet::new();
    let mut scored: Vec<ScoredApp> = all_apps
        .into_iter()
        .filter_map(|app| {
            if !seen_execs.insert(app.exec.clone()) {
                return None;
            }
            let name_lower = app.name.to_lowercase();
            let (tier, mut score);

            if q.is_empty() {
                tier = 0;
                score = 10;
            } else if name_lower == q {
                tier = 5;
                score = 100;
            } else if name_lower.starts_with(&q) {
                tier = 4;
                score = 80;
            } else if name_lower.contains(&q) {
                tier = 3;
                score = 50;
            } else if let Some(desc) = &app.description
                && desc.to_lowercase().contains(&q)
            {
                tier = 2;
                score = 30;
            } else if app.categories.iter().any(|c| c.to_lowercase().contains(&q)) {
                tier = 1;
                score = 20;
            } else {
                return None;
            }

            // Recent app boost
            if let Some(pos) = recent_apps.iter().position(|r| r == &app.exec) {
                score += 50u32.saturating_sub((pos * 5) as u32);
            }

            score += tier * 1_000;
            Some(ScoredApp { app, score })
        })
        .collect();

    // Sort descending by score, then ascending by name
    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.app.name.cmp(&b.app.name))
    });

    scored.into_iter().map(|s| s.app).take(8).collect()
}

pub fn rank_actions(descriptors: &[ActionDescriptor], query: &str) -> Vec<ActionDescriptor> {
    let q = query.trim().to_lowercase();
    let mut actions: Vec<_> = descriptors
        .iter()
        .filter(|action| {
            action.input.can_invoke_without_input()
                && (q.is_empty()
                    || action.label.to_lowercase().contains(&q)
                    || action.name.contains(&q))
        })
        .cloned()
        .collect();

    actions.sort_by_key(|action| {
        let label_lower = action.label.to_lowercase();
        if label_lower.starts_with(&q) {
            0
        } else if label_lower.contains(&q) {
            1
        } else {
            2
        }
    });

    actions.into_iter().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_path() {
        assert!(expand_path("/tmp").is_some());
        assert!(expand_path("/nonexistent_path_123456").is_none());
    }

    #[test]
    fn test_is_uri_spec() {
        assert!(is_uri_spec("https://github.com"));
        assert!(is_uri_spec("http://localhost:8080"));
        assert!(!is_uri_spec("github.com"));
    }
}
