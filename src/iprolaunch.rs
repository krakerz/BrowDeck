use std::path::{Path, PathBuf};
use std::process::Command;

/// Detects the IProLaunch CLI binary. `~/.config/iprolaunch/bin-path` is
/// itself a small config file whose *contents* are the actual path to the
/// binary — not the binary itself. Checked once at startup (see
/// `BrowDeckApp::new`), not every frame, since it never changes while the
/// app is running.
pub fn detect_bin() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let config_path = PathBuf::from(home).join(".config/iprolaunch/bin-path");
    resolve_bin(&config_path)
}

/// Reads `config_path` (the `bin-path` config file) and resolves it to a
/// real binary — split out from `detect_bin` so this logic is testable
/// without touching the real `$HOME`.
fn resolve_bin(config_path: &Path) -> Option<PathBuf> {
    let contents = std::fs::read_to_string(config_path).ok()?;
    let bin_path = PathBuf::from(contents.trim());
    bin_path.is_file().then_some(bin_path)
}

/// Whether `path` looks like something IProLaunch would run (`.exe`/`.bat`).
pub fn is_launchable(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("bat"))
}

/// Registers `target` with IProLaunch: `<bin> add <target>`.
pub fn add(bin: &Path, target: &Path) -> std::io::Result<()> {
    Command::new(bin).arg("add").arg(target).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_launchable_matches_exe_and_bat_case_insensitively() {
        assert!(is_launchable(Path::new("Game.exe")));
        assert!(is_launchable(Path::new("setup.BAT")));
        assert!(!is_launchable(Path::new("readme.txt")));
        assert!(!is_launchable(Path::new("no_extension")));
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "browdeck_test_iprolaunch_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_bin_follows_the_config_file_to_the_real_binary() {
        let dir = scratch_dir("resolve_ok");
        let real_bin = dir.join("iprolaunch");
        std::fs::write(&real_bin, b"#!/bin/sh\n").unwrap();
        let config = dir.join("bin-path");
        // Trailing newline, like a config file a human/editor saved.
        std::fs::write(&config, format!("{}\n", real_bin.display())).unwrap();

        assert_eq!(resolve_bin(&config), Some(real_bin));
    }

    #[test]
    fn resolve_bin_is_none_when_the_config_file_is_missing() {
        let dir = scratch_dir("resolve_missing_config");
        assert_eq!(resolve_bin(&dir.join("bin-path")), None);
    }

    #[test]
    fn resolve_bin_is_none_when_the_referenced_binary_does_not_exist() {
        let dir = scratch_dir("resolve_missing_bin");
        let config = dir.join("bin-path");
        std::fs::write(&config, dir.join("does-not-exist").display().to_string()).unwrap();

        assert_eq!(resolve_bin(&config), None);
    }
}
