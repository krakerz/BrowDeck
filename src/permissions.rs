use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub fn read_mode(path: &Path) -> Option<u32> {
    std::fs::metadata(path)
        .ok()
        .map(|m| m.permissions().mode() & 0o777)
}

/// Whether `chmod`ing every one of `paths` can actually succeed for the
/// current user — on Linux that's governed by file *ownership*, not the
/// current permission bits: only the owner (or root) can change a file's
/// mode, regardless of what it's currently set to. Used to grey out the
/// Permissions action instead of letting the user hit an `EPERM` per
/// path (`set_mode` already tolerates and logs individual failures, but
/// showing "this won't work" upfront is better than a silent post-hoc
/// log line). `false` for an empty slice or if any path's metadata can't
/// be read at all.
pub fn can_chmod(paths: &[PathBuf]) -> bool {
    if paths.is_empty() {
        return false;
    }
    let euid = current_euid();
    if euid == 0 {
        return true;
    }
    paths
        .iter()
        .all(|p| std::fs::metadata(p).is_ok_and(|m| m.uid() == euid))
}

/// The current process's effective UID, read from `/proc/self/status` —
/// deliberately not a `libc`/`nix` dependency, matching this project's
/// existing preference for parsing `/proc`/`/etc` directly (see
/// `fileinfo::user_name`/`group_name`). Falls back to `u32::MAX` (never
/// equal to a real uid or to root's `0`) if unreadable, which just means
/// `can_chmod` always reports `false` rather than risk a false positive.
fn current_euid() -> u32 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|contents| parse_euid(&contents))
        .unwrap_or(u32::MAX)
}

/// `Uid:` line format is `real  effective  saved  fs`, whitespace-
/// separated — split out from `current_euid` so it's testable without
/// touching `/proc`.
fn parse_euid(status_contents: &str) -> Option<u32> {
    status_contents.lines().find_map(|line| {
        let rest = line.strip_prefix("Uid:")?;
        rest.split_whitespace().nth(1)?.parse().ok()
    })
}

pub fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    let mut perms = std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .permissions();
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms).map_err(|e| e.to_string())
}

/// `mode` (the low 9 bits) as `rwxrwxrwx`.
pub fn format_mode(mode: u32) -> String {
    let mut s = String::with_capacity(9);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        s.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        s.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        s.push(if bits & 0o1 != 0 { 'x' } else { '-' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_mode_matches_ls_style_output() {
        assert_eq!(format_mode(0o755), "rwxr-xr-x");
        assert_eq!(format_mode(0o644), "rw-r--r--");
        assert_eq!(format_mode(0o000), "---------");
        assert_eq!(format_mode(0o777), "rwxrwxrwx");
    }

    #[test]
    fn parse_euid_reads_the_second_uid_field() {
        let status = "Name:\tbash\nUmask:\t0022\nState:\tS (sleeping)\nUid:\t1000\t1000\t1000\t1000\nGid:\t1000\t1000\t1000\t1000\n";
        assert_eq!(parse_euid(status), Some(1000));
    }

    #[test]
    fn parse_euid_is_none_without_a_uid_line() {
        assert_eq!(parse_euid("Name:\tbash\n"), None);
    }

    #[test]
    fn can_chmod_is_false_for_an_empty_selection() {
        assert!(!can_chmod(&[]));
    }

    #[test]
    fn can_chmod_is_true_for_files_the_current_user_owns() {
        let dir = std::env::temp_dir().join(format!(
            "browdeck_test_can_chmod_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("owned.txt");
        std::fs::write(&file, b"hi").unwrap();

        assert!(can_chmod(&[file]));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
