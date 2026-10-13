use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Square edge of a generated thumbnail, in pixels — comfortably above the
/// file list's largest zoomed icon size, so it never upscales.
const THUMB_PX: u32 = 96;
const WORKERS: usize = 2;
/// Decoding a huge image just for a list thumbnail isn't worth the stall.
const MAX_SOURCE_BYTES: u64 = 40 * 1024 * 1024;
/// Upper bound on cached textures before the cache is simply dropped.
const MAX_CACHED: usize = 600;

struct Request {
    path: PathBuf,
    generation: u64,
}

struct Decoded {
    path: PathBuf,
    generation: u64,
    image: Option<egui::ColorImage>,
}

/// Small square image thumbnails for the file list, decoded off the UI
/// thread. `None` in `textures` marks a file that failed to decode, so it
/// isn't retried every frame.
pub struct Thumbnails {
    textures: HashMap<PathBuf, Option<egui::TextureHandle>>,
    pending: HashSet<PathBuf>,
    requests: Sender<Request>,
    results: Receiver<Decoded>,
    /// Bumped on `clear`; workers skip requests from older generations, so
    /// leaving a big folder doesn't keep decoding it.
    generation: Arc<AtomicU64>,
}

impl Thumbnails {
    pub fn new(ctx: &egui::Context) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<Request>();
        let (res_tx, res_rx) = mpsc::channel();
        let req_rx = Arc::new(Mutex::new(req_rx));
        let generation = Arc::new(AtomicU64::new(0));
        for _ in 0..WORKERS {
            let req_rx = req_rx.clone();
            let res_tx = res_tx.clone();
            let generation = generation.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                loop {
                    let request = match req_rx.lock() {
                        Ok(rx) => rx.recv(),
                        Err(_) => return,
                    };
                    let Ok(request) = request else {
                        return;
                    };
                    if request.generation != generation.load(Ordering::Relaxed) {
                        continue;
                    }
                    let image = decode_thumbnail(&request.path);
                    let decoded = Decoded {
                        path: request.path,
                        generation: request.generation,
                        image,
                    };
                    if res_tx.send(decoded).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            });
        }
        Thumbnails {
            textures: HashMap::new(),
            pending: HashSet::new(),
            requests: req_tx,
            results: res_rx,
            generation,
        }
    }

    /// Uploads whatever the workers finished since last frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let current = self.generation.load(Ordering::Relaxed);
        while let Ok(decoded) = self.results.try_recv() {
            if decoded.generation != current {
                continue;
            }
            self.pending.remove(&decoded.path);
            let texture = decoded.image.map(|image| {
                ctx.load_texture(
                    format!("thumb:{}", decoded.path.display()),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
            self.textures.insert(decoded.path, texture);
        }
    }

    pub fn get(&self, path: &Path) -> Option<&egui::TextureHandle> {
        self.textures.get(path).and_then(Option::as_ref)
    }

    /// Queues `path` for decoding unless it's already cached or queued.
    pub fn request(&mut self, path: &Path) {
        if self.textures.contains_key(path) || self.pending.contains(path) {
            return;
        }
        if self.textures.len() >= MAX_CACHED {
            self.clear();
        }
        self.pending.insert(path.to_path_buf());
        let _ = self.requests.send(Request {
            path: path.to_path_buf(),
            generation: self.generation.load(Ordering::Relaxed),
        });
    }

    pub fn clear(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.textures.clear();
        self.pending.clear();
    }
}

fn decode_thumbnail(path: &Path) -> Option<egui::ColorImage> {
    if std::fs::metadata(path).ok()?.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(square_thumbnail(&image))
}

/// Center-crops to a square, then shrinks to `THUMB_PX`.
fn square_thumbnail(image: &image::DynamicImage) -> egui::ColorImage {
    let (w, h) = (image.width(), image.height());
    let side = w.min(h).max(1);
    let cropped = image.crop_imm((w - side.min(w)) / 2, (h - side.min(h)) / 2, side, side);
    let thumb = if side > THUMB_PX {
        cropped.thumbnail(THUMB_PX, THUMB_PX)
    } else {
        cropped
    };
    let rgba = thumb.to_rgba8();
    egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_are_square_and_capped() {
        let wide = image::DynamicImage::new_rgba8(400, 100);
        assert_eq!(square_thumbnail(&wide).size, [THUMB_PX as usize; 2]);
        let tall = image::DynamicImage::new_rgba8(50, 300);
        assert_eq!(square_thumbnail(&tall).size, [50, 50]);
    }

    #[test]
    fn decode_round_trips_a_real_png() {
        let dir = std::env::temp_dir().join(format!(
            "browdeck_test_thumb_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.png");
        image::DynamicImage::new_rgba8(200, 120)
            .save(&path)
            .unwrap();
        let thumb = decode_thumbnail(&path).unwrap();
        assert_eq!(thumb.size, [THUMB_PX as usize; 2]);
        assert!(decode_thumbnail(&dir.join("missing.png")).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
