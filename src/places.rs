use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_ARTICLE, ICON_DESKTOP_WINDOWS, ICON_DOWNLOAD, ICON_HOME, ICON_IMAGE, ICON_LIBRARY_MUSIC,
    ICON_MOVIE,
};
use std::path::PathBuf;

pub struct Place {
    pub label: String,
    pub path: PathBuf,
    pub icon: MaterialIcon,
}

/// Home plus whichever XDG user dirs (`~/.config/user-dirs.dirs`) exist on disk.
pub fn list_places() -> Vec<Place> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut places = vec![Place {
        label: "Home".into(),
        path: PathBuf::from(&home),
        icon: ICON_HOME,
    }];

    let dirs_file = PathBuf::from(&home).join(".config/user-dirs.dirs");
    if let Ok(contents) = std::fs::read_to_string(&dirs_file) {
        // Compare against the canonical form of $HOME so a trailing slash
        // or symlink component doesn't defeat the dedup below.
        let home_canonical = PathBuf::from(&home).canonicalize().ok();
        for (key, label, icon) in [
            ("XDG_DESKTOP_DIR", "Desktop", ICON_DESKTOP_WINDOWS),
            ("XDG_DOWNLOAD_DIR", "Downloads", ICON_DOWNLOAD),
            ("XDG_DOCUMENTS_DIR", "Documents", ICON_ARTICLE),
            ("XDG_MUSIC_DIR", "Music", ICON_LIBRARY_MUSIC),
            ("XDG_PICTURES_DIR", "Pictures", ICON_IMAGE),
            ("XDG_VIDEOS_DIR", "Videos", ICON_MOVIE),
        ] {
            if let Some(path) = parse_dirs_entry(&contents, key, &home)
                && path.is_dir()
                // A user-dir entry that resolves to $HOME itself (seen in
                // the wild as a misconfigured `XDG_DOWNLOAD_DIR="$HOME/"`
                // in ~/.config/user-dirs.dirs, rather than
                // `"$HOME/Downloads"`) would just duplicate the Home row
                // and silently show the wrong folder — skip it rather than
                // add a place that looks broken. This is a system
                // config issue, not something to silently paper over by
                // guessing a path instead.
                && path.canonicalize().ok() != home_canonical
            {
                places.push(Place {
                    label: label.into(),
                    path,
                    icon,
                });
            }
        }
    }
    places
}

fn parse_dirs_entry(contents: &str, key: &str, home: &str) -> Option<PathBuf> {
    let prefix = format!("{key}=\"");
    let line = contents
        .lines()
        .find(|l| l.trim_start().starts_with(&prefix))?;
    let value = line.trim_start().strip_prefix(&prefix)?.strip_suffix('"')?;
    Some(PathBuf::from(value.replace("$HOME", home)))
}
