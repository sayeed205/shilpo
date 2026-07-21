pub mod icons;

use anyhow::Result;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    thread,
};

/// Represents an installed desktop application parsed from a .desktop file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub icon_path: Option<PathBuf>,
    pub description: Option<String>,
    pub desktop_file: PathBuf,
}

impl Application {
    /// Launches the application in a detached background thread.
    pub fn launch(&self) {
        let exec = self.exec.clone();
        thread::spawn(move || {
            let exec_cleaned = exec
                .replace("%f", "")
                .replace("%F", "")
                .replace("%u", "")
                .replace("%U", "")
                .replace("%d", "")
                .replace("%D", "")
                .replace("%n", "")
                .replace("%N", "")
                .replace("%i", "")
                .replace("%c", "")
                .replace("%k", "");

            let _ = Command::new("sh").args(["-c", exec_cleaned.trim()]).spawn();
        });
    }
}

/// Service for scanning and searching installed desktop applications.
pub struct AppScanner {
    apps: Arc<Mutex<Vec<Application>>>,
}

impl AppScanner {
    /// Creates a new AppScanner and scans system application directories.
    pub fn new() -> Result<Self> {
        let apps = Arc::new(Mutex::new(Vec::new()));
        let scanner = Self { apps };
        scanner.rescan();
        Ok(scanner)
    }

    /// Rescans XDG application directories for .desktop files.
    pub fn rescan(&self) {
        let mut scanned = Vec::new();
        let mut dirs = vec![
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
        ];

        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join(".local/share/applications"));
        }

        for dir in dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.filter_map(Result::ok) {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("desktop")
                        && let Ok(app) = parse_desktop_file(&path)
                    {
                        scanned.push(app);
                    }
                }
            }
        }

        scanned.sort_by_key(|a| a.name.to_lowercase());
        let mut lock = self.apps.lock().unwrap();
        *lock = scanned;
    }

    /// Returns all scanned applications.
    pub fn applications(&self) -> Vec<Application> {
        self.apps.lock().unwrap().clone()
    }

    /// Performs case-insensitive search over application names and descriptions.
    pub fn search(&self, query: &str) -> Vec<Application> {
        let query_lower = query.trim().to_lowercase();
        let lock = self.apps.lock().unwrap();

        if query_lower.is_empty() {
            return lock.clone();
        }

        lock.iter()
            .filter(|app| {
                app.name.to_lowercase().contains(&query_lower)
                    || app
                        .description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .collect()
    }
}

fn parse_desktop_file(path: &PathBuf) -> Result<Application> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut description = None;
    let mut no_display = false;
    let mut in_desktop_entry = false;

    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }

        if !in_desktop_entry || line.starts_with('#') {
            continue;
        }

        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim();

            match key {
                "Name" if name.is_none() => name = Some(val.to_string()),
                "Exec" if exec.is_none() => exec = Some(val.to_string()),
                "Icon" if icon.is_none() => icon = Some(val.to_string()),
                "Comment" if description.is_none() => description = Some(val.to_string()),
                "NoDisplay" if val.eq_ignore_ascii_case("true") => no_display = true,
                _ => {}
            }
        }
    }

    if no_display {
        anyhow::bail!("NoDisplay is true");
    }

    let name = name.ok_or_else(|| anyhow::anyhow!("Missing Name"))?;
    let exec = exec.ok_or_else(|| anyhow::anyhow!("Missing Exec"))?;
    let icon_path = icon.as_ref().and_then(|i| icons::lookup_icon(i));

    Ok(Application {
        name,
        exec,
        icon,
        icon_path,
        description,
        desktop_file: path.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_scanner_creation() {
        let scanner = AppScanner::new().unwrap();
        let apps = scanner.applications();
        assert!(
            !apps.is_empty(),
            "Expected to find system desktop applications"
        );
    }

    #[test]
    fn test_app_search() {
        let scanner = AppScanner::new().unwrap();
        let results = scanner.search("a");
        assert!(!results.is_empty());
    }
}
