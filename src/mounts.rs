use crate::fileops::{Job, ProgressMsg};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{ICON_SD_CARD, ICON_STORAGE, ICON_USB};
use std::path::{Path, PathBuf};

#[derive(Clone)]
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

    /// Media udisks2 lets the active session unmount without root — its
    /// auto-mounts under `/run/media`, plus anything the kernel flags as
    /// removable (many USB enclosures don't, hence the path check).
    pub fn is_ejectable(&self) -> bool {
        self.removable || self.mount_point.starts_with("/run/media")
    }
}

/// Unmounts via `udisksctl`, then tries to power the drive off. Power-off
/// is best-effort: built-in SD readers and drives with another partition
/// still mounted refuse it, and the unmount alone already makes removal
/// safe.
pub fn spawn_eject(device: String) -> Job {
    let (mut job, tx) = Job::new("Ejecting", None);
    job.cancellable = false;
    std::thread::spawn(move || {
        tx.send(ProgressMsg::Progress { done: 0, total: 1 });
        match udisksctl(&["unmount", "-b", &device]) {
            Ok(()) => {
                let _ = udisksctl(&["power-off", "-b", &device]);
                tx.send(ProgressMsg::Progress { done: 1, total: 1 });
                tx.send(ProgressMsg::Done);
            }
            Err(e) => tx.send(ProgressMsg::Error(e)),
        }
    });
    job
}

fn udisksctl(args: &[&str]) -> Result<(), String> {
    // Fails fast instead of waiting on a polkit password prompt that Game
    // Mode has no agent to show.
    let output = std::process::Command::new("udisksctl")
        .args(args)
        .arg("--no-user-interaction")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("couldn't run udisksctl: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(short_udisks_error(&String::from_utf8_lossy(&output.stderr)))
    }
}

/// udisksctl errors wrap the useful part in D-Bus noise, e.g. "Error
/// unmounting /dev/sdb1: GDBus.Error:org.freedesktop.UDisks2.Error.
/// DeviceBusy: Error unmounting /dev/sdb1: target is busy" — keep only the
/// last clause.
fn short_udisks_error(stderr: &str) -> String {
    let trimmed = stderr.trim();
    let last = trimmed.rsplit(": ").next().unwrap_or(trimmed).trim();
    if last.is_empty() {
        "eject failed".to_string()
    } else {
        format!("eject failed: {last}")
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
    // Resolves `/dev/disk/by-*` and `/dev/mapper/*` links to the kernel name.
    let device = std::fs::canonicalize(device).unwrap_or_else(|_| PathBuf::from(device));
    let Some(name) = device.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    disk_is_removable(Path::new("/sys/class/block"), name)
}

/// `removable` lives on the whole disk only. A partition's sysfs entry has a
/// `partition` file and sits inside its disk's directory, so `..` reaches
/// the disk whatever its naming scheme (`sda1`, `nvme0n1p1`, `mmcblk0p1`,
/// or a whole disk mounted with no partition table).
fn disk_is_removable(sys_block: &Path, name: &str) -> bool {
    let entry = sys_block.join(name);
    let disk = if entry.join("partition").exists() {
        entry.join("..")
    } else {
        entry
    };
    std::fs::read_to_string(disk.join("removable")).is_ok_and(|s| s.trim() == "1")
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
    fn ejectable_covers_udisks_mounts_and_removable_devices_only() {
        let mut usb_ssd = mount("/run/media/deck/SSD");
        usb_ssd.removable = false;
        assert!(usb_ssd.is_ejectable());
        assert!(mount("/mnt/sdcard").is_ejectable());
        let mut root = mount("/");
        root.removable = false;
        assert!(!root.is_ejectable());
    }

    #[test]
    fn udisks_errors_are_trimmed_to_their_last_clause() {
        let raw = "Error unmounting /dev/sdb1: GDBus.Error:org.freedesktop.UDisks2.Error.DeviceBusy: \
                   Error unmounting /dev/sdb1: target is busy\n";
        assert_eq!(short_udisks_error(raw), "eject failed: target is busy");
        assert_eq!(short_udisks_error("  "), "eject failed");
    }

    #[test]
    fn removable_is_read_from_the_whole_disk() {
        let root = std::env::temp_dir().join(format!(
            "browdeck_test_sysblock_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // Mirrors sysfs: real dirs under `devices/`, symlinks in `class/`.
        let devices = root.join("devices");
        let class = root.join("class");
        std::fs::create_dir_all(&class).unwrap();
        for (disk, part, removable) in
            [("mmcblk0", "mmcblk0p1", "1"), ("nvme0n1", "nvme0n1p2", "0")]
        {
            std::fs::create_dir_all(devices.join(disk).join(part)).unwrap();
            std::fs::write(devices.join(disk).join("removable"), removable).unwrap();
            std::fs::write(devices.join(disk).join(part).join("partition"), "1").unwrap();
            std::os::unix::fs::symlink(devices.join(disk), class.join(disk)).unwrap();
            std::os::unix::fs::symlink(devices.join(disk).join(part), class.join(part)).unwrap();
        }
        assert!(disk_is_removable(&class, "mmcblk0p1"));
        assert!(disk_is_removable(&class, "mmcblk0"));
        assert!(!disk_is_removable(&class, "nvme0n1p2"));
        assert!(!disk_is_removable(&class, "nvme0n1"));
        assert!(!disk_is_removable(&class, "missing"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn internal_system_mounts_are_not_common_locations() {
        assert!(!mount("/").is_common_location());
        assert!(!mount("/var/log").is_common_location());
        assert!(!mount("/boot").is_common_location());
    }
}
