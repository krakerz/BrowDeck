use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Text,
}

const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "tiff", "tif",
];

const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "rs", "toml", "json", "yaml", "yml", "ini", "cfg", "conf", "log",
    "csv", "sh", "bash", "zsh", "py", "js", "ts", "html", "htm", "css", "xml",
];

pub fn classify(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Some(Kind::Image)
    } else if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        Some(Kind::Text)
    } else {
        None
    }
}

/// Cap on how much of a text file is read for preview, so a huge log file
/// doesn't stall the UI thread.
const TEXT_PREVIEW_CAP: usize = 64 * 1024;

pub fn read_text_preview(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let truncated = bytes.len() > TEXT_PREVIEW_CAP;
    let slice = &bytes[..bytes.len().min(TEXT_PREVIEW_CAP)];
    let mut text = String::from_utf8_lossy(slice).into_owned();
    if truncated {
        text.push_str("\n\n… (truncated)");
    }
    Ok(text)
}

/// `file://` URI egui_extras' `FileLoader` accepts — requires `path` to be
/// absolute (the loader strips exactly the `file://` prefix, so a relative
/// path here would resolve relative to nothing meaningful).
pub fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}
