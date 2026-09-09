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
    list_places_for_home(&home)
}

/// Split out from `list_places` so it's testable against a fake `$HOME`
/// without touching the real one — same pattern as
/// `iprolaunch::resolve_bin`. A user dir is included as long as it
/// resolves to a real directory, symlink or not (`Path::is_dir`/
/// `canonicalize` both follow symlinks transparently, so an unmounted
/// symlink target — the drive it points at isn't plugged in/mounted —
/// naturally excludes it exactly like a plain missing directory would,
/// no special-casing needed).
///
/// `~/.config/user-dirs.dirs` (written by `xdg-user-dirs-update`) is
/// consulted first for each dir, but a plain `$HOME/<Name>` (`Desktop`,
/// `Downloads`, …) is always tried as a fallback — when that file is
/// missing entirely, when it just doesn't define that particular key,
/// *or* when the entry it does have resolves to `$HOME` itself (a real,
/// seen-in-the-wild misconfiguration: `XDG_DOWNLOAD_DIR="$HOME/"`
/// instead of `"$HOME/Downloads"`, which used to just silently drop the
/// Place entirely even when a perfectly good `$HOME/Downloads` — symlink
/// or not — existed right there). SteamOS/a bare `$HOME` don't reliably
/// ship a fully-populated `user-dirs.dirs` either, and "the conventional
/// name under `$HOME`" is what most file managers already fall back to.
fn list_places_for_home(home: &str) -> Vec<Place> {
    let mut places = vec![Place {
        label: "Home".into(),
        path: PathBuf::from(home),
        icon: ICON_HOME,
    }];

    // Compare against the canonical form of $HOME so a trailing slash
    // or symlink component doesn't defeat the checks below.
    let home_canonical = PathBuf::from(home).canonicalize().ok();
    let resolves_to_home = |p: &PathBuf| p.canonicalize().ok() == home_canonical;
    let dirs_file = PathBuf::from(home).join(".config/user-dirs.dirs");
    let dirs_contents = std::fs::read_to_string(&dirs_file).ok();

    for (key, name, icon) in [
        ("XDG_DESKTOP_DIR", "Desktop", ICON_DESKTOP_WINDOWS),
        ("XDG_DOWNLOAD_DIR", "Downloads", ICON_DOWNLOAD),
        ("XDG_DOCUMENTS_DIR", "Documents", ICON_ARTICLE),
        ("XDG_MUSIC_DIR", "Music", ICON_LIBRARY_MUSIC),
        ("XDG_PICTURES_DIR", "Pictures", ICON_IMAGE),
        ("XDG_VIDEOS_DIR", "Videos", ICON_MOVIE),
    ] {
        let configured = dirs_contents
            .as_deref()
            .and_then(|c| parse_dirs_entry(c, key, home));
        let path = match configured {
            Some(p) if !resolves_to_home(&p) => p,
            _ => PathBuf::from(home).join(name),
        };

        // A path that *still* resolves to $HOME after falling back (or
        // that isn't a real directory at all — a broken symlink whose
        // target isn't mounted, say) would either duplicate the Home
        // row or show a Place that errors out when clicked — skip it
        // rather than add either.
        if path.is_dir() && !resolves_to_home(&path) {
            places.push(Place {
                label: name.into(),
                path,
                icon,
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "browdeck_test_places_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// A symlinked XDG dir (e.g. `~/Downloads` pointed at another mount,
    /// a common real-world setup) should show up in Places exactly like
    /// a plain directory — `Path::is_dir`/`canonicalize` both follow
    /// symlinks transparently, so nothing special is needed for this to
    /// already work, but it's worth pinning down with a real symlink
    /// rather than trusting that reasoning alone.
    #[test]
    fn symlinked_xdg_dir_appears_in_places() {
        let home = temp_dir("home_symlink");
        let real_target = temp_dir("real_downloads_target");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&real_target).unwrap();
        std::fs::create_dir_all(home.join(".config")).unwrap();

        let downloads_link = home.join("Downloads");
        std::os::unix::fs::symlink(&real_target, &downloads_link).unwrap();

        std::fs::write(
            home.join(".config/user-dirs.dirs"),
            format!("XDG_DOWNLOAD_DIR=\"{}\"\n", downloads_link.display()),
        )
        .unwrap();

        let places = list_places_for_home(&home.to_string_lossy());
        let downloads = places.iter().find(|p| p.label == "Downloads");
        assert!(downloads.is_some(), "Downloads place should be present");
        let downloads = downloads.unwrap();
        // Stored path is the symlink itself (not pre-resolved) — matches
        // how a normal directory entry is stored, and lets navigating
        // "up" from inside it go back to where the symlink lives.
        assert_eq!(downloads.path, downloads_link);
        assert_eq!(downloads.path.canonicalize().unwrap(), real_target);

        std::fs::remove_dir_all(&home).unwrap();
        std::fs::remove_dir_all(&real_target).unwrap();
    }

    /// A broken symlink (dangling target, e.g. an unplugged drive) should
    /// be silently excluded rather than shown as a Place that errors out
    /// when clicked.
    #[test]
    fn broken_symlinked_xdg_dir_is_excluded() {
        let home = temp_dir("home_broken_symlink");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(home.join(".config")).unwrap();

        let downloads_link = home.join("Downloads");
        std::os::unix::fs::symlink(home.join("does-not-exist"), &downloads_link).unwrap();

        std::fs::write(
            home.join(".config/user-dirs.dirs"),
            format!("XDG_DOWNLOAD_DIR=\"{}\"\n", downloads_link.display()),
        )
        .unwrap();

        let places = list_places_for_home(&home.to_string_lossy());
        assert!(!places.iter().any(|p| p.label == "Downloads"));

        std::fs::remove_dir_all(&home).unwrap();
    }

    /// No `user-dirs.dirs` at all (never generated, or a bare `$HOME`
    /// like some SteamOS setups) — conventional `$HOME/Downloads` etc.
    /// should still be picked up directly, symlinked or not.
    #[test]
    fn falls_back_to_conventional_names_with_no_user_dirs_file() {
        let home = temp_dir("home_no_dirs_file");
        let real_target = temp_dir("real_downloads_no_dirs_file");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&real_target).unwrap();
        std::os::unix::fs::symlink(&real_target, home.join("Downloads")).unwrap();
        std::fs::create_dir_all(home.join("Pictures")).unwrap();

        let places = list_places_for_home(&home.to_string_lossy());
        assert!(places.iter().any(|p| p.label == "Downloads"));
        assert!(places.iter().any(|p| p.label == "Pictures"));

        std::fs::remove_dir_all(&home).unwrap();
        std::fs::remove_dir_all(&real_target).unwrap();
    }

    /// `user-dirs.dirs` exists but doesn't mention this particular key
    /// (e.g. only `XDG_DESKTOP_DIR` was ever written) — still falls back
    /// to `$HOME/Downloads` for the rest rather than only ever trusting
    /// the file wholesale.
    #[test]
    fn falls_back_to_conventional_name_for_a_key_missing_from_user_dirs_file() {
        let home = temp_dir("home_partial_dirs_file");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(home.join(".config")).unwrap();
        std::fs::create_dir_all(home.join("Downloads")).unwrap();

        std::fs::write(
            home.join(".config/user-dirs.dirs"),
            format!("XDG_DESKTOP_DIR=\"{}/Desktop\"\n", home.display()),
        )
        .unwrap();

        let places = list_places_for_home(&home.to_string_lossy());
        assert!(places.iter().any(|p| p.label == "Downloads"));

        std::fs::remove_dir_all(&home).unwrap();
    }

    /// The exact real-world case this whole fallback was added for:
    /// `XDG_DOWNLOAD_DIR="$HOME/"` (a real misconfiguration, not a
    /// hypothetical — found in this project's own dev machine's
    /// `user-dirs.dirs`) used to make Downloads disappear entirely, even
    /// with a perfectly good symlinked `$HOME/Downloads` sitting right
    /// there. Falls back to the conventional name instead of giving up.
    #[test]
    fn falls_back_to_conventional_name_when_configured_entry_resolves_to_home_itself() {
        let home = temp_dir("home_download_dir_is_home");
        let real_target = temp_dir("real_downloads_is_home_case");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&real_target).unwrap();
        std::fs::create_dir_all(home.join(".config")).unwrap();
        std::os::unix::fs::symlink(&real_target, home.join("Downloads")).unwrap();

        std::fs::write(
            home.join(".config/user-dirs.dirs"),
            format!("XDG_DOWNLOAD_DIR=\"{}/\"\n", home.display()),
        )
        .unwrap();

        let places = list_places_for_home(&home.to_string_lossy());
        let downloads = places.iter().find(|p| p.label == "Downloads");
        assert!(downloads.is_some(), "Downloads place should be present");
        assert_eq!(downloads.unwrap().path.canonicalize().unwrap(), real_target);

        std::fs::remove_dir_all(&home).unwrap();
        std::fs::remove_dir_all(&real_target).unwrap();
    }
}
