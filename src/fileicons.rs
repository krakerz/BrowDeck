use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_AUDIO_FILE, ICON_CODE, ICON_DATA_OBJECT, ICON_DESCRIPTION, ICON_FOLDER, ICON_FOLDER_ZIP,
    ICON_FONT_DOWNLOAD, ICON_IMAGE, ICON_INSERT_DRIVE_FILE, ICON_PICTURE_AS_PDF, ICON_TERMINAL,
    ICON_VIDEO_FILE,
};
use std::path::Path;

const ARCHIVE: &[&str] = &["zip", "tar", "gz", "tgz", "rar", "7z", "bz2", "xz"];
const IMAGE: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "tiff", "tif", "svg", "avif",
];
const AUDIO: &[&str] = &["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus", "wma"];
const VIDEO: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "flv", "wmv", "m4v"];
const CODE: &[&str] = &[
    "rs", "py", "js", "ts", "jsx", "tsx", "go", "c", "h", "cpp", "hpp", "cc", "java", "rb", "php",
    "swift", "kt", "cs", "lua", "html", "htm", "css", "scss", "sh", "bash", "zsh",
];
const DATA: &[&str] = &[
    "json", "yaml", "yml", "toml", "ini", "cfg", "conf", "xml", "csv",
];
const TEXT: &[&str] = &["txt", "md", "markdown", "log"];
const FONT: &[&str] = &["ttf", "otf", "woff", "woff2"];

/// Picks an icon by extension/mimetype-ish category — folders always get
/// `ICON_FOLDER`; everything else falls back to a generic file icon if it
/// doesn't match a known category.
pub fn icon_for(path: &Path, is_dir: bool) -> MaterialIcon {
    if is_dir {
        return ICON_FOLDER;
    }
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return ICON_INSERT_DRIVE_FILE;
    };
    let ext = ext.to_lowercase();
    let ext = ext.as_str();
    if ARCHIVE.contains(&ext) {
        ICON_FOLDER_ZIP
    } else if IMAGE.contains(&ext) {
        ICON_IMAGE
    } else if AUDIO.contains(&ext) {
        ICON_AUDIO_FILE
    } else if VIDEO.contains(&ext) {
        ICON_VIDEO_FILE
    } else if ext == "pdf" {
        ICON_PICTURE_AS_PDF
    } else if matches!(ext, "sh" | "bash" | "zsh") {
        ICON_TERMINAL
    } else if CODE.contains(&ext) {
        ICON_CODE
    } else if DATA.contains(&ext) {
        ICON_DATA_OBJECT
    } else if FONT.contains(&ext) {
        ICON_FONT_DOWNLOAD
    } else if TEXT.contains(&ext) {
        ICON_DESCRIPTION
    } else {
        ICON_INSERT_DRIVE_FILE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_always_get_the_folder_icon_regardless_of_name() {
        assert_eq!(
            icon_for(Path::new("archive.zip"), true).codepoint,
            ICON_FOLDER.codepoint
        );
    }

    #[test]
    fn known_categories_pick_the_right_icon() {
        assert_eq!(
            icon_for(Path::new("a.zip"), false).codepoint,
            ICON_FOLDER_ZIP.codepoint
        );
        assert_eq!(
            icon_for(Path::new("a.PNG"), false).codepoint,
            ICON_IMAGE.codepoint
        );
        assert_eq!(
            icon_for(Path::new("a.mp3"), false).codepoint,
            ICON_AUDIO_FILE.codepoint
        );
        assert_eq!(
            icon_for(Path::new("a.mp4"), false).codepoint,
            ICON_VIDEO_FILE.codepoint
        );
        assert_eq!(
            icon_for(Path::new("a.pdf"), false).codepoint,
            ICON_PICTURE_AS_PDF.codepoint
        );
        assert_eq!(
            icon_for(Path::new("a.rs"), false).codepoint,
            ICON_CODE.codepoint
        );
        assert_eq!(
            icon_for(Path::new("a.sh"), false).codepoint,
            ICON_TERMINAL.codepoint
        );
        assert_eq!(
            icon_for(Path::new("a.json"), false).codepoint,
            ICON_DATA_OBJECT.codepoint
        );
        assert_eq!(
            icon_for(Path::new("a.ttf"), false).codepoint,
            ICON_FONT_DOWNLOAD.codepoint
        );
        assert_eq!(
            icon_for(Path::new("a.txt"), false).codepoint,
            ICON_DESCRIPTION.codepoint
        );
    }

    #[test]
    fn unknown_or_missing_extension_falls_back_to_a_generic_file_icon() {
        assert_eq!(
            icon_for(Path::new("no_extension"), false).codepoint,
            ICON_INSERT_DRIVE_FILE.codepoint
        );
        assert_eq!(
            icon_for(Path::new("a.xyz123"), false).codepoint,
            ICON_INSERT_DRIVE_FILE.codepoint
        );
    }
}
