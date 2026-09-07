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
}
