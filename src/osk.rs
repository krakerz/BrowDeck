//! Steam's on-screen keyboard, opened the same way SDL does for games on the
//! Deck: Steam handles `steam://open/keyboard` / `steam://close/keyboard`
//! URLs, and sets `SteamDeck=1` in the environment of everything it launches
//! there (SDL gates on the same variable).

use std::process::{Command, Stdio};

pub fn available() -> bool {
    std::env::var("SteamDeck").is_ok_and(|v| v == "1")
}

/// `field` is the text field's rect in physical pixels, relative to the
/// window — Steam keeps the keyboard from covering it where it can.
pub fn open(field: Option<[i32; 4]>) {
    send(&open_url(field));
}

pub fn close() {
    send("steam://close/keyboard");
}

/// `Mode=0` is Steam's single-line mode: Enter submits and dismisses.
fn open_url(field: Option<[i32; 4]>) -> String {
    match field {
        Some([x, y, w, h]) => {
            format!("steam://open/keyboard?XPosition={x}&YPosition={y}&Width={w}&Height={h}&Mode=0")
        }
        None => "steam://open/keyboard?Mode=0".to_string(),
    }
}

fn send(url: &str) {
    // Steam's overlay/runtime libraries would otherwise leak into xdg-open
    // (same reason as `iprolaunch::open_gui`).
    let spawned = Command::new("xdg-open")
        .arg(url)
        .env_remove("LD_PRELOAD")
        .env_remove("LD_LIBRARY_PATH")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match spawned {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => eprintln!("couldn't open {url}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_url_carries_the_field_rect_like_sdl() {
        assert_eq!(
            open_url(Some([10, 700, 300, 40])),
            "steam://open/keyboard?XPosition=10&YPosition=700&Width=300&Height=40&Mode=0"
        );
        assert_eq!(open_url(None), "steam://open/keyboard?Mode=0");
    }
}
