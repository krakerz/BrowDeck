# BrowDeck

A gamepad-first file browser for Linux, built for Steam gamemode / gamescope.

## Description

Most file browsers assume a mouse and keyboard. BrowDeck doesn't — every
action (navigate, select, copy/cut/delete, extract, change permissions) is
reachable from a controller alone, with no nested right-click menus.
Fullscreen by default at the display's native resolution, so it behaves
correctly under gamescope instead of rendering at some fallback resolution
and getting blurred.

## Features

- Full gamepad navigation — d-pad/stick movement stays confined to whichever
  pane (sidebar, file list, actions/preview, toolbar) is focused; LB/RB
  switches between panes
- Copy, cut, delete (to trash), extract (`.zip`/`.tar`/`.tar.gz`/`.tgz`),
  and change permissions, all from a button-driven actions panel
- Multi-select for batch operations
- Sidebar: XDG places (Home/Desktop/Documents/…), real block-device mounts
  with USB/microSD detection, and a trash view with restore/empty
- Quick preview pane for images and text files
- In-folder search, with an optional recursive (subfolder) mode
- Material Design icons with an adjustable icon scale
- CJK (Japanese/Chinese/Korean) filename rendering
- Optional IProLaunch integration — an "Add to IProLaunch" action appears
  for `.exe`/`.bat` files when IProLaunch is installed

## Installation

No packaged releases yet — build from source (below).

## Building from source

Requires a recent stable Rust toolchain (2024 edition).

```sh
git clone <this-repo>
cd BrowDeck
cargo build --release
./target/release/browdeck
```

Fullscreen/windowed mode and resolution are configurable via
`$XDG_CONFIG_HOME/browdeck/config.toml` (defaults to fullscreen at the
display's native resolution).

## Usage

Navigate with the d-pad or left stick; the right stick scrolls the focused
pane. Gamepad button reference:

| Button    | Action                                    |
|-----------|--------------------------------------------|
| A         | Activate (open/select focused item)         |
| B         | Back — close a menu, or go up a directory   |
| X         | Actions — select the focused item and open the actions panel |
| Y (tap)   | Refresh the active pane                     |
| Y (hold)  | Search                                      |
| L3        | Toggle multi-select                         |
| R3        | Open the selected item                      |
| Start     | Open the selected item                      |
| Select    | Toggle preview pane width                   |
| LB / RB   | Switch pane (Sidebar / main list / Toolbar) |
| LT / RT   | Icon scale down / up                        |

Everything is also reachable with a mouse and keyboard.

## FAQ

**Why fullscreen by default?**
Because rendering at a fallback resolution and letting the compositor
upscale/blur the result is the exact gamescope behavior this project exists
to avoid.

**Does it work without a gamepad?**
Yes — mouse and keyboard work throughout; the gamepad button legend only
appears in the status bar when a controller is actually connected.

**Does it follow symlinks?**
Directory listings and navigation follow symlinks; the recursive search
does not descend into symlinked directories, to avoid loops.

**How does the IProLaunch integration work?**
At startup, BrowDeck reads `~/.config/iprolaunch/bin-path` (a config file
whose contents are the path to the
[IProLaunch](https://github.com/krakerz/IProLaunch) CLI binary) and
checks that the binary it points to exists. If so, selecting an
`.exe`/`.bat` file and opening the actions panel shows an "Add to
IProLaunch" button, which runs that binary with
`add <absolute path to the selected file>`.

---

### Notes

Built and maintained with the help of AI.
