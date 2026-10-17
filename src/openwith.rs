use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    /// Desktop file id, e.g. `org.gnome.eog.desktop`.
    pub id: String,
    pub name: String,
    exec: String,
}

/// Installed applications that declare they handle `path`'s MIME type, the
/// user's default (from `mimeapps.list`) first, the rest by name.
pub fn apps_for(path: &Path) -> Vec<App> {
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    let globs = std::fs::read_to_string("/usr/share/mime/globs2").unwrap_or_default();
    let Some(mime) = mime_for(file_name, &globs) else {
        return Vec::new();
    };
    let mimeapps = config_home()
        .map(|c| std::fs::read_to_string(c.join("mimeapps.list")).unwrap_or_default())
        .unwrap_or_default();
    let (defaults, added) = parse_mimeapps(&mimeapps, &mime);

    let mut apps = Vec::new();
    let mut seen = HashSet::new();
    for dir in application_dirs() {
        collect_entries(&dir, &dir, &mut seen, &mut apps);
    }
    let mut matching: Vec<App> = apps
        .into_iter()
        .filter(|(app, mimes)| mimes.contains(&mime) || added.contains(&app.id))
        .map(|(app, _)| app)
        .collect();
    matching.sort_by_key(|app| {
        let rank = defaults
            .iter()
            .position(|d| *d == app.id)
            .unwrap_or(usize::MAX);
        (rank, app.name.to_lowercase())
    });
    matching
}

pub fn launch(app: &App, path: &Path) -> std::io::Result<()> {
    let argv = exec_argv(&app.exec, path);
    let Some((program, args)) = argv.split_first() else {
        return Err(std::io::Error::other("empty Exec line"));
    };
    // Same Steam overlay/runtime leak as `iprolaunch::open_gui`: when
    // BrowDeck runs as a Steam shortcut these would be forced on the app.
    let mut child = std::process::Command::new(program)
        .args(args)
        .env_remove("LD_PRELOAD")
        .env_remove("LD_LIBRARY_PATH")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    // Reaped on its own thread so a closed app doesn't linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

/// Highest-priority first. The Flatpak export dirs are listed explicitly:
/// under Game Mode `XDG_DATA_DIRS` often lacks them, and most Deck apps
/// are Flatpaks.
fn application_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs = Vec::new();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
    if let Some(d) = data_home {
        dirs.push(d.join("applications"));
    }
    if let Some(h) = &home {
        dirs.push(h.join(".local/share/flatpak/exports/share/applications"));
    }
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(Path::new(d).join("applications"));
    }
    let mut seen = HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

/// Desktop ids are the path under `applications/` with `/` turned into
/// `-`; the first directory to provide an id wins.
fn collect_entries(
    root: &Path,
    dir: &Path,
    seen: &mut HashSet<String>,
    out: &mut Vec<(App, Vec<String>)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_entries(root, &path, seen, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let id = rel.to_string_lossy().replace('/', "-");
        if !seen.insert(id.clone()) {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some((name, exec, mimes)) = parse_desktop_entry(&contents) {
            out.push((App { id, name, exec }, mimes));
        }
    }
}

/// `(Name, Exec, MimeType list)` of a launchable application entry; `None`
/// for anything `Hidden`, non-application, or incomplete.
fn parse_desktop_entry(contents: &str) -> Option<(String, String, Vec<String>)> {
    let mut in_main = false;
    let mut name = None;
    let mut exec = None;
    let mut mimes = Vec::new();
    let mut is_app = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Type" => is_app = value == "Application",
            "Name" => name = Some(value.to_string()),
            "Exec" => exec = Some(unescape_value(value)),
            "MimeType" => {
                mimes = value
                    .split(';')
                    .filter(|m| !m.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            // `NoDisplay` only hides an app from menus — the spec still
            // counts it as a handler for its MIME types, so it stays.
            // Terminal apps need a terminal to host them, which this
            // launches without.
            "Hidden" | "Terminal" if value == "true" => return None,
            _ => {}
        }
    }
    if !is_app {
        return None;
    }
    Some((name?, exec?, mimes))
}

/// The `.desktop` string-value escapes (`\s`, `\n`, `\t`, `\r`, `\\`).
fn unescape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Splits an `Exec` line into argv per the Desktop Entry spec (quoting,
/// field codes), substituting `path` for the file/URL codes — appended at
/// the end if the line has none.
fn exec_argv(exec: &str, path: &Path) -> Vec<String> {
    let file = path.to_string_lossy();
    let mut argv = Vec::new();
    let mut used_file = false;
    for token in split_exec(exec) {
        if matches!(token.as_str(), "%f" | "%F" | "%u" | "%U") {
            argv.push(file.to_string());
            used_file = true;
            continue;
        }
        let mut arg = String::new();
        let mut chars = token.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '%' {
                arg.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => arg.push('%'),
                Some('f' | 'F' | 'u' | 'U') => {
                    arg.push_str(&file);
                    used_file = true;
                }
                // %i/%c/%k and the deprecated codes expand to nothing here.
                _ => {}
            }
        }
        if !arg.is_empty() {
            argv.push(arg);
        }
    }
    if !used_file {
        argv.push(file.to_string());
    }
    argv
}

fn split_exec(exec: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut in_quotes = false;
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => in_quotes = false,
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                }
                _ => current.push(c),
            }
        } else if c == '"' {
            in_quotes = true;
            in_token = true;
        } else if c.is_whitespace() {
            if in_token {
                tokens.push(std::mem::take(&mut current));
                in_token = false;
            }
        } else {
            current.push(c);
            in_token = true;
        }
    }
    if in_token {
        tokens.push(current);
    }
    tokens
}

/// MIME type for a file name from shared-mime-info's `globs2`
/// (`weight:type:glob[:flags]`), highest weight then longest glob winning.
fn mime_for(file_name: &str, globs2: &str) -> Option<String> {
    let lower = file_name.to_lowercase();
    let mut best: Option<(u32, usize, &str)> = None;
    for line in globs2.lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut parts = line.split(':');
        let (Some(weight), Some(mime), Some(glob)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let case_sensitive = parts
            .next()
            .is_some_and(|f| f.split(',').any(|f| f == "cs"));
        let Ok(weight) = weight.parse::<u32>() else {
            continue;
        };
        let (name, pattern) = if case_sensitive {
            (file_name, glob.to_string())
        } else {
            (lower.as_str(), glob.to_lowercase())
        };
        let matched = match pattern.strip_prefix('*') {
            Some(suffix) if !suffix.contains(['*', '?', '[']) => name.ends_with(suffix),
            Some(_) => false,
            None if !pattern.contains(['*', '?', '[']) => name == pattern,
            None => false,
        };
        if !matched {
            continue;
        }
        let candidate = (weight, glob.len(), mime);
        if best.is_none_or(|(w, l, _)| (candidate.0, candidate.1) > (w, l)) {
            best = Some(candidate);
        }
    }
    best.map(|(_, _, mime)| mime.to_string())
}

/// `(default app ids, extra associated app ids)` for `mime` from a
/// `mimeapps.list`.
fn parse_mimeapps(contents: &str, mime: &str) -> (Vec<String>, Vec<String>) {
    let mut defaults = Vec::new();
    let mut added = Vec::new();
    let mut section = "";
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != mime {
            continue;
        }
        let ids = value
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim().to_string());
        match section {
            "[Default Applications]" => defaults.extend(ids),
            "[Added Associations]" => added.extend(ids),
            _ => {}
        }
    }
    (defaults, added)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLOBS: &str = "\
# comment
50:image/png:*.png
40:image/apng:*.png
50:text/plain:*.txt
50:text/x-makefile:Makefile
50:text/x-c++src:*.C:cs
55:application/x-compressed-tar:*.tar.gz
50:application/gzip:*.gz
";

    #[test]
    fn mime_prefers_weight_then_longest_glob() {
        assert_eq!(mime_for("a.png", GLOBS).as_deref(), Some("image/png"));
        assert_eq!(mime_for("PHOTO.PNG", GLOBS).as_deref(), Some("image/png"));
        assert_eq!(
            mime_for("x.tar.gz", GLOBS).as_deref(),
            Some("application/x-compressed-tar")
        );
        assert_eq!(
            mime_for("Makefile", GLOBS).as_deref(),
            Some("text/x-makefile")
        );
        assert_eq!(mime_for("unknown.zzz", GLOBS), None);
    }

    #[test]
    fn case_sensitive_globs_only_match_exact_case() {
        assert_eq!(mime_for("a.C", GLOBS).as_deref(), Some("text/x-c++src"));
        assert_eq!(mime_for("a.c", GLOBS), None);
    }

    #[test]
    fn desktop_entry_parses_main_section_only() {
        let entry = "\
[Desktop Entry]
Type=Application
Name=Image Viewer
Exec=eog %U
MimeType=image/png;image/jpeg;

[Desktop Action new-window]
Name=New Window
Exec=eog --new-window
";
        let (name, exec, mimes) = parse_desktop_entry(entry).unwrap();
        assert_eq!(name, "Image Viewer");
        assert_eq!(exec, "eog %U");
        assert_eq!(mimes, vec!["image/png", "image/jpeg"]);
    }

    #[test]
    fn hidden_and_non_application_entries_are_skipped() {
        let hidden = "[Desktop Entry]\nType=Application\nName=X\nExec=x\nHidden=true\n";
        assert!(parse_desktop_entry(hidden).is_none());
        let handler_only = "[Desktop Entry]\nType=Application\nName=X\nExec=x\nNoDisplay=true\n";
        assert!(parse_desktop_entry(handler_only).is_some());
        let link = "[Desktop Entry]\nType=Link\nName=X\nURL=https://example.com\n";
        assert!(parse_desktop_entry(link).is_none());
        let tui = "[Desktop Entry]\nType=Application\nName=Vim\nExec=vim %F\nTerminal=true\n";
        assert!(parse_desktop_entry(tui).is_none());
    }

    #[test]
    fn exec_substitutes_field_codes_and_keeps_quoting() {
        let p = Path::new("/media/my files/a.png");
        assert_eq!(exec_argv("eog %U", p), vec!["eog", "/media/my files/a.png"]);
        assert_eq!(
            exec_argv(r#""/opt/My App/run" --open=%f %i %c"#, p),
            vec!["/opt/My App/run", "--open=/media/my files/a.png"]
        );
        assert_eq!(
            exec_argv("viewer", p),
            vec!["viewer", "/media/my files/a.png"]
        );
        assert_eq!(
            exec_argv("app 100%%", p),
            vec!["app", "100%", "/media/my files/a.png"]
        );
    }

    #[test]
    fn exec_handles_flatpak_file_forwarding() {
        let p = Path::new("/home/deck/a.png");
        assert_eq!(
            exec_argv(
                "/usr/bin/flatpak run --file-forwarding org.gnome.eog @@ %f @@",
                p
            ),
            vec![
                "/usr/bin/flatpak",
                "run",
                "--file-forwarding",
                "org.gnome.eog",
                "@@",
                "/home/deck/a.png",
                "@@"
            ]
        );
    }

    #[test]
    fn value_escapes_are_decoded() {
        assert_eq!(unescape_value(r"a\sb\\c"), r"a b\c");
    }

    #[test]
    fn mimeapps_defaults_and_added_associations() {
        let list = "\
[Default Applications]
image/png=org.gnome.eog.desktop;gimp.desktop;
text/plain=kate.desktop

[Added Associations]
image/png=krita.desktop;
";
        let (defaults, added) = parse_mimeapps(list, "image/png");
        assert_eq!(defaults, vec!["org.gnome.eog.desktop", "gimp.desktop"]);
        assert_eq!(added, vec!["krita.desktop"]);
    }
}
