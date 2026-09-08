use crate::permissions;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// A small, formatted snapshot of a single selected file/folder — shown in
/// the sidebar's info card. Everything is pre-formatted to a display
/// string since it's only ever used for that.
pub struct FileInfo {
    pub name: String,
    pub owner: String,
    pub group: String,
    /// Formatted size — for a directory, the recursive total (see
    /// `dir_size` for the scan cap).
    pub size: String,
    /// `rwxrwxrwx`.
    pub permissions: String,
    /// The link target, if `path` is a symlink.
    pub symlink_target: Option<String>,
}

/// Caps the recursive directory-size walk — same idea and cap as
/// `BrowDeckApp::recursive_search`'s scan limit, for the same reason: this
/// runs synchronously on the UI thread every time the selection changes,
/// so an unbounded walk of a huge tree would visibly stall navigation.
const MAX_DIR_SCAN: usize = 20_000;

pub fn compute(path: &Path) -> FileInfo {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    let symlink_target = std::fs::symlink_metadata(path)
        .ok()
        .filter(|m| m.file_type().is_symlink())
        .and_then(|_| std::fs::read_link(path).ok())
        .map(|p| p.to_string_lossy().into_owned());

    // Follows symlinks, matching what a file manager normally shows for a
    // link's own owner/permissions/size (the `symlink_target` line above
    // covers "what it points to" separately).
    let meta = std::fs::metadata(path).ok();
    let (owner, group, permissions) = match &meta {
        Some(m) => (
            user_name(m.uid()),
            group_name(m.gid()),
            permissions::format_mode(m.mode() & 0o777),
        ),
        None => ("?".to_string(), "?".to_string(), "?????????".to_string()),
    };
    let size = match &meta {
        Some(m) if m.is_dir() => {
            let (bytes, capped) = dir_size(path);
            let formatted = format_size(bytes);
            if capped {
                format!("≥ {formatted}")
            } else {
                formatted
            }
        }
        Some(m) => format_size(m.len()),
        None => "?".to_string(),
    };

    FileInfo {
        name,
        owner,
        group,
        size,
        permissions,
        symlink_target,
    }
}

/// Sums file sizes under `path`, up to `MAX_DIR_SCAN` entries — returns
/// `(total_bytes, was_capped)`. Never follows symlinked subdirectories, to
/// avoid a loop (same rule as `recursive_search`).
fn dir_size(path: &Path) -> (u64, bool) {
    let mut total = 0u64;
    let mut scanned = 0usize;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            if scanned >= MAX_DIR_SCAN {
                return (total, true);
            }
            scanned += 1;
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    (total, false)
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn user_name(uid: u32) -> String {
    std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|c| parse_id_name(&c, uid))
        .unwrap_or_else(|| uid.to_string())
}

fn group_name(gid: u32) -> String {
    std::fs::read_to_string("/etc/group")
        .ok()
        .and_then(|c| parse_id_name(&c, gid))
        .unwrap_or_else(|| gid.to_string())
}

/// Parses `/etc/passwd`/`/etc/group`-formatted content (`name:x:id:...`)
/// looking for `id` in the third colon-separated field — split out from
/// `user_name`/`group_name` so it's testable without touching the real
/// filesystem.
fn parse_id_name(contents: &str, id: u32) -> Option<String> {
    contents.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let _passwd = fields.next()?;
        let entry_id: u32 = fields.next()?.parse().ok()?;
        (entry_id == id).then(|| name.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_id_name_finds_the_matching_id() {
        let passwd =
            "root:x:0:0:root:/root:/bin/bash\nrie_zel:x:1000:1000:Rie:/home/rie_zel:/bin/zsh\n";
        assert_eq!(parse_id_name(passwd, 1000), Some("rie_zel".to_string()));
        assert_eq!(parse_id_name(passwd, 0), Some("root".to_string()));
        assert_eq!(parse_id_name(passwd, 999), None);
    }

    #[test]
    fn format_size_picks_the_right_unit() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.00 MB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.00 GB");
    }

    #[test]
    fn dir_size_sums_nested_files() {
        let dir = std::env::temp_dir().join(format!(
            "browdeck_test_fileinfo_dir_size_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested = dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.join("a.txt"), b"12345").unwrap();
        std::fs::write(nested.join("b.txt"), b"1234567890").unwrap();

        let (bytes, capped) = dir_size(&dir);
        assert_eq!(bytes, 15);
        assert!(!capped);
    }
}
