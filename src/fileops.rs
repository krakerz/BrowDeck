use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

pub enum ProgressMsg {
    Progress { done: u64, total: u64 },
    Done,
    Error(String),
}

pub struct Job {
    pub label: String,
    pub receiver: Receiver<ProgressMsg>,
    pub done: u64,
    pub total: u64,
    pub finished: bool,
    pub error: Option<String>,
    pub finished_at: Option<Instant>,
}

impl Job {
    /// Drains all pending progress messages, keeping only the latest state.
    pub fn poll(&mut self) {
        while let Ok(msg) = self.receiver.try_recv() {
            match msg {
                ProgressMsg::Progress { done, total } => {
                    self.done = done;
                    self.total = total;
                }
                ProgressMsg::Done => {
                    self.finished = true;
                    self.finished_at = Some(Instant::now());
                }
                ProgressMsg::Error(e) => {
                    self.error = Some(e);
                    self.finished = true;
                    self.finished_at = Some(Instant::now());
                }
            }
        }
    }
}

pub fn spawn_copy(sources: Vec<PathBuf>, dest_dir: PathBuf) -> Job {
    spawn_job("Copying", sources, dest_dir, false)
}

pub fn spawn_move(sources: Vec<PathBuf>, dest_dir: PathBuf) -> Job {
    spawn_job("Moving", sources, dest_dir, true)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    Tar,
    TarGz,
}

fn archive_kind(path: &Path) -> Option<ArchiveKind> {
    let name = path.file_name()?.to_str()?.to_lowercase();
    if name.ends_with(".zip") {
        Some(ArchiveKind::Zip)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some(ArchiveKind::TarGz)
    } else if name.ends_with(".tar") {
        Some(ArchiveKind::Tar)
    } else {
        None
    }
}

pub fn is_archive(path: &Path) -> bool {
    archive_kind(path).is_some()
}

pub fn spawn_extract(archive: PathBuf, dest_dir: PathBuf) -> Job {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || run_extract(&archive, &dest_dir, &tx));
    Job {
        label: "Extracting".to_string(),
        receiver: rx,
        done: 0,
        total: 1,
        finished: false,
        error: None,
        finished_at: None,
    }
}

fn run_extract(archive_path: &Path, dest_dir: &Path, tx: &Sender<ProgressMsg>) {
    let result = match archive_kind(archive_path) {
        Some(ArchiveKind::Zip) => extract_zip(archive_path, dest_dir, tx),
        Some(ArchiveKind::Tar) => extract_tar(archive_path, dest_dir, tx),
        Some(ArchiveKind::TarGz) => extract_tar_gz(archive_path, dest_dir, tx),
        None => Err("unsupported archive format".to_string()),
    };
    match result {
        Ok(()) => {
            let _ = tx.send(ProgressMsg::Done);
        }
        Err(e) => {
            let _ = tx.send(ProgressMsg::Error(e));
        }
    }
}

fn extract_zip(
    archive_path: &Path,
    dest_dir: &Path,
    tx: &Sender<ProgressMsg>,
) -> Result<(), String> {
    let file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let total = archive.len().max(1) as u64;
    let _ = tx.send(ProgressMsg::Progress { done: 0, total });
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        // `enclosed_name` rejects absolute paths and `..` components —
        // skip anything a malicious archive could use to escape dest_dir.
        if let Some(rel_path) = entry.enclosed_name() {
            let out_path = dest_dir.join(rel_path);
            if entry.is_dir() {
                std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut out_file = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
                std::io::copy(&mut entry, &mut out_file).map_err(|e| e.to_string())?;
            }
        }
        let _ = tx.send(ProgressMsg::Progress {
            done: (i + 1) as u64,
            total,
        });
    }
    Ok(())
}

/// tar has no central directory (it's a flat entry stream), so there's no
/// cheap way to know the entry count up front — this does a first pass
/// (decompressing but not writing) purely to count entries for progress,
/// then a second pass that actually extracts.
fn extract_tar(
    archive_path: &Path,
    dest_dir: &Path,
    tx: &Sender<ProgressMsg>,
) -> Result<(), String> {
    let count_file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let total = count_tar_entries(count_file)?.max(1);
    let _ = tx.send(ProgressMsg::Progress { done: 0, total });
    let file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
    extract_tar_entries(file, dest_dir, total, tx)
}

fn extract_tar_gz(
    archive_path: &Path,
    dest_dir: &Path,
    tx: &Sender<ProgressMsg>,
) -> Result<(), String> {
    let count_file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let total = count_tar_entries(flate2::read::GzDecoder::new(count_file))?.max(1);
    let _ = tx.send(ProgressMsg::Progress { done: 0, total });
    let file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
    extract_tar_entries(flate2::read::GzDecoder::new(file), dest_dir, total, tx)
}

fn count_tar_entries<R: Read>(reader: R) -> Result<u64, String> {
    let mut archive = tar::Archive::new(reader);
    let mut count = 0u64;
    for entry in archive.entries().map_err(|e| e.to_string())? {
        entry.map_err(|e| e.to_string())?;
        count += 1;
    }
    Ok(count)
}

fn extract_tar_entries<R: Read>(
    reader: R,
    dest_dir: &Path,
    total: u64,
    tx: &Sender<ProgressMsg>,
) -> Result<(), String> {
    let mut archive = tar::Archive::new(reader);
    for (i, entry) in archive.entries().map_err(|e| e.to_string())?.enumerate() {
        let mut entry = entry.map_err(|e| e.to_string())?;
        // `unpack_in` (not `unpack`) is the sanitizing variant — refuses to
        // write outside `dest_dir` (path traversal / absolute paths).
        entry.unpack_in(dest_dir).map_err(|e| e.to_string())?;
        let _ = tx.send(ProgressMsg::Progress {
            done: (i + 1) as u64,
            total,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CompressKind {
    Zip,
    TarGz,
}

pub fn spawn_compress(sources: Vec<PathBuf>, dest: PathBuf, kind: CompressKind) -> Job {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || run_compress(&sources, &dest, kind, &tx));
    Job {
        label: "Compressing".to_string(),
        receiver: rx,
        done: 0,
        total: 1,
        finished: false,
        error: None,
        finished_at: None,
    }
}

fn run_compress(sources: &[PathBuf], dest: &Path, kind: CompressKind, tx: &Sender<ProgressMsg>) {
    let result = match kind {
        CompressKind::Zip => compress_zip(sources, dest, tx),
        CompressKind::TarGz => compress_tar_gz(sources, dest, tx),
    };
    match result {
        Ok(()) => {
            let _ = tx.send(ProgressMsg::Done);
        }
        Err(e) => {
            let _ = tx.send(ProgressMsg::Error(e));
        }
    }
}

/// One increment per entry (file or directory) that `add_to_zip`/
/// `add_to_tar` will write — mirrors the tar-extract "count first, then do
/// the work" pattern already used above, since neither writer exposes a
/// total up front.
fn walk_count(path: &Path, count: &mut u64) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return;
    };
    *count += 1;
    if meta.is_dir()
        && let Ok(entries) = std::fs::read_dir(path)
    {
        for entry in entries.flatten() {
            walk_count(&entry.path(), count);
        }
    }
}

fn compress_zip(sources: &[PathBuf], dest: &Path, tx: &Sender<ProgressMsg>) -> Result<(), String> {
    let mut total = 0u64;
    for src in sources {
        walk_count(src, &mut total);
    }
    let total = total.max(1);
    let _ = tx.send(ProgressMsg::Progress { done: 0, total });

    let file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    let mut done = 0u64;
    for src in sources {
        let Some(name) = src.file_name() else {
            continue;
        };
        add_to_zip(
            &mut zip,
            src,
            Path::new(name),
            options,
            &mut done,
            total,
            tx,
        )?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn add_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    src: &Path,
    rel_name: &Path,
    options: zip::write::SimpleFileOptions,
    done: &mut u64,
    total: u64,
    tx: &Sender<ProgressMsg>,
) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        zip.add_directory(format!("{}/", rel_name.to_string_lossy()), options)
            .map_err(|e| e.to_string())?;
        *done += 1;
        let _ = tx.send(ProgressMsg::Progress { done: *done, total });
        for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let child_rel = rel_name.join(entry.file_name());
            add_to_zip(zip, &entry.path(), &child_rel, options, done, total, tx)?;
        }
    } else {
        zip.start_file(rel_name.to_string_lossy(), options)
            .map_err(|e| e.to_string())?;
        let mut f = std::fs::File::open(src).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, zip).map_err(|e| e.to_string())?;
        *done += 1;
        let _ = tx.send(ProgressMsg::Progress { done: *done, total });
    }
    Ok(())
}

fn compress_tar_gz(
    sources: &[PathBuf],
    dest: &Path,
    tx: &Sender<ProgressMsg>,
) -> Result<(), String> {
    let mut total = 0u64;
    for src in sources {
        walk_count(src, &mut total);
    }
    let total = total.max(1);
    let _ = tx.send(ProgressMsg::Progress { done: 0, total });

    let file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut done = 0u64;
    for src in sources {
        let Some(name) = src.file_name() else {
            continue;
        };
        add_to_tar(&mut builder, src, Path::new(name), &mut done, total, tx)?;
    }
    let encoder = builder.into_inner().map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn add_to_tar<W: Write>(
    builder: &mut tar::Builder<W>,
    src: &Path,
    rel_name: &Path,
    done: &mut u64,
    total: u64,
    tx: &Sender<ProgressMsg>,
) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        builder
            .append_dir(rel_name, src)
            .map_err(|e| e.to_string())?;
        *done += 1;
        let _ = tx.send(ProgressMsg::Progress { done: *done, total });
        for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let child_rel = rel_name.join(entry.file_name());
            add_to_tar(builder, &entry.path(), &child_rel, done, total, tx)?;
        }
    } else {
        let mut f = std::fs::File::open(src).map_err(|e| e.to_string())?;
        builder
            .append_file(rel_name, &mut f)
            .map_err(|e| e.to_string())?;
        *done += 1;
        let _ = tx.send(ProgressMsg::Progress { done: *done, total });
    }
    Ok(())
}

fn spawn_job(label: &str, sources: Vec<PathBuf>, dest_dir: PathBuf, is_move: bool) -> Job {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || run_job(&sources, &dest_dir, is_move, &tx));
    Job {
        label: label.to_string(),
        receiver: rx,
        done: 0,
        total: 1,
        finished: false,
        error: None,
        finished_at: None,
    }
}

fn run_job(sources: &[PathBuf], dest_dir: &Path, is_move: bool, tx: &Sender<ProgressMsg>) {
    let total = sources.iter().map(|s| size_of(s)).sum::<u64>().max(1);
    let mut done = 0u64;
    for src in sources {
        let Some(file_name) = src.file_name() else {
            continue;
        };
        let dest = dest_dir.join(file_name);
        let result = if is_move {
            move_one(src, &dest, size_of(src), &mut done, total, tx)
        } else {
            copy_one(src, &dest, &mut done, total, tx)
        };
        if let Err(e) = result {
            let _ = tx.send(ProgressMsg::Error(e));
            return;
        }
    }
    let _ = tx.send(ProgressMsg::Done);
}

fn size_of(path: &Path) -> u64 {
    let mut total = 0;
    walk_size(path, &mut total);
    total
}

fn walk_size(path: &Path, total: &mut u64) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return;
    };
    if meta.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                walk_size(&entry.path(), total);
            }
        }
    } else {
        *total += meta.len();
    }
}

fn copy_one(
    src: &Path,
    dest: &Path,
    done: &mut u64,
    total: u64,
    tx: &Sender<ProgressMsg>,
) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
        for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_one(
                &entry.path(),
                &dest.join(entry.file_name()),
                done,
                total,
                tx,
            )?;
        }
        Ok(())
    } else {
        copy_file(src, dest, done, total, tx)
    }
}

fn copy_file(
    src: &Path,
    dest: &Path,
    done: &mut u64,
    total: u64,
    tx: &Sender<ProgressMsg>,
) -> Result<(), String> {
    let mut reader = std::fs::File::open(src).map_err(|e| e.to_string())?;
    let mut writer = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 256 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        *done += n as u64;
        let _ = tx.send(ProgressMsg::Progress { done: *done, total });
    }
    Ok(())
}

/// Renames when possible (instant, same filesystem); falls back to a
/// streamed copy + remove across filesystems (e.g. onto a USB/microSD mount).
fn move_one(
    src: &Path,
    dest: &Path,
    item_size: u64,
    done: &mut u64,
    total: u64,
    tx: &Sender<ProgressMsg>,
) -> Result<(), String> {
    if std::fs::rename(src, dest).is_ok() {
        *done += item_size;
        let _ = tx.send(ProgressMsg::Progress { done: *done, total });
        return Ok(());
    }
    copy_one(src, dest, done, total, tx)?;
    remove_path(src)
}

fn remove_path(path: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "browdeck_test_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Drains a job's channel to completion, forwarding progress into a
    /// [`Job`]-like tally so the test can assert on the final state.
    fn wait_done(rx: &Receiver<ProgressMsg>) -> Result<(u64, u64), String> {
        let mut done = 0;
        let mut total = 1;
        loop {
            match rx.recv().map_err(|e| e.to_string())? {
                ProgressMsg::Progress { done: d, total: t } => {
                    done = d;
                    total = t;
                }
                ProgressMsg::Done => return Ok((done, total)),
                ProgressMsg::Error(e) => return Err(e),
            }
        }
    }

    #[test]
    fn copy_preserves_source_and_recreates_tree() {
        let root = scratch_dir("copy");
        let src = root.join("src_dir");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        std::fs::write(src.join("nested/b.txt"), b"world").unwrap();
        let dest_dir = root.join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let job = spawn_copy(vec![src.clone()], dest_dir.clone());
        let (done, total) = wait_done(&job.receiver).unwrap();
        assert_eq!(done, total);

        assert_eq!(
            std::fs::read_to_string(dest_dir.join("src_dir/a.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("src_dir/nested/b.txt")).unwrap(),
            "world"
        );
        // source untouched by a copy
        assert!(src.join("a.txt").exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn move_removes_source() {
        let root = scratch_dir("move");
        let src = root.join("src_dir");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        let dest_dir = root.join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let job = spawn_move(vec![src.clone()], dest_dir.clone());
        wait_done(&job.receiver).unwrap();

        assert!(!src.exists());
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("src_dir/a.txt")).unwrap(),
            "hello"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn is_archive_matches_known_extensions_case_insensitively() {
        assert!(is_archive(Path::new("thing.zip")));
        assert!(is_archive(Path::new("thing.ZIP")));
        assert!(is_archive(Path::new("thing.tar")));
        assert!(is_archive(Path::new("thing.tar.gz")));
        assert!(is_archive(Path::new("thing.TGZ")));
        assert!(!is_archive(Path::new("thing.rar")));
        assert!(!is_archive(Path::new("thing")));
    }

    #[test]
    fn extract_writes_entries_and_rejects_path_traversal() {
        let root = scratch_dir("extract");
        let archive_path = root.join("test.zip");
        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            zip.start_file("nested/inside.txt", options).unwrap();
            zip.write_all(b"hello from zip").unwrap();
            // A malicious entry trying to escape the destination directory —
            // `enclosed_name()` should reject it and `run_extract` should
            // just skip it rather than writing outside `dest_dir`.
            zip.start_file("../escape.txt", options).unwrap();
            zip.write_all(b"should not land here").unwrap();
            zip.finish().unwrap();
        }
        let dest_dir = root.join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let job = spawn_extract(archive_path, dest_dir.clone());
        let (done, total) = wait_done(&job.receiver).unwrap();
        assert_eq!(done, total);

        assert_eq!(
            std::fs::read_to_string(dest_dir.join("nested/inside.txt")).unwrap(),
            "hello from zip"
        );
        assert!(!root.join("escape.txt").exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn extract_tar_gz_writes_entries() {
        let root = scratch_dir("targz");
        let src = root.join("src_dir");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("nested/inside.txt"), b"hello from tar.gz").unwrap();

        let archive_path = root.join("test.tar.gz");
        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            builder.append_dir_all("", &src).unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        let dest_dir = root.join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let job = spawn_extract(archive_path, dest_dir.clone());
        let (done, total) = wait_done(&job.receiver).unwrap();
        assert_eq!(done, total);

        assert_eq!(
            std::fs::read_to_string(dest_dir.join("nested/inside.txt")).unwrap(),
            "hello from tar.gz"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn compress_zip_round_trips_through_extract() {
        let root = scratch_dir("compress_zip");
        let src = root.join("src_dir");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        std::fs::write(src.join("nested/b.txt"), b"world").unwrap();

        let archive_path = root.join("out.zip");
        let job = spawn_compress(vec![src.clone()], archive_path.clone(), CompressKind::Zip);
        let (done, total) = wait_done(&job.receiver).unwrap();
        assert_eq!(done, total);

        let dest_dir = root.join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();
        let job = spawn_extract(archive_path, dest_dir.clone());
        wait_done(&job.receiver).unwrap();

        assert_eq!(
            std::fs::read_to_string(dest_dir.join("src_dir/a.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("src_dir/nested/b.txt")).unwrap(),
            "world"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn compress_tar_gz_round_trips_through_extract() {
        let root = scratch_dir("compress_targz");
        let src = root.join("src_dir");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        std::fs::write(src.join("nested/b.txt"), b"world").unwrap();

        let archive_path = root.join("out.tar.gz");
        let job = spawn_compress(vec![src.clone()], archive_path.clone(), CompressKind::TarGz);
        let (done, total) = wait_done(&job.receiver).unwrap();
        assert_eq!(done, total);

        let dest_dir = root.join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();
        let job = spawn_extract(archive_path, dest_dir.clone());
        wait_done(&job.receiver).unwrap();

        assert_eq!(
            std::fs::read_to_string(dest_dir.join("src_dir/a.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("src_dir/nested/b.txt")).unwrap(),
            "world"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn compress_zip_with_multiple_sources_keeps_each_as_its_own_top_level_entry() {
        let root = scratch_dir("compress_zip_multi");
        let file_a = root.join("a.txt");
        let file_b = root.join("b.txt");
        std::fs::write(&file_a, b"hello").unwrap();
        std::fs::write(&file_b, b"world").unwrap();

        let archive_path = root.join("out.zip");
        let job = spawn_compress(
            vec![file_a, file_b],
            archive_path.clone(),
            CompressKind::Zip,
        );
        wait_done(&job.receiver).unwrap();

        let dest_dir = root.join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();
        let job = spawn_extract(archive_path, dest_dir.clone());
        wait_done(&job.receiver).unwrap();

        assert_eq!(
            std::fs::read_to_string(dest_dir.join("a.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("b.txt")).unwrap(),
            "world"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }
}
