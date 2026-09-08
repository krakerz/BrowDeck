use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    pub fullscreen: bool,
    pub width: u32,
    pub height: u32,
    /// Whether the thin title header above the toolbar shows — see
    /// `HEADER_HEIGHT` in `app.rs`. Defaults to on since its purpose
    /// (keeping the toolbar's own icons clear of Steam/gamescope's
    /// performance overlay) matters most on the primary target
    /// (Steam Deck/Game Mode); off it entirely for a plain desktop
    /// window where there's no overlay to avoid.
    pub show_header: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fullscreen: true,
            width: 1280,
            height: 800,
            show_header: true,
        }
    }
}

/// Written to a fresh `config.toml` the first time BrowDeck runs with
/// none present, so the defaults that matter for the Deck (fullscreen,
/// the header) are visible and already set, not just implicit — the
/// user's own request: "when first time launch, will create a default
/// configuration... so this config will be changable, if user want to
/// hide it". `width`/`height` are left commented (as in
/// `config/config.example.toml`) since they're ignored while
/// `fullscreen = true`.
const DEFAULT_CONFIG_TOML: &str = "\
# BrowDeck configuration — generated on first launch.
# See config.example.toml (shipped alongside the binary) for every
# available key; this file only sets the ones that matter out of the box.

# Launch fullscreen at the display's native resolution — the intended
# mode for gamescope / Steam Game Mode. Set to false for a resizable
# desktop window instead.
fullscreen = true

# Thin title header above the toolbar, there to keep the toolbar's own
# icons clear of Steam/gamescope's performance overlay. Set to false to
# hide it (e.g. on a plain desktop window, where there's no overlay to
# avoid).
show_header = true

# Window size when fullscreen = false. Ignored while fullscreen = true.
#width = 1280
#height = 800
";

impl Config {
    /// Loads `$XDG_CONFIG_HOME/browdeck/config.toml` (falling back to
    /// `~/.config/browdeck/config.toml`), or defaults if absent/invalid.
    /// A missing file is also treated as a first launch: `DEFAULT_CONFIG_TOML`
    /// is written out so there's something on disk to edit — best-effort,
    /// silently skipped if the directory can't be created/written to.
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(contents) = std::fs::read_to_string(&path) else {
            write_default_config(&path);
            return Self::default();
        };
        toml::from_str(&contents).unwrap_or_default()
    }
}

fn write_default_config(path: &PathBuf) {
    if let Some(dir) = path.parent()
        && std::fs::create_dir_all(dir).is_ok()
    {
        let _ = std::fs::write(path, DEFAULT_CONFIG_TOML);
    }
}

fn config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("browdeck/config.toml"));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/browdeck/config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_toml_parses_and_matches_the_in_memory_default() {
        let parsed: Config = toml::from_str(DEFAULT_CONFIG_TOML).unwrap();
        assert!(parsed.fullscreen);
        assert!(parsed.show_header);
        assert_eq!(parsed.width, Config::default().width);
        assert_eq!(parsed.height, Config::default().height);
    }

    #[test]
    fn write_default_config_creates_missing_parent_dirs_and_a_parseable_file() {
        let dir = std::env::temp_dir().join(format!(
            "browdeck_test_config_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("nested/config.toml");

        write_default_config(&path);

        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed: Config = toml::from_str(&contents).unwrap();
        assert!(parsed.fullscreen);
        assert!(parsed.show_header);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
