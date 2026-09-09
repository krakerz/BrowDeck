use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{ICON_SD_CARD, ICON_STORAGE, ICON_USB};
use std::path::PathBuf;

pub struct Mount {
    pub device: String,
    pub mount_point: PathBuf,
    pub removable: bool,
}

impl Mount {
    pub fn icon(&self) -> MaterialIcon {
        if !self.removable {
            ICON_STORAGE
        } else if self.device.contains("mmcblk") {
            ICON_SD_CARD
        } else {
            ICON_USB
        }
    }

    /// Whether this is one of the conventional removable-media mount
    /// locations (`/media/*`, `/mnt/*`, `/run/media/*` — the last is
    /// udisks2's auto-mount location, what SteamOS itself uses for a USB
    /// drive/SD card inserted in Game/Desktop Mode) — the default,
    /// uncluttered sidebar view shows only these; internal system mounts
    /// (`/`, `/var/log`, …) are opt-in via "show all".
    pub fn is_common_location(&self) -> bool {
        self.mount_point.starts_with("/media")
            || self.mount_point.starts_with("/mnt")
            || self.mount_point.starts_with("/run/media")
    }
}

/// Real block-device mounts from `/proc/mounts` (pseudo filesystems like
/// proc/sysfs/tmpfs have a non-`/dev/*` "device" field and are skipped),
/// sorted ascending by mount point.
pub fn list_mounts() -> Vec<Mount> {
    let Ok(contents) = std::fs::read_to_string("/proc/mounts") else {
        return Vec::new();
    };
    let mut mounts: Vec<Mount> = contents.lines().filter_map(parse_mount_line).collect();
    mounts.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    mounts
}

fn parse_mount_line(line: &str) -> Option<Mount> {
    let mut fields = line.split_whitespace();
    let device = fields.next()?.to_string();
    let mount_point = fields.next()?;
    if !device.starts_with("/dev/") {
        return None;
    }
    let mount_point = PathBuf::from(unescape_octal(mount_point));
    let removable = is_removable(&device);
    Some(Mount {
        device,
        mount_point,
        removable,
    })
}

/// `/proc/mounts` escapes space/tab/backslash/newline in paths as `\0NN` octal.
fn unescape_octal(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && let Ok(val) = u8::from_str_radix(&s[i + 1..i + 4], 8)
        {
            out.push(val);
            i += 4;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_removable(device: &str) -> bool {
    let Some(name) = device.strip_prefix("/dev/") else {
        return false;
    };
    let parent = parent_block_device(name);
    std::fs::read_to_string(format!("/sys/block/{parent}/removable"))
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}

/// Maps a partition device name to its parent disk, e.g. `sda1` -> `sda`,
/// `nvme0n1p1` -> `nvme0n1`, `mmcblk0p1` -> `mmcblk0` (microSD readers).
fn parent_block_device(name: &str) -> String {
    if let Some(pos) = name.rfind('p')
        && name[..pos].ends_with(|c: char| c.is_ascii_digit())
        && !name[pos + 1..].is_empty()
        && name[pos + 1..].chars().all(|c| c.is_ascii_digit())
    {
        return name[..pos].to_string();
    }
    let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if trimmed.is_empty() {
        name.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(mount_point: &str) -> Mount {
        Mount {
            device: "/dev/sda1".to_string(),
            mount_point: PathBuf::from(mount_point),
            removable: true,
        }
    }

    #[test]
    fn common_locations_include_run_media_alongside_media_and_mnt() {
        assert!(mount("/media/deck/MY_USB").is_common_location());
        assert!(mount("/mnt/data").is_common_location());
        assert!(mount("/run/media/deck/MY_USB").is_common_location());
    }

    #[test]
    fn internal_system_mounts_are_not_common_locations() {
        assert!(!mount("/").is_common_location());
        assert!(!mount("/var/log").is_common_location());
        assert!(!mount("/boot").is_common_location());
    }
}
