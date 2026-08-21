use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use super::SetupAssets;
use super::compositor::Compositor;

pub fn stage_configs(compositor: Compositor) -> Result<(), String> {
    match compositor {
        Compositor::Niri => stage_niri(),
        Compositor::Hyprland => stage_hyprland(),
    }?;
    stage_common()
}

fn config_home() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| home().join(".config"))
}

fn home() -> PathBuf {
    dirs::home_dir().expect("HOME must be set")
}

fn current_bin_path() -> Result<String, String> {
    env::current_exe()
        .map_err(|e| format!("could not resolve current executable path: {e}"))?
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| "current executable path is not valid UTF-8".to_string())
}

fn write_embedded(rel: &str, dest: &Path) -> Result<(), String> {
    let file = SetupAssets::get(rel).ok_or_else(|| format!("missing embedded asset {rel}"))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    fs::write(dest, file.data.as_ref())
        .map_err(|e| format!("could not write {}: {e}", dest.display()))
}

fn write_embedded_rendered(
    rel: &str,
    dest: &Path,
    replacements: &[(&str, &str)],
) -> Result<(), String> {
    let file = SetupAssets::get(rel).ok_or_else(|| format!("missing embedded asset {rel}"))?;
    let mut text = std::str::from_utf8(&file.data)
        .map_err(|e| format!("embedded asset {rel} is not valid UTF-8: {e}"))?
        .to_string();
    for (from, to) in replacements {
        text = text.replace(from, to);
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    fs::write(dest, text).map_err(|e| format!("could not write {}: {e}", dest.display()))
}

const NIRI_CONFIG_D_PLAIN: &[&str] = &[
    "10-input-and-cursor.kdl",
    "20-layout-and-overview.kdl",
    "30-window-rules.kdl",
    "40-environment.kdl",
    "50-startup.kdl",
    "60-animations.kdl",
    "80-layer-rules.kdl",
];

fn stage_niri() -> Result<(), String> {
    let config_home = config_home();
    let bin = current_bin_path()?;

    println!("Staging Niri configuration...");
    write_embedded("niri/config.kdl", &config_home.join("niri/config.kdl"))?;

    let config_d_dir = config_home.join("niri/config.d");
    for name in NIRI_CONFIG_D_PLAIN {
        write_embedded(&format!("niri/config.d/{name}"), &config_d_dir.join(name))?;
    }

    // Rewrite bare `spawn "shilpo" ...` calls to the resolved absolute binary path: Niri
    // spawns commands with its own compositor-managed environment, which does not
    // reliably inherit the shell PATH shilpo was installed into (e.g. ~/.local/bin).
    let settings_target = "spawn \"shilpo\" \"settings\"".to_string();
    let settings_replacement = format!("spawn \"{bin}\" \"settings\"");
    let base_target = "spawn \"shilpo\"".to_string();
    let base_replacement = format!("spawn \"{bin}\"");
    write_embedded_rendered(
        "niri/config.d/70-binds.kdl",
        &config_d_dir.join("70-binds.kdl"),
        &[
            (settings_target.as_str(), settings_replacement.as_str()),
            (base_target.as_str(), base_replacement.as_str()),
        ],
    )?;

    // User-owned extension point: never overwrite it once it exists.
    let user_extra = config_d_dir.join("90-user-extra.kdl");
    if !user_extra.exists() {
        write_embedded("niri/config.d/90-user-extra.kdl", &user_extra)?;
    }

    Ok(())
}

const HYPRLAND_CONFIG_D: &[&str] = &[
    "10-input-and-cursor.lua",
    "20-layout-and-overview.lua",
    "30-window-rules.lua",
    "40-environment.lua",
    "50-startup.lua",
    "60-animations.lua",
    "70-binds.lua",
    "80-layer-rules.lua",
];

fn stage_hyprland() -> Result<(), String> {
    let config_home = config_home();
    let bin = current_bin_path()?;
    let bin_replacement = [("@SHILPO_BIN@", bin.as_str())];

    println!("Staging Hyprland configuration...");
    let hypr_dir = config_home.join("hypr");
    write_embedded("hyprland/hyprland.lua", &hypr_dir.join("hyprland.lua"))?;

    let config_d_dir = hypr_dir.join("config.d");
    for name in HYPRLAND_CONFIG_D {
        write_embedded_rendered(
            &format!("hyprland/config.d/{name}"),
            &config_d_dir.join(name),
            &bin_replacement,
        )?;
    }

    // User-owned extension point: never overwrite it once it exists.
    let user_extra = hypr_dir.join("shilpo-user-extra.lua");
    if !user_extra.exists() {
        write_embedded("hyprland/shilpo-user-extra.lua", &user_extra)?;
    }

    Ok(())
}

fn stage_common() -> Result<(), String> {
    let config_home = config_home();

    println!("Staging Kitty, Fish, Starship, Swaylock, Swayidle, Shilpo configuration...");
    write_embedded("kitty/kitty.conf", &config_home.join("kitty/kitty.conf"))?;
    write_embedded(
        "starship/starship.toml",
        &config_home.join("starship/starship.toml"),
    )?;
    write_embedded("swaylock/config", &config_home.join("swaylock/config"))?;
    write_embedded("swayidle/config", &config_home.join("swayidle/config"))?;
    write_embedded(
        "fish/shilpo.fish",
        &config_home.join("fish/conf.d/shilpo.fish"),
    )?;
    write_embedded(
        "shilpo/config.toml",
        &config_home.join("shilpo/config.toml"),
    )?;

    let wallpapers_dir = home().join("Pictures/Wallpapers");
    let default_wallpaper = wallpapers_dir.join("shilpo-default.png");
    if !default_wallpaper.exists() {
        write_embedded("wallpapers/shilpo-default.png", &default_wallpaper)?;
    }

    Ok(())
}
