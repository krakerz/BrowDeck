![BrowDeck screenshot](docs/screenshot.png)

# BrowDeck

A gamepad-first file browser for Linux, built for Steam gamemode / gamescope.

## Description

Every action — navigate, select, copy/cut/delete, extract, change
permissions — is reachable from a controller alone, no nested right-click
menus. Windowed at 1280x800 (Steam Deck LCD's native resolution) by
default; borderless fullscreen is available via `config.toml`.

## Features

- Full gamepad navigation — d-pad/stick movement stays confined to
  whichever pane (sidebar, file list, actions/preview, toolbar) is
  focused; LB/RB switches between panes
- Copy, cut, delete (to trash), extract (`.zip`/`.tar`/`.tar.gz`/`.tgz`),
  rename/new folder, and change permissions (single file or a whole
  multi-selection), all from a button-driven actions panel
- Multi-select for batch operations
- Sidebar: XDG places, real block-device mounts with USB/microSD
  detection, pinned folders, a trash view with restore/empty, and a
  selection info card (owner/group/size/permissions/link target)
- Pin any folder to the sidebar; unpin from the Actions strip or the
  sidebar itself
- Sort the file list by name, size, or modified date, ascending or
  descending
- Mimetype-aware file icons (archives, images, audio, video, PDFs, code,
  fonts, …)
- Quick preview pane for images and text files
- Show/hide hidden files toggle (hidden by default)
- In-folder search, with an optional recursive (subfolder) mode
- Material Design icons; LT/RT adjusts the scale of the sidebar and main
  pane specifically
- CJK (Japanese/Chinese/Korean) filename rendering
- R3 to quit, with a confirmation in the actions strip
- Optional IProLaunch integration — an "Add to IProLaunch" action for
  `.exe`/`.bat` files when IProLaunch is installed

## Installation

Grab the latest release from the
[Releases page](https://github.com/krakerz/BrowDeck/releases), extract the
`.tar.gz`, then run `./install.sh` (`./uninstall.sh` to remove it later) —
see the bundled `INSTALL.txt`.

Or build from source (below).

## Building from source

Requires a recent stable Rust toolchain (2024 edition).

```sh
git clone <this-repo>
cd BrowDeck
cargo build --release
./target/release/browdeck
```

BrowDeck writes `$XDG_CONFIG_HOME/browdeck/config.toml` (falling back to
`~/.config/browdeck/config.toml`) itself on first run, with sane defaults
already set — windowed at 1280x800, header on. Edit it to go fullscreen,
change the resolution, or turn the header off; see
`config/config.example.toml` for every available key.

## Usage

Navigate with the d-pad or left stick; the right stick moves the selection
(or scrolls, in Preview) faster. Gamepad button reference:

| Button    | Action                                    |
|-----------|--------------------------------------------|
| A         | Activate (open/select focused item)         |
| B         | Back — close a menu, or go up a directory   |
| X         | Actions — select the focused item and open the actions panel |
| Y (tap)   | Refresh the active pane                     |
| Y (hold)  | Search                                      |
| L3        | Toggle multi-select                         |
| R3        | Quit (asks for confirmation in the actions strip) |
| Start     | Open the selected item                      |
| Select    | Toggle preview pane width                   |
| LB / RB   | Switch pane (Sidebar / main list / Preview / Toolbar) |
| LT / RT   | Icon scale down / up                        |

Everything is also reachable with a mouse and keyboard.

## FAQ

**Why windowed by default, not fullscreen?**
Confirmed the reliable combination under gamescope's nested Xwayland on
real Steam Deck hardware. Set `fullscreen = true` in `config.toml` for
borderless fullscreen instead.

**Does it work without a gamepad?**
Yes — mouse and keyboard work throughout, and the status bar's button
legend always shows as a reference for the equivalents.

**Does it follow symlinks?**
Directory listings and navigation follow symlinks; recursive search
doesn't descend into symlinked directories, to avoid loops.

**How does the IProLaunch integration work?**
At startup, BrowDeck checks `~/.config/iprolaunch/bin-path` for the
[IProLaunch](https://github.com/krakerz/IProLaunch) CLI binary. If found,
selecting an `.exe`/`.bat` file and opening the actions panel shows an
"Add to IProLaunch" button; already-registered files show a disabled
checkmark instead.

---

### Notes

Built and maintained with the help of AI.

Licensed under [GPL-3.0-or-later](LICENSE).
