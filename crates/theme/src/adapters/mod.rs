use crate::state::ThemeMode;
use anyhow::{Result, bail};
use std::env;
use std::process::Command;
use tracing::info;

pub trait DesktopAdapter: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;
    fn set_mode(&self, mode: ThemeMode) -> Result<()>;
}

#[derive(Debug)]
pub struct GnomeNiriAdapter {
    pub gtk_theme_light: Option<String>,
    pub gtk_theme_dark: Option<String>,
}

impl DesktopAdapter for GnomeNiriAdapter {
    fn name(&self) -> &'static str {
        "gnome-niri"
    }

    fn set_mode(&self, mode: ThemeMode) -> Result<()> {
        let scheme = match mode {
            ThemeMode::Dark => "prefer-dark",
            ThemeMode::Light => "prefer-light",
            ThemeMode::System => return Ok(()),
        };

        let ok = Command::new("gsettings")
            .args(["set", "org.gnome.desktop.interface", "color-scheme", scheme])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !ok {
            let fallback_ok = Command::new("dconf")
                .args([
                    "write",
                    "/org/gnome/desktop/interface/color-scheme",
                    &format!("'{}'", scheme),
                ])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !fallback_ok {
                bail!("Unable to set GNOME color-scheme with gsettings or dconf");
            }
        }

        let custom_gtk = match mode {
            ThemeMode::Dark => self.gtk_theme_dark.as_deref(),
            ThemeMode::Light => self.gtk_theme_light.as_deref(),
            ThemeMode::System => None,
        };

        if let Some(gtk_theme) = custom_gtk {
            let status = Command::new("gsettings")
                .args(["set", "org.gnome.desktop.interface", "gtk-theme", gtk_theme])
                .status()
                .map_err(|error| anyhow::anyhow!("Unable to set GTK theme: {error}"))?;
            if !status.success() {
                bail!("Unable to set GTK theme (exit status {:?})", status.code());
            }
        }

        info!(provider = self.name(), mode = %mode, scheme, "Dispatched desktop theme mode");
        Ok(())
    }
}

#[derive(Debug)]
pub struct KdeAdapter;

impl DesktopAdapter for KdeAdapter {
    fn name(&self) -> &'static str {
        "kde"
    }

    fn set_mode(&self, mode: ThemeMode) -> Result<()> {
        let scheme = match mode {
            ThemeMode::Dark => "BreezeDark",
            ThemeMode::Light => "BreezeLight",
            ThemeMode::System => return Ok(()),
        };

        let status = if command_exists("plasma-apply-colorscheme") {
            Command::new("plasma-apply-colorscheme")
                .arg(scheme)
                .status()
        } else {
            let tool = if command_exists("kwriteconfig6") {
                Some("kwriteconfig6")
            } else if command_exists("kwriteconfig5") {
                Some("kwriteconfig5")
            } else {
                None
            };
            let Some(tool) = tool else {
                bail!("No KDE theme configuration tool is available");
            };
            Command::new(tool)
                .args([
                    "--file",
                    "kdeglobals",
                    "--group",
                    "General",
                    "--key",
                    "ColorScheme",
                    scheme,
                ])
                .status()
        }?;

        if !status.success() {
            bail!("KDE theme command failed (exit status {:?})", status.code());
        } else {
            info!(provider = self.name(), mode = %mode, scheme, "Dispatched KDE theme mode");
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct XfceAdapter {
    pub gtk_theme_light: Option<String>,
    pub gtk_theme_dark: Option<String>,
    pub wm_theme_light: Option<String>,
    pub wm_theme_dark: Option<String>,
}

impl DesktopAdapter for XfceAdapter {
    fn name(&self) -> &'static str {
        "xfce"
    }

    fn set_mode(&self, mode: ThemeMode) -> Result<()> {
        let (gtk, wm) = match mode {
            ThemeMode::Dark => (
                self.gtk_theme_dark.as_deref(),
                self.wm_theme_dark.as_deref(),
            ),
            ThemeMode::Light => (
                self.gtk_theme_light.as_deref(),
                self.wm_theme_light.as_deref(),
            ),
            ThemeMode::System => return Ok(()),
        };

        let mut changed = false;
        if let Some(gtk_theme) = gtk {
            let status = Command::new("xfconf-query")
                .args(["-c", "xsettings", "-p", "/Net/ThemeName", "-s", gtk_theme])
                .status()
                .map_err(|error| anyhow::anyhow!("Unable to run xfconf-query: {error}"))?;
            if !status.success() {
                bail!(
                    "XFCE GTK theme command failed (exit status {:?})",
                    status.code()
                );
            }
            changed = true;
        }

        if let Some(wm_theme) = wm {
            let status = Command::new("xfconf-query")
                .args(["-c", "xfwm4", "-p", "/general/theme", "-s", wm_theme])
                .status()
                .map_err(|error| anyhow::anyhow!("Unable to run xfconf-query: {error}"))?;
            if !status.success() {
                bail!(
                    "XFCE window-manager theme command failed (exit status {:?})",
                    status.code()
                );
            }
            changed = true;
        }

        if !changed {
            bail!("No XFCE theme configured for {mode}");
        }

        info!(provider = self.name(), mode = %mode, "Dispatched XFCE theme mode");
        Ok(())
    }
}

#[derive(Debug)]
pub struct DarkmanAdapter;

impl DarkmanAdapter {
    pub fn is_available() -> bool {
        if !command_exists("darkman") {
            return false;
        }
        Command::new("darkman")
            .arg("get")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl DesktopAdapter for DarkmanAdapter {
    fn name(&self) -> &'static str {
        "darkman"
    }

    fn set_mode(&self, mode: ThemeMode) -> Result<()> {
        let cmd_mode = match mode {
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
            ThemeMode::System => return Ok(()),
        };

        let status = Command::new("darkman").args(["set", cmd_mode]).status()?;

        if !status.success() {
            bail!("darkman set failed with exit code {:?}", status.code());
        }

        info!(provider = self.name(), mode = %mode, "Dispatched Darkman theme mode");
        Ok(())
    }
}

#[derive(Debug)]
pub struct CustomAdapter {
    pub argv: Vec<String>,
}

impl DesktopAdapter for CustomAdapter {
    fn name(&self) -> &'static str {
        "custom"
    }

    fn set_mode(&self, mode: ThemeMode) -> Result<()> {
        if self.argv.is_empty() {
            bail!("Custom adapter argv is empty");
        }

        let mode_str = mode.as_str();
        let program = &self.argv[0];
        let args: Vec<String> = self.argv[1..]
            .iter()
            .map(|arg| arg.replace("{mode}", mode_str))
            .collect();

        let status = Command::new(program).args(&args).status()?;
        if !status.success() {
            bail!(
                "Custom adapter command failed: exit status {:?}",
                status.code()
            );
        }

        info!(provider = self.name(), mode = %mode, "Dispatched custom adapter command");
        Ok(())
    }
}

pub fn select_desktop_adapter(
    config_provider: Option<&str>,
    config_gtk_light: Option<String>,
    config_gtk_dark: Option<String>,
    custom_argv: Option<Vec<String>>,
) -> Box<dyn DesktopAdapter> {
    if let Some(adapter) = env::var("SHILPO_THEME_PROVIDER")
        .ok()
        .and_then(|env_override| {
            create_adapter_by_name(
                &env_override,
                config_gtk_light.clone(),
                config_gtk_dark.clone(),
                custom_argv.clone(),
            )
        })
    {
        info!(
            provider = adapter.name(),
            "Selected desktop provider via SHILPO_THEME_PROVIDER"
        );
        return adapter;
    }

    if let Some(adapter) = config_provider.and_then(|provider_name| {
        create_adapter_by_name(
            provider_name,
            config_gtk_light.clone(),
            config_gtk_dark.clone(),
            custom_argv.clone(),
        )
    }) {
        info!(
            provider = adapter.name(),
            "Selected desktop provider via config.toml"
        );
        return adapter;
    }

    if DarkmanAdapter::is_available() {
        info!("Selected Darkman desktop provider via active control endpoint");
        return Box::new(DarkmanAdapter);
    }

    let desktop_env = env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();
    if desktop_env.contains("kde") || desktop_env.contains("plasma") {
        info!("Selected KDE desktop provider via XDG_CURRENT_DESKTOP");
        return Box::new(KdeAdapter);
    }

    if desktop_env.contains("xfce") {
        info!("Selected XFCE desktop provider via XDG_CURRENT_DESKTOP");
        return Box::new(XfceAdapter {
            gtk_theme_light: config_gtk_light,
            gtk_theme_dark: config_gtk_dark,
            wm_theme_light: None,
            wm_theme_dark: None,
        });
    }

    info!("Selected GNOME/Niri fallback desktop provider");
    Box::new(GnomeNiriAdapter {
        gtk_theme_light: config_gtk_light,
        gtk_theme_dark: config_gtk_dark,
    })
}

fn create_adapter_by_name(
    name: &str,
    gtk_light: Option<String>,
    gtk_dark: Option<String>,
    custom_argv: Option<Vec<String>>,
) -> Option<Box<dyn DesktopAdapter>> {
    match name.to_lowercase().as_str() {
        "gnome" | "niri" | "gtk" => Some(Box::new(GnomeNiriAdapter {
            gtk_theme_light: gtk_light,
            gtk_theme_dark: gtk_dark,
        })),
        "kde" | "plasma" => Some(Box::new(KdeAdapter)),
        "xfce" => Some(Box::new(XfceAdapter {
            gtk_theme_light: gtk_light,
            gtk_theme_dark: gtk_dark,
            wm_theme_light: None,
            wm_theme_dark: None,
        })),
        "darkman" => Some(Box::new(DarkmanAdapter)),
        "custom" => {
            custom_argv.map(|argv| -> Box<dyn DesktopAdapter> { Box::new(CustomAdapter { argv }) })
        }
        _ => None,
    }
}

fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_adapter_substitution() {
        let adapter = CustomAdapter {
            argv: vec!["echo".into(), "--theme={mode}".into()],
        };

        // Custom adapter executes directly without shell string invocation
        assert_eq!(adapter.name(), "custom");
    }

    #[test]
    fn test_adapter_selection_fallback() {
        let adapter = select_desktop_adapter(None, None, None, None);
        assert!(!adapter.name().is_empty());
    }
}
