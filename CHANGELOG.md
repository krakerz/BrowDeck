## [Unreleased]

## [1.6.0] — 2026-09-09

### Added
- Pin/unpin any folder to the sidebar from the Actions strip
- Unpin a pinned folder directly from the sidebar (right-click, or gamepad context-menu)

## [1.5.4] — 2026-09-09

### Fixed
- Toolbar "Refresh" now also refreshes Places, not just Mounts
- A Place could vanish if `user-dirs.dirs` was misconfigured or missing an entry — falls back to the conventional `$HOME/<Name>` path

## [1.5.3] — 2026-09-09

### Fixed
- Recognize `/run/media/*` as a common mount location

## [1.5.2] — 2026-09-09

### Fixed
- Recentered off-center title text on the Steam library background/logo images

## [1.5.1] — 2026-09-09

### Fixed
- Sidebar info card now refreshes after a permission change

## [1.5.0] — 2026-09-09

### Added
- Permissions action disables (with a tooltip) when you don't own the file and aren't root

## [1.4.0] — 2026-09-08

### Changed
- Relicensed MIT → GPL-3.0-or-later
- Quit is now R3, not holding Start
- Quit confirmation is a strip row instead of a popup
- Default `config.toml` is windowed 1280x800 instead of fullscreen

### Fixed
- Quit confirmation popup could break gamepad input on real hardware
- Windowed mode could render narrower than configured under gamescope

## [1.3.0] — 2026-09-08

### Added
- `config.toml` auto-generated on first launch
- `show_header` config option
- README screenshot

### Changed
- Status bar legend centered, version moved to its own spot, labels trimmed, height reduced
- Button legend always shows, even without a gamepad connected
- Face-button badges are true circles
- Actions/Permissions/Rename strip: uniform badge height, auto-fit content height, small top/bottom padding
- LT/RT icon scale confined to sidebar and main pane
- Zoom tooltips renamed "Smaller/Bigger scale"

### Fixed
- Rename/New Folder field no longer auto-focuses for editing on open

## [1.2.2] — 2026-09-08

### Added
- Thin title header above the toolbar

### Fixed
- D-pad/stick toolbar navigation could snap back at a disabled zoom button

## [1.2.1] — 2026-09-08

### Fixed
- Cold launch under gamescope could render at 800x600
- Installed `.desktop` entry's `Exec` path

## [1.2.0] — 2026-09-08

### Added
- Mimetype-aware file icons
- Sort file list by name/size/modified date, ascending/descending
- Rename and New Folder actions
- Mass permission change (chmod) for multi-selection
- Sidebar info card for the current selection
- Preview pane reachable via LB/RB

### Changed
- LB/RB pane order: Places/Active/Preview/Toolbar
- Permissions badge available for any selection, not just single files

### Fixed
- Preview via LB/RB could fail to focus
- Opening Actions/Permissions from within Actions could steal focus

## [1.1.0] — 2026-09-08

### Added
- Hidden files toggle (hidden by default)
- IProLaunch checkmark/disable once a file is already registered
- `packaging/install.sh`/`uninstall.sh`
- Automated GitHub Actions release build

### Changed
- Right-stick scroll moves the selection directly, ~1.75x faster
- Refresh/"show all mounts" moved to the toolbar
- Scrollbars always visible
- Actions/Permissions strip navigation is row-aware
- Closing Actions and Permissions together closes Permissions first
- More toolbar tooltips

### Fixed
- Actions/Permissions strip could render blank
- D-pad/stick navigation could escape the Actions/Permissions strip
- Action badges visibly shifted size on focus change
- Right-stick scroll could stop once the focused row scrolled off-screen
- Opening an executable script could hit a desktop-portal dialog instead of running
- `.exe`/`.bat` now launch via IProLaunch when available

## [1.0.0] — 2026-09-08

### Added
- Multi-select for batch copy/cut/delete
- `.tar`/`.tar.gz`/`.tgz` extraction
- Recursive (subfolder) search with a loading indicator
- Persistent status bar with gamepad legend and app version
- Select a folder without opening it
- Optional IProLaunch integration

### Changed
- Full gamepad remap
- D-pad/stick navigation confined to the focused pane; LB/RB switches panes
- Focused row auto-scrolls into view
- Right pane zones stack from the bottom instead of a fixed split
- Inactive panes dim instead of a highlight border
- Image preview fills the available pane space

### Fixed
- File/trash list wasn't refreshing after copy/paste/extract
- Misconfigured `XDG_DOWNLOAD_DIR` could point Downloads at Home
- Status bar and action buttons could clip at the bottom

## [0.9.0] — 2026-09-07

### Added
- Gamepad-first file browser: navigate, copy, cut, delete to trash, extract `.zip`, change permissions
- Fullscreen-by-default window, overridable resolution via config
- Sidebar: XDG places, block-device mounts with USB/microSD detection, trash view
- Gamepad navigation: d-pad/stick focus, face buttons, button-driven actions panel
- Preview pane for images and text files
- Material Design icons with a scale control
- CJK (Japanese/Chinese/Korean) filename rendering
