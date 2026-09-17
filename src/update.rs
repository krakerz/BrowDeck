use crate::fileops::{Job, ProgressMsg};
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};

const RELEASES_API_URL: &str = "https://api.github.com/repos/krakerz/BrowDeck/releases/latest";
/// GitHub's API rejects requests with no `User-Agent` at all.
const USER_AGENT: &str = "BrowDeck-update-check";

pub struct ReleaseInfo {
    pub version: String,
    pub download_url: String,
}

pub enum CheckResult {
    UpToDate,
    Available(ReleaseInfo),
    Error(String),
}

pub struct CheckJob {
    receiver: Receiver<CheckResult>,
}

impl CheckJob {
    pub fn poll(&self) -> Option<CheckResult> {
        self.receiver.try_recv().ok()
    }
}

/// Checks the latest **published** GitHub release against `current_version`
/// (draft releases — this project's own convention, see `build.yml` — never
/// show up here, only what's actually been published).
pub fn spawn_check(current_version: &str) -> CheckJob {
    let (tx, rx) = mpsc::channel();
    let current_version = current_version.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(run_check(&current_version));
    });
    CheckJob { receiver: rx }
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn run_check(current_version: &str) -> CheckResult {
    let body = match ureq::get(RELEASES_API_URL)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(mut response) => match response.body_mut().read_to_string() {
            Ok(body) => body,
            Err(e) => return CheckResult::Error(e.to_string()),
        },
        Err(e) => return CheckResult::Error(e.to_string()),
    };
    parse_release(&body, current_version)
}

fn parse_release(body: &str, current_version: &str) -> CheckResult {
    let release: GithubRelease = match serde_json::from_str(body) {
        Ok(release) => release,
        Err(e) => return CheckResult::Error(format!("couldn't read release info: {e}")),
    };
    let Some(asset) = release.assets.iter().find(|a| a.name.ends_with(".tar.gz")) else {
        return CheckResult::Error("latest release has no .tar.gz asset".to_string());
    };
    let remote_version = release.tag_name.trim_start_matches('v');
    if is_newer(remote_version, current_version) {
        CheckResult::Available(ReleaseInfo {
            version: remote_version.to_string(),
            download_url: asset.browser_download_url.clone(),
        })
    } else {
        CheckResult::UpToDate
    }
}

/// This project's own versions are always plain `MAJOR.MINOR.PATCH` (see
/// `CLAUDE.md`), so a full `semver` crate isn't needed just to compare two
/// of them — anything that doesn't parse that way is treated as "not
/// newer" rather than erroring, since this only ever gates a UI prompt.
fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

fn is_newer(remote: &str, current: &str) -> bool {
    match (parse_version(remote), parse_version(current)) {
        (Some(r), Some(c)) => r > c,
        _ => false,
    }
}

/// Downloads `release`'s archive, extracts just the `browdeck` binary from
/// it, and atomically replaces the currently-running executable — reuses
/// `fileops::Job`/`ProgressMsg` (the same background-thread-plus-channel
/// shape as `fileops::spawn_extract`/`spawn_compress`) rather than a
/// parallel progress type.
pub fn spawn_install(release: ReleaseInfo) -> Job {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || run_install(&release, &tx));
    Job {
        label: "Updating".to_string(),
        receiver: rx,
        done: 0,
        total: 1,
        finished: false,
        error: None,
        finished_at: None,
    }
}

fn run_install(release: &ReleaseInfo, tx: &Sender<ProgressMsg>) {
    match try_install(release, tx) {
        Ok(()) => {
            let _ = tx.send(ProgressMsg::Done);
        }
        Err(e) => {
            let _ = tx.send(ProgressMsg::Error(e));
        }
    }
}

fn try_install(release: &ReleaseInfo, tx: &Sender<ProgressMsg>) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Both temp files live *next to* the installed binary, not in
    // `std::env::temp_dir()` — the final `rename` over `current_exe` only
    // works atomically (and often only works at all) within the same
    // filesystem.
    let install_dir = current_exe
        .parent()
        .ok_or("couldn't determine the installed binary's directory")?;
    let pid = std::process::id();
    let tmp_archive = install_dir.join(format!(".browdeck-update-{pid}.tar.gz"));
    let tmp_bin = install_dir.join(format!(".browdeck-update-{pid}"));

    let result = download(&release.download_url, &tmp_archive, tx)
        .and_then(|()| extract_binary(&tmp_archive, &tmp_bin))
        .and_then(|()| make_executable(&tmp_bin))
        .and_then(|()| std::fs::rename(&tmp_bin, &current_exe).map_err(|e| e.to_string()));

    let _ = std::fs::remove_file(&tmp_archive);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_bin);
    }
    result
}

fn download(url: &str, dest: &Path, tx: &Sender<ProgressMsg>) -> Result<(), String> {
    let response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| e.to_string())?;
    // `Content-Length` isn't guaranteed (redirects, chunked encoding) — fall
    // back to an indeterminate `1` rather than failing, same fallback shape
    // already used by every `fileops.rs` job for this exact reason.
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1)
        .max(1);
    let _ = tx.send(ProgressMsg::Progress { done: 0, total });

    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut reader = response.into_body().into_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        let _ = tx.send(ProgressMsg::Progress {
            done,
            total: total.max(done),
        });
    }
    Ok(())
}

/// Pulls just the `browdeck` binary out of the downloaded release archive
/// (`BrowDeck-<version>-x86_64/browdeck`, per `build.yml`'s own archive
/// layout) — same reasoning as `fileops.rs`'s extraction, just targeting
/// one specific entry instead of the whole tree.
fn extract_binary(archive_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let is_binary = entry
            .path()
            .map(|p| p.file_name().is_some_and(|n| n == "browdeck"))
            .unwrap_or(false);
        if is_binary {
            let mut out = std::fs::File::create(dest).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err("release archive has no browdeck binary".to_string())
}

fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_compares_major_minor_patch_numerically() {
        assert!(is_newer("1.8.0", "1.7.0"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(is_newer("1.7.10", "1.7.9"));
        assert!(!is_newer("1.7.0", "1.7.0"));
        assert!(!is_newer("1.6.9", "1.7.0"));
    }

    #[test]
    fn is_newer_strips_a_leading_v() {
        assert!(is_newer("v1.8.0", "1.7.0"));
        assert!(is_newer("1.8.0", "v1.7.0"));
    }

    #[test]
    fn is_newer_is_false_for_unparsable_input_rather_than_panicking() {
        assert!(!is_newer("not-a-version", "1.7.0"));
        assert!(!is_newer("1.7.0", "also-not-a-version"));
        assert!(!is_newer("1.7", "1.6.0"));
    }

    #[test]
    fn parse_release_finds_the_tar_gz_asset_and_flags_a_newer_version() {
        let body = r#"{
            "tag_name": "v1.8.0",
            "assets": [
                {"name": "BrowDeck-1.8.0-x86_64.tar.gz",
                 "browser_download_url": "https://example.com/BrowDeck-1.8.0-x86_64.tar.gz"}
            ]
        }"#;
        match parse_release(body, "1.7.0") {
            CheckResult::Available(release) => {
                assert_eq!(release.version, "1.8.0");
                assert_eq!(
                    release.download_url,
                    "https://example.com/BrowDeck-1.8.0-x86_64.tar.gz"
                );
            }
            _ => panic!("expected an available update"),
        }
    }

    #[test]
    fn parse_release_reports_up_to_date_when_not_newer() {
        let body = r#"{
            "tag_name": "v1.7.0",
            "assets": [
                {"name": "BrowDeck-1.7.0-x86_64.tar.gz",
                 "browser_download_url": "https://example.com/BrowDeck-1.7.0-x86_64.tar.gz"}
            ]
        }"#;
        assert!(matches!(
            parse_release(body, "1.7.0"),
            CheckResult::UpToDate
        ));
    }

    #[test]
    fn parse_release_errors_when_no_tar_gz_asset_is_present() {
        let body = r#"{
            "tag_name": "v1.8.0",
            "assets": [
                {"name": "SHA256SUMS",
                 "browser_download_url": "https://example.com/SHA256SUMS"}
            ]
        }"#;
        assert!(matches!(
            parse_release(body, "1.7.0"),
            CheckResult::Error(_)
        ));
    }

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "browdeck_test_update_{name}_{}_{}",
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
    fn extract_binary_pulls_out_only_the_browdeck_entry() {
        let root = scratch_dir("extract");
        let archive_path = root.join("release.tar.gz");
        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);

            let mut header = tar::Header::new_gnu();
            header.set_size(b"fake binary contents".len() as u64);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "BrowDeck-1.8.0-x86_64/browdeck",
                    &b"fake binary contents"[..],
                )
                .unwrap();

            let mut header = tar::Header::new_gnu();
            header.set_size(b"a desktop entry".len() as u64);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "BrowDeck-1.8.0-x86_64/browdeck.desktop",
                    &b"a desktop entry"[..],
                )
                .unwrap();

            builder.into_inner().unwrap().finish().unwrap();
        }

        let dest = root.join("browdeck-out");
        extract_binary(&archive_path, &dest).unwrap();
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "fake binary contents"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }
}
