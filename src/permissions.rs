use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub fn read_mode(path: &Path) -> Option<u32> {
    std::fs::metadata(path)
        .ok()
        .map(|m| m.permissions().mode() & 0o777)
}

pub fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    let mut perms = std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .permissions();
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms).map_err(|e| e.to_string())
}
