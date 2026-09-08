use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    pub fullscreen: bool,
    pub width: u32,
    pub height: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fullscreen: true,
            width: 1280,
            height: 800,
        }
    }
}

impl Config {
    /// Loads `$XDG_CONFIG_HOME/browdeck/config.toml` (falling back to
    /// `~/.config/browdeck/config.toml`), or defaults if absent/invalid.
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(contents) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        toml::from_str(&contents).unwrap_or_default()
    }
}

fn config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("browdeck/config.toml"));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/browdeck/config.toml"))
}
