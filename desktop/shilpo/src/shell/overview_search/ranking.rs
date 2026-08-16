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
