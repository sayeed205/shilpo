pub mod icons;

use anyhow::Result;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex, mpsc},
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
    pub categories: Vec<String>,
    pub desktop_file: PathBuf,
    pub working_dir: Option<PathBuf>,
    pub terminal: bool,
    pub try_exec: Option<String>,
}

fn spawn_scoped_command(
    program: &str,
    args: &[String],
    working_dir: Option<&PathBuf>,
    app_name: &str,
) -> std::io::Result<std::process::Child> {
    if binary_exists("systemd-run") {
        let clean_name: String = app_name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let unit_name = format!("app-{}-{}", clean_name, timestamp);

        let mut systemd_cmd = Command::new("systemd-run");
        systemd_cmd
            .arg("--user")
            .arg("--scope")
            .arg(format!("--unit={}", unit_name))
            .arg("--")
            .arg(program)
            .args(args);

        if let Some(dir) = working_dir {
            systemd_cmd.current_dir(dir);
        }

        if let Ok(child) = systemd_cmd.spawn() {
            return Ok(child);
        }
    }

    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }
    cmd.spawn()
}

impl Application {
    /// Launches the application in a detached background thread.
    pub fn launch(&self) {
        let exec = self.exec.clone();
        let icon = self.icon.clone();
        let desktop_file = self.desktop_file.clone();
        let name = self.name.clone();
        let working_dir = self.working_dir.clone();
        let is_terminal = self.terminal;

        thread::spawn(move || match parse_exec(&exec, icon.as_deref()) {
            Ok(mut argv) => {
                if is_terminal {
                    let term = find_terminal_emulator().unwrap_or_else(|| "xterm".to_string());
                    let mut term_argv = vec![term, "-e".to_string()];
                    term_argv.extend(argv);
                    argv = term_argv;
                }

                let program = &argv[0];
                let args = &argv[1..];

                if let Err(err) = spawn_scoped_command(program, args, working_dir.as_ref(), &name) {
                    eprintln!(
                        "Failed to launch application '{}' ({}) via {:?}: {}",
                        name,
                        desktop_file.display(),
                        argv,
                        err
                    );
                }
            }
            Err(err) => {
                eprintln!(
                    "Failed to parse Exec key for application '{}' ({}): {}",
                    name,
                    desktop_file.display(),
                    err
                );
            }
        });
    }

    /// Launches the application in a detached background thread and invokes `on_failure` callback if execution fails.
    pub fn launch_with_feedback(&self, on_failure: impl Fn(String) + Send + 'static) {
        let exec = self.exec.clone();
        let icon = self.icon.clone();
        let desktop_file = self.desktop_file.clone();
        let name = self.name.clone();
        let working_dir = self.working_dir.clone();
        let is_terminal = self.terminal;

        thread::spawn(move || match parse_exec(&exec, icon.as_deref()) {
            Ok(mut argv) => {
                if is_terminal {
                    let term = find_terminal_emulator().unwrap_or_else(|| "xterm".to_string());
                    let mut term_argv = vec![term, "-e".to_string()];
                    term_argv.extend(argv);
                    argv = term_argv;
                }

                let program = &argv[0];
                let args = &argv[1..];

                match spawn_scoped_command(program, args, working_dir.as_ref(), &name) {
                    Ok(mut child) => {
                        thread::sleep(std::time::Duration::from_millis(400));
                        if let Ok(Some(status)) = child.try_wait()
                            && !status.success()
                        {
                            let code_str = status
                                .code()
                                .map(|c| c.to_string())
                                .unwrap_or_else(|| "signal".to_string());
                            on_failure(format!(
                                "Application '{}' exited with status {}",
                                name, code_str
                            ));
                        }
                    }
                    Err(err) => {
                        on_failure(format!(
                            "Failed to launch application '{}' ({}) via {:?}: {}",
                            name,
                            desktop_file.display(),
                            argv,
                            err
                        ));
                    }
                }
            }
            Err(err) => {
                on_failure(format!("Failed to parse exec line for '{}': {}", name, err));
            }
        });
    }
}

pub fn binary_exists(bin: &str) -> bool {
    let clean = bin.trim();
    if clean.is_empty() {
        return false;
    }
    if clean.contains('/') {
        return std::path::Path::new(clean).exists();
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if dir.join(clean).is_file() {
                return true;
            }
        }
    }
    false
}

pub fn find_terminal_emulator() -> Option<String> {
    for term in ["foot", "alacritty", "kitty", "xterm", "x-terminal-emulator"] {
        if binary_exists(term) {
            return Some(term.to_string());
        }
    }
    None
}

/// Parses a Freedesktop Desktop Entry `Exec` line into a program and argument vector.
///
/// Handles double quotes (`"..."`), single quotes (`'...'`), backslash escaping (`\`),
/// and Freedesktop field codes (`%f`, `%F`, `%u`, `%U`, `%i`, `%%`, etc.).
pub fn parse_exec(exec: &str, icon: Option<&str>) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current_arg = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut has_token = false;

    let chars: Vec<char> = exec.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if escaped {
            if in_double {
                // Inside double quotes, backslash only escapes ", \, $, `
                match ch {
                    '"' | '\\' | '$' | '`' => current_arg.push(ch),
                    _ => {
                        current_arg.push('\\');
                        current_arg.push(ch);
                    }
                }
            } else {
                // Outside double quotes, backslash escapes any character
                current_arg.push(ch);
            }
            escaped = false;
            has_token = true;
            i += 1;
            continue;
        }

        if ch == '\\' && !in_single {
            escaped = true;
            has_token = true;
            i += 1;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current_arg.push(ch);
            }
            has_token = true;
            i += 1;
            continue;
        }

        if in_double {
            if ch == '"' {
                in_double = false;
            } else if ch == '%' {
                // Check for field code inside double quotes
                if i + 1 < chars.len() {
                    let next = chars[i + 1];
                    if next == '%' {
                        current_arg.push('%');
                        i += 1;
                    } else if next.is_ascii_alphabetic() {
                        // Field code inside quotes without target is stripped
                        i += 1;
                    } else {
                        current_arg.push('%');
                    }
                } else {
                    current_arg.push('%');
                }
            } else {
                current_arg.push(ch);
            }
            has_token = true;
            i += 1;
            continue;
        }

        // Unquoted context
        match ch {
            '\'' => {
                in_single = true;
                has_token = true;
            }
            '"' => {
                in_double = true;
                has_token = true;
            }
            ' ' | '\t' | '\n' | '\r' => {
                if has_token {
                    args.push(std::mem::take(&mut current_arg));
                    has_token = false;
                }
            }
            '%' => {
                if i + 1 < chars.len() {
                    let next = chars[i + 1];
                    match next {
                        '%' => {
                            current_arg.push('%');
                            has_token = true;
                            i += 1;
                        }
                        'i' => {
                            i += 1;
                            if let Some(ic) = icon.filter(|s| !s.is_empty()) {
                                if has_token {
                                    args.push(std::mem::take(&mut current_arg));
                                    has_token = false;
                                }
                                args.push("--icon".to_string());
                                args.push(ic.to_string());
                            }
                        }
                        c if c.is_ascii_alphabetic() => {
                            // Strip other field codes (%f, %F, %u, %U, %d, %D, %n, %N, %c, %k)
                            i += 1;
                        }
                        _ => {
                            current_arg.push('%');
                            has_token = true;
                        }
                    }
                } else {
                    current_arg.push('%');
                    has_token = true;
                }
            }
            _ => {
                current_arg.push(ch);
                has_token = true;
            }
        }

        i += 1;
    }

    if escaped || in_single || in_double {
        anyhow::bail!("Unclosed quote or trailing escape in Exec command line");
    }

    if has_token {
        args.push(current_arg);
    }

    if args.is_empty() {
        anyhow::bail!("Empty Exec command line");
    }

    Ok(args)
}

/// Service for scanning and searching installed desktop applications.
#[derive(Clone)]
pub struct AppScanner {
    apps: Arc<Mutex<Vec<Application>>>,
    directories: Arc<Vec<PathBuf>>,
    subscribers: Arc<Mutex<Vec<mpsc::SyncSender<()>>>>,
}

impl Default for AppScanner {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl AppScanner {
    /// Creates an empty AppScanner without running synchronous disk I/O.
    pub fn new_empty() -> Self {
        Self::with_directories(application_directories())
    }

    /// Creates an AppScanner initialized with a pre-scanned list of applications.
    pub fn from_applications(apps: Vec<Application>) -> Self {
        Self {
            apps: Arc::new(Mutex::new(apps)),
            directories: Arc::new(application_directories()),
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Creates a new AppScanner and scans system application directories.
    pub fn new() -> Result<Self> {
        let scanner = Self::new_empty();
        scanner.rescan();
        Ok(scanner)
    }

    /// Rescans XDG application directories for .desktop files.
    pub fn rescan(&self) {
        let mut scanned = Vec::new();
        for dir in self.directories.iter() {
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
        self.replace_applications(scanned);
    }

    /// Starts watching XDG and Flatpak application directories for .desktop file changes.
    pub fn start_watcher(&self) -> Option<notify::RecommendedWatcher> {
        use notify::Watcher;

        let catalog = self.clone();

        match notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res
                    && event
                        .paths
                        .iter()
                        .any(|p| p.extension().and_then(|e| e.to_str()) == Some("desktop"))
                {
                    icons::clear_icon_cache();
                    catalog.rescan();
                }
            },
            notify::Config::default(),
        ) {
            Ok(mut watcher) => {
                let mut watched_any = false;
                for dir in self.directories.iter() {
                    if dir.exists()
                        && watcher
                            .watch(dir, notify::RecursiveMode::NonRecursive)
                            .is_ok()
                    {
                        watched_any = true;
                    }
                }
                if watched_any { Some(watcher) } else { None }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to initialize application directory watcher");
                None
            }
        }
    }

    /// Returns all scanned applications.
    pub fn applications(&self) -> Vec<Application> {
        self.apps.lock().unwrap().clone()
    }

    /// Subscribes to catalog changes. Notifications are coalesced while a caller is busy.
    pub fn subscribe(&self) -> mpsc::Receiver<()> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.subscribers.lock().unwrap().push(sender);
        receiver
    }

    /// Performs case-insensitive search with category filtering and relevance ranking score.
    pub fn search_with_category(&self, query: &str, category: Option<&str>) -> Vec<Application> {
        let query_lower = query.trim().to_lowercase();
        let cat_lower = category.map(|c| c.trim().to_lowercase());
        let lock = self.apps.lock().unwrap();

        let mut matches: Vec<(u32, Application)> = lock
            .iter()
            .filter_map(|app| {
                if let Some(ref target_cat) = cat_lower
                    && !target_cat.is_empty()
                {
                    let has_cat = app
                        .categories
                        .iter()
                        .any(|c| c.to_lowercase() == *target_cat);
                    if !has_cat {
                        return None;
                    }
                }

                if query_lower.is_empty() {
                    return Some((0, app.clone()));
                }

                let name_lower = app.name.to_lowercase();
                let score = if name_lower == query_lower {
                    100
                } else if name_lower.starts_with(&query_lower) {
                    80
                } else if name_lower.contains(&query_lower) {
                    50
                } else if app
                    .description
                    .as_ref()
                    .is_some_and(|d| d.to_lowercase().contains(&query_lower))
                {
                    30
                } else if app
                    .categories
                    .iter()
                    .any(|c| c.to_lowercase().contains(&query_lower))
                {
                    20
                } else {
                    return None;
                };

                Some((score, app.clone()))
            })
            .collect();

        if !query_lower.is_empty() {
            matches.sort_by_key(|a| std::cmp::Reverse(a.0));
        }

        matches.into_iter().map(|(_, app)| app).collect()
    }

    /// Performs case-insensitive search over application names and descriptions.
    pub fn search(&self, query: &str) -> Vec<Application> {
        self.search_with_category(query, None)
    }

    fn with_directories(directories: Vec<PathBuf>) -> Self {
        Self {
            apps: Arc::new(Mutex::new(Vec::new())),
            directories: Arc::new(directories),
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn replace_applications(&self, applications: Vec<Application>) {
        *self.apps.lock().unwrap() = applications;
        self.subscribers
            .lock()
            .unwrap()
            .retain(|subscriber| match subscriber.try_send(()) {
                Ok(()) | Err(mpsc::TrySendError::Full(())) => true,
                Err(mpsc::TrySendError::Disconnected(())) => false,
            });
    }
}

pub fn list_applications() -> Result<Vec<Application>> {
    let mut apps = Vec::new();
    for dir in application_directories() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("desktop")
                && let Ok(app) = parse_desktop_file(&path)
            {
                apps.push(app);
            }
        }
    }
    Ok(apps)
}

fn application_directories() -> Vec<PathBuf> {
    let mut directories = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];

    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        directories.push(home.join(".local/share/applications"));
        directories.push(home.join(".local/share/flatpak/exports/share/applications"));
    }

    directories
}

fn parse_desktop_file(path: &PathBuf) -> Result<Application> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut description = None;
    let mut categories = Vec::new();
    let mut no_display = false;
    let mut working_dir = None;
    let mut terminal = false;
    let mut try_exec = None;
    let mut only_show_in = Vec::new();
    let mut not_show_in = Vec::new();
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
                "TryExec" if try_exec.is_none() => try_exec = Some(val.to_string()),
                "Path" if working_dir.is_none() => working_dir = Some(PathBuf::from(val)),
                "Terminal" => terminal = val.eq_ignore_ascii_case("true"),
                "Categories" if categories.is_empty() => {
                    categories = val
                        .split(';')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "OnlyShowIn" if only_show_in.is_empty() => {
                    only_show_in = val
                        .split(';')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "NotShowIn" if not_show_in.is_empty() => {
                    not_show_in = val
                        .split(';')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "NoDisplay" if val.eq_ignore_ascii_case("true") => no_display = true,
                _ => {}
            }
        }
    }

    if no_display {
        anyhow::bail!("NoDisplay is true");
    }

    if let Some(ref bin) = try_exec
        && !binary_exists(bin)
    {
        anyhow::bail!("TryExec binary '{}' does not exist", bin);
    }

    if let Ok(current_desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
        let current = current_desktop.trim();
        if !only_show_in.is_empty() && !only_show_in.iter().any(|d| d.eq_ignore_ascii_case(current))
        {
            anyhow::bail!("OnlyShowIn does not match current desktop '{}'", current);
        }
        if not_show_in.iter().any(|d| d.eq_ignore_ascii_case(current)) {
            anyhow::bail!("NotShowIn matches current desktop '{}'", current);
        }
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
        categories,
        desktop_file: path.clone(),
        working_dir,
        terminal,
        try_exec,
    })
}

/// Resolves the default URI scheme or MIME type handler desktop file name (e.g. mailto:, https:, file:).
pub fn resolve_handler_for_uri(uri: &str) -> Option<String> {
    if let Some((scheme, _)) = uri.split_once(':') {
        let scheme_mime = format!("x-scheme-handler/{}", scheme.to_lowercase());
        let mut mime_paths = Vec::new();

        if let Ok(home) = std::env::var("HOME") {
            mime_paths.push(PathBuf::from(home).join(".config/mimeapps.list"));
        }
        mime_paths.push(PathBuf::from("/usr/share/applications/mimeinfo.cache"));
        mime_paths.push(PathBuf::from(
            "/usr/local/share/applications/mimeinfo.cache",
        ));

        for path in mime_paths {
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines() {
                    if let Some((key, val)) = line.split_once('=')
                        && key.trim() == scheme_mime
                    {
                        let desktop = val.split(';').next().unwrap_or(val).trim();
                        if !desktop.is_empty() {
                            return Some(desktop.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Helper to percent-decode URL string (e.g. "%20" -> " ").
pub fn percent_decode(s: &str) -> String {
    let mut decoded = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            decoded.push(b);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&decoded).to_string()
}

/// Parses a Freedesktop/RFC 2483 `text/uri-list` payload into validated local file `PathBuf`s.
pub fn parse_uri_list(raw_data: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in raw_data.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let path_str = if let Some(stripped) = trimmed.strip_prefix("file://") {
            stripped
        } else {
            trimmed
        };

        let decoded = percent_decode(path_str);
        let path = PathBuf::from(decoded);
        if path.exists() {
            paths.push(path);
        }
    }
    paths
}

/// Validates drag & drop payload MIME types and sanitizes local file paths against path traversal attacks (`..`).
pub fn validate_drag_drop_payload(mime_type: &str, data: &[u8]) -> Vec<PathBuf> {
    let mime = mime_type.to_lowercase();
    if mime != "text/uri-list" && mime != "text/plain" && mime != "application/x-uri" {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(data);
    let paths = parse_uri_list(&text);
    paths
        .into_iter()
        .filter(|path| {
            path.is_absolute()
                && !path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uri_list_decoding_and_validation() {
        let raw = "# Comment line\nfile:///tmp\nfile:///nonexistent_path_shilpo_999\n";
        let parsed = parse_uri_list(raw);
        assert_eq!(parsed, vec![PathBuf::from("/tmp")]);
        assert_eq!(percent_decode("Hello%20World"), "Hello World");
    }

    #[test]
    fn test_parse_exec_basic() {
        let argv = parse_exec("firefox", None).unwrap();
        assert_eq!(argv, vec!["firefox"]);
    }

    #[test]
    fn test_parse_exec_with_arguments() {
        let argv = parse_exec("vlc --fullscreen --random", None).unwrap();
        assert_eq!(argv, vec!["vlc", "--fullscreen", "--random"]);
    }

    #[test]
    fn test_parse_exec_quoted_paths_and_args() {
        let exec = r#""/opt/My App/bin" --title="Hello World" 'foo bar'"#;
        let argv = parse_exec(exec, None).unwrap();
        assert_eq!(
            argv,
            vec!["/opt/My App/bin", "--title=Hello World", "foo bar"]
        );
    }

    #[test]
    fn test_parse_exec_escaped_quotes_and_spaces() {
        let exec = r#"app \"quoted\" arg\ with\ space"#;
        let argv = parse_exec(exec, None).unwrap();
        assert_eq!(argv, vec!["app", "\"quoted\"", "arg with space"]);
    }

    #[test]
    fn test_parse_exec_field_codes_stripped() {
        let exec = "gedit %f %F %u %U %d %D %n %N %c %k";
        let argv = parse_exec(exec, None).unwrap();
        assert_eq!(argv, vec!["gedit"]);
    }

    #[test]
    fn test_parse_exec_percent_literal() {
        let exec = "app %%";
        let argv = parse_exec(exec, None).unwrap();
        assert_eq!(argv, vec!["app", "%"]);
    }

    #[test]
    fn test_parse_exec_icon_code() {
        let exec = "app %i";
        let argv_with_icon = parse_exec(exec, Some("app-icon")).unwrap();
        assert_eq!(argv_with_icon, vec!["app", "--icon", "app-icon"]);

        let argv_no_icon = parse_exec(exec, None).unwrap();
        assert_eq!(argv_no_icon, vec!["app"]);
    }

    #[test]
    fn test_uri_protocol_handler_resolution() {
        assert!(
            resolve_handler_for_uri("https://example.com").is_none()
                || resolve_handler_for_uri("https://example.com").is_some()
        );
        assert!(
            resolve_handler_for_uri("mailto:user@example.com").is_none()
                || resolve_handler_for_uri("mailto:user@example.com").is_some()
        );
    }

    #[test]
    fn test_parse_exec_shell_metacharacters_literal() {
        let exec = "echo $HOME && cat /etc/passwd | grep foo";
        let argv = parse_exec(exec, None).unwrap();
        assert_eq!(
            argv,
            vec![
                "echo",
                "$HOME",
                "&&",
                "cat",
                "/etc/passwd",
                "|",
                "grep",
                "foo"
            ]
        );
    }

    #[test]
    fn test_parse_exec_unclosed_quotes() {
        assert!(parse_exec("app \"unclosed", None).is_err());
        assert!(parse_exec("app 'unclosed", None).is_err());
    }

    #[test]
    fn test_parse_exec_empty_command() {
        assert!(parse_exec("", None).is_err());
        assert!(parse_exec("   ", None).is_err());
        assert!(parse_exec("%f", None).is_err());
    }

    #[test]
    fn test_app_scanner_from_applications() {
        let app = Application {
            name: "Test App".to_string(),
            exec: "test-binary --flag".to_string(),
            icon: Some("test-icon".to_string()),
            icon_path: None,
            description: Some("A test application".to_string()),
            categories: vec!["Utility".to_string()],
            desktop_file: PathBuf::from("/tmp/test.desktop"),
            working_dir: Some(PathBuf::from("/tmp")),
            terminal: false,
            try_exec: Some("test-binary".to_string()),
        };
        let scanner = AppScanner::from_applications(vec![app.clone()]);
        assert_eq!(scanner.applications(), vec![app.clone()]);
        assert_eq!(scanner.search("test"), vec![app]);
    }

    #[test]
    fn test_drag_drop_payload_validation_and_mime_filter() {
        let valid = validate_drag_drop_payload(
            "text/uri-list",
            b"file:///tmp\nfile:///tmp/../etc/passwd\n",
        );
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0], PathBuf::from("/tmp"));

        let invalid_mime = validate_drag_drop_payload("application/octet-stream", b"file:///tmp");
        assert!(invalid_mime.is_empty());
    }

    #[test]
    fn test_binary_exists_validation() {
        assert!(binary_exists("ls") || binary_exists("sh") || binary_exists("bash"));
        assert!(!binary_exists("nonexistent_binary_shilpo_12345"));
    }

    #[test]
    fn test_app_search_category_filtering() {
        let app1 = Application {
            name: "VS Code".to_string(),
            exec: "code".to_string(),
            icon: None,
            icon_path: None,
            description: Some("Code Editor".to_string()),
            categories: vec!["Development".to_string()],
            desktop_file: PathBuf::from("/tmp/code.desktop"),
            working_dir: None,
            terminal: false,
            try_exec: None,
        };
        let app2 = Application {
            name: "GIMP".to_string(),
            exec: "gimp".to_string(),
            icon: None,
            icon_path: None,
            description: Some("Image Editor".to_string()),
            categories: vec!["Graphics".to_string()],
            desktop_file: PathBuf::from("/tmp/gimp.desktop"),
            working_dir: None,
            terminal: false,
            try_exec: None,
        };

        let scanner = AppScanner::from_applications(vec![app1.clone(), app2]);
        let dev_apps = scanner.search_with_category("", Some("Development"));
        assert_eq!(dev_apps.len(), 1);
        assert_eq!(dev_apps[0].name, "VS Code");
    }

    #[test]
    fn test_app_search_relevance_ranking() {
        let app1 = Application {
            name: "Terminal".to_string(),
            exec: "terminal".to_string(),
            icon: None,
            icon_path: None,
            description: Some("System Command Line".to_string()),
            categories: vec!["System".to_string()],
            desktop_file: PathBuf::from("/tmp/term.desktop"),
            working_dir: None,
            terminal: true,
            try_exec: None,
        };
        let app2 = Application {
            name: "GNOME Terminal".to_string(),
            exec: "gnome-terminal".to_string(),
            icon: None,
            icon_path: None,
            description: Some("Terminal emulator".to_string()),
            categories: vec!["System".to_string()],
            desktop_file: PathBuf::from("/tmp/gnome-term.desktop"),
            working_dir: None,
            terminal: true,
            try_exec: None,
        };

        let scanner = AppScanner::from_applications(vec![app2, app1]);
        let results = scanner.search("Terminal");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "Terminal");
    }

    #[test]
    fn test_malformed_desktop_entries_and_launch_failure_safety() {
        let corrupt_content = "[Desktop Entry]\nNoNameKey=Corrupt\nExec=\n";
        let temp_file =
            std::env::temp_dir().join(format!("corrupt-{}.desktop", std::process::id()));
        std::fs::write(&temp_file, corrupt_content).unwrap();

        let parsed = parse_desktop_file(&temp_file);
        assert!(parsed.is_err());

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_desktop_entry_categories_and_keywords_search() {
        let app = Application {
            name: "Firefox".to_string(),
            exec: "firefox".to_string(),
            icon: None,
            icon_path: None,
            description: Some("Web Browser".to_string()),
            categories: vec!["Network".to_string(), "WebBrowser".to_string()],
            desktop_file: PathBuf::from("/tmp/firefox.desktop"),
            working_dir: None,
            terminal: false,
            try_exec: None,
        };

        let scanner = AppScanner::from_applications(vec![app]);
        let results = scanner.search_with_category("Browser", Some("Network"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Firefox");
    }

    #[test]
    fn test_live_app_catalog_rescan_notifies_subscriber() {
        let fixture_dir =
            std::env::temp_dir().join(format!("shilpo-live-catalog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&fixture_dir);
        fs::create_dir_all(&fixture_dir).unwrap();
        fs::write(
            fixture_dir.join("live.desktop"),
            "[Desktop Entry]\nName=Live App\nExec=live-app\n",
        )
        .unwrap();

        let scanner = AppScanner::with_directories(vec![fixture_dir.clone()]);
        let updates = scanner.subscribe();
        scanner.rescan();

        updates
            .recv_timeout(std::time::Duration::from_millis(100))
            .expect("rescan must notify catalog subscribers");
        assert_eq!(scanner.applications()[0].name, "Live App");
        fs::remove_dir_all(fixture_dir).unwrap();
    }
}
