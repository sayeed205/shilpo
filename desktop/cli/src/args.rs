use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "shilpo",
    author,
    version,
    about = "Shilpo Shell Control & Desktop Utility CLI",
    long_about = None
)]
pub struct Cli {
    /// Machine-readable JSON output
    #[arg(global = true, long)]
    pub json: bool,

    /// Suppress human output (errors still printed to stderr)
    #[arg(global = true, short, long)]
    pub quiet: bool,

    /// Command timeout duration (e.g. 10s, 500ms, 10)
    #[arg(global = true, long, value_name = "DURATION")]
    pub timeout: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Shell daemon lifecycle, status, and telemetry
    Shell {
        #[command(subcommand)]
        command: ShellCommands,
    },
    /// Workspace overview visibility
    Overview {
        #[command(subcommand)]
        action: VisibilityAction,
    },
    /// Control center visibility
    ControlCenter {
        #[command(subcommand)]
        action: VisibilityAction,
    },
    /// Status bar visibility
    Bar {
        #[command(subcommand)]
        action: VisibilityAction,
    },
    /// Workspace navigation and layout
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
    /// Window focus and layout
    Window {
        #[command(subcommand)]
        command: WindowCommands,
    },
    /// Shell configuration inspection and validation
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Theme mode, seed color, and wallpaper management
    Theme {
        #[command(subcommand)]
        command: ThemeCommands,
    },
    /// Environment and runtime diagnostic checks
    Doctor {
        /// Attempt automatic fixes for warnings/errors
        #[arg(long)]
        fix: bool,
        /// Run as first-login one-shot task: write reports, send desktop notification, mark complete
        #[arg(long)]
        first_login: bool,
    },
    /// Shell extension development, packaging, and catalog management
    Ext {
        #[command(subcommand)]
        command: ExtCommands,
    },
    /// Native screen capture and annotation
    Capture {
        #[command(subcommand)]
        action: CaptureAction,
    },
    /// Native screen recording pipeline and session control
    Record {
        #[command(subcommand)]
        action: RecordAction,
    },
    /// Display brightness controls
    Brightness {
        #[command(subcommand)]
        command: BrightnessCommands,
    },
}

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureAction {
    /// Select a region and copy PNG to clipboard immediately
    Region,
    /// Select a region and open the annotation editor
    Edit,
    /// Select a region and run OCR, copying recognized text
    Ocr,
    /// Open the capture menu (selection shape chooser)
    Menu,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum RecordAction {
    /// Start recording, or stop and finalize the active recording
    Toggle,
    /// Choose a display output and start video recording
    Start {
        /// Optional display output name to record (defaults to primary output)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Pause the active recording
    Pause,
    /// Resume the paused recording
    Resume,
    /// Stop and finalize the recording
    Stop,
    /// Cancel and discard the recording
    Cancel,
    /// Query the current recording state
    Status,
}

#[derive(Subcommand, Debug, Clone)]
pub enum BrightnessCommands {
    /// List detected displays and current brightness percentages
    List,
    /// Set brightness for a specific display or all displays
    Set {
        /// Target display connector or ID (e.g. "DP-1" or "ddc:i2c-3")
        #[arg(long)]
        display: Option<String>,
        /// Target brightness percentage (0..=100)
        value: u8,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ShellCommands {
    /// Print status of shell daemon and services
    Status,
    /// Start shell daemon via systemd user unit
    Start,
    /// Stop shell daemon via systemd user unit
    Stop,
    /// Restart shell daemon via systemd user unit
    Restart,
    /// Stream or view shell daemon logs via journald
    Logs {
        /// Stream new log output continuously
        #[arg(short, long)]
        follow: bool,
        /// Show logs since specified time (e.g. "10m ago")
        #[arg(long)]
        since: Option<String>,
        /// Number of recent log lines to display
        #[arg(short = 'n', long)]
        lines: Option<usize>,
    },
    /// Query service health and compositor broker telemetry
    Telemetry,
}

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityAction {
    /// Show component idempotently
    Show,
    /// Hide component idempotently
    Hide,
    /// Toggle component visibility
    Toggle,
}

#[derive(Subcommand, Debug, Clone)]
pub enum WorkspaceCommands {
    /// Focus workspace by numeric ID
    Focus { id: u64 },
    /// Create and activate a new empty workspace
    Create,
}

#[derive(Subcommand, Debug, Clone)]
pub enum WindowCommands {
    /// Focus window by numeric ID
    Focus { id: u64 },
    /// Focus previously active window
    FocusPrevious,
    /// Move window to specified workspace
    Move {
        id: u64,
        #[arg(long)]
        workspace: u64,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCommands {
    /// Print active configuration file path
    Path,
    /// Validate configuration file syntax and values
    Validate,
    /// Signal running daemon to reload configuration
    Reload,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ThemeCommands {
    /// Theme mode management (get, set, toggle)
    Mode {
        #[command(subcommand)]
        action: ThemeModeAction,
    },
    /// Theme seed color management
    Seed {
        #[command(subcommand)]
        action: ThemeSeedAction,
    },
    /// Wallpaper management
    Wallpaper {
        #[command(subcommand)]
        action: ThemeWallpaperAction,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ThemeModeAction {
    /// Get current theme mode
    Get,
    /// Set theme mode
    Set { mode: ModeValue },
    /// Toggle theme mode
    Toggle,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeValue {
    Light,
    Dark,
    System,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ThemeSeedAction {
    /// Set custom color seed (HEX string or integer)
    Set { color: String },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ThemeWallpaperAction {
    /// Show the current wallpaper and wallpaper directory
    Get,
    /// Set wallpaper file path
    Set { path: PathBuf },
    /// Select random wallpaper from directory
    Random,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ExtCommands {
    /// Inspect extension manifest, component, and assets
    Check { path: Option<PathBuf> },
    /// Pack extension directory into .shilpo-ext bundle
    Pack {
        path: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Register extension directory for development override
    Dev { path: PathBuf },
    /// Reload extension development override
    Reload { id: Option<String> },
    /// View development extension logs
    Logs {
        id: Option<String>,
        #[arg(short, long)]
        follow: bool,
    },
    /// List active and development extensions
    List {
        #[arg(long)]
        dev: bool,
    },
    /// Search extension catalog
    Search { query: Option<String> },
    /// Show detailed info for extension ID
    Info { id: String },
    /// Install extension package or catalog ID
    Install {
        target: String,
        #[arg(long)]
        hash: Option<String>,
    },
    /// Update extensions
    Update {
        id: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Enable installed extension
    Enable { id: String },
    /// Disable installed extension
    Disable { id: String },
    /// Approve pending extension permissions
    Approve {
        id: String,
        #[arg(long)]
        grant_all: bool,
    },
    /// Rollback extension to previous version
    Rollback { id: String },
    /// Uninstall extension
    Uninstall { id: String },
    /// Check for catalog extension updates
    CheckUpdates,
    /// Manage extension release channel
    Channel { id: String, channel: Option<String> },
    /// Manage extension registry sources
    Source {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Refresh extension registry sources
    RefreshSources,
    /// Sign extension package with key
    Sign {
        package: PathBuf,
        key: PathBuf,
        #[arg(long)]
        publisher: String,
    },
    /// Generate ed25519 signing keypair
    Keygen { output: PathBuf },
}
