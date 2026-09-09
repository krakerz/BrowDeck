use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// User-pinned folders — "Pin to Sidebar" in the Actions strip, folders
/// only. Shown in the sidebar right after the XDG Places list
/// (Home/Desktop/Downloads/…), before Mounts. Persisted to
/// `~/.config/browdeck/custom_places.toml` (or
/// `$XDG_CONFIG_HOME/browdeck/custom_places.toml`), independent of
/// `config.toml` since this list grows/shrinks at runtime while that
/// file is meant to stay hand-edited.
#[derive(Serialize, Deserialize, Default)]
struct CustomPlacesFile {
    #[serde(default)]
    paths: Vec<PathBuf>,
}

/// Loads the pinned folder list, dropping anything that's no longer a
/// real directory (deleted, or a symlink whose target isn't mounted
/// right now) rather than show a Place that errors out when clicked —
/// same reasoning as `places::list_places_for_home`'s own filtering.
pub fn load() -> Vec<PathBuf> {
    let Some(path) = config_path() else {
        return Vec::new();
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let file: CustomPlacesFile = toml::from_str(&contents).unwrap_or_default();
    file.paths.into_iter().filter(|p| p.is_dir()).collect()
}

/// Writes the full list back — callers mutate their own in-memory copy
/// (push/remove) and call this to persist it, rather than this module
/// tracking state itself.
pub fn save(paths: &[PathBuf]) {
    let Some(path) = config_path() else {
        return;
    };
    let Some(dir) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let file = CustomPlacesFile {
        paths: paths.to_vec(),
    };
    if let Ok(contents) = toml::to_string_pretty(&file) {
        let _ = std::fs::write(path, contents);
    }
}

/// Whether `path` is already pinned — compares canonicalized forms so
/// the same real folder reached via two different-looking paths (a
/// symlink vs. its target, a trailing slash, …) is still recognized as
/// already-pinned instead of allowing a duplicate.
pub fn contains(paths: &[PathBuf], path: &Path) -> bool {
    let target = path.canonicalize().ok();
    target.is_some() && paths.iter().any(|p| p.canonicalize().ok() == target)
}

fn config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("browdeck/custom_places.toml"));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/browdeck/custom_places.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "browdeck_test_custom_places_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn contains_matches_the_same_folder_reached_via_a_symlink() {
        let real = temp_dir("real");
        let link_parent = temp_dir("link_parent");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(&link_parent).unwrap();
        let link = link_parent.join("alias");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert!(contains(std::slice::from_ref(&real), &link));

        std::fs::remove_dir_all(&real).unwrap();
        std::fs::remove_dir_all(&link_parent).unwrap();
    }

    #[test]
    fn contains_is_false_when_not_bookmarked() {
        let real = temp_dir("real_not_bookmarked");
        std::fs::create_dir_all(&real).unwrap();
        assert!(!contains(&[], &real));
        std::fs::remove_dir_all(&real).unwrap();
    }

    #[test]
    fn save_then_load_round_trips_and_drops_missing_dirs() {
        let dir = temp_dir("config_dir");
        let real = temp_dir("bookmarked");
        std::fs::create_dir_all(&real).unwrap();
        let config_file = dir.join("custom_places.toml");

        let file = CustomPlacesFile {
            paths: vec![real.clone(), dir.join("this-does-not-exist")],
        };
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&config_file, toml::to_string_pretty(&file).unwrap()).unwrap();

        let contents = std::fs::read_to_string(&config_file).unwrap();
        let parsed: CustomPlacesFile = toml::from_str(&contents).unwrap();
        let existing: Vec<PathBuf> = parsed.paths.into_iter().filter(|p| p.is_dir()).collect();
        assert_eq!(existing, vec![real.clone()]);

        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&real).unwrap();
    }
}
