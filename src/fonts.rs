use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use std::process::Command;

/// egui's bundled default font has no CJK glyphs, so filenames containing
/// Japanese/Chinese/Korean characters render as tofu boxes. Ask fontconfig
/// (already present on any Linux desktop, no extra dependency) which
/// installed font it would pick for Japanese text, and register that as a
/// low-priority fallback — tried only for glyphs the default font lacks.
pub fn install_cjk_fallback(ctx: &egui::Context) {
    let Some((bytes, index)) = find_cjk_font() else {
        eprintln!("no CJK-capable font found via fontconfig; CJK filenames may not render");
        return;
    };
    let mut data = egui::FontData::from_owned(bytes);
    data.index = index;
    ctx.add_font(FontInsert::new(
        "cjk-fallback",
        data,
        vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: FontPriority::Lowest,
            },
            // Also needed for `ui.monospace(...)` (e.g. the text preview
            // pane) — Monospace is a separate family with its own fallback
            // chain, registering on Proportional alone doesn't cover it.
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: FontPriority::Lowest,
            },
        ],
    ));
}

fn find_cjk_font() -> Option<(Vec<u8>, u32)> {
    let output = Command::new("fc-match")
        .args(["-f", "%{file}|%{index}", ":lang=ja"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let (path, index) = text.split_once('|')?;
    let index = index.trim().parse().ok()?;
    let bytes = std::fs::read(path.trim()).ok()?;
    Some((bytes, index))
}
