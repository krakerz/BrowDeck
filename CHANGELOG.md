## [Unreleased]

## [1.0.0] — 2026-09-08

### Added
- Multi-select for batch copy/cut/delete
- `.tar`/`.tar.gz`/`.tgz` extraction
- Recursive (subfolder) search with a loading indicator
- Persistent status bar with gamepad legend and app version
- Select a folder without opening it
- Optional IProLaunch integration for `.exe`/`.bat` files

### Changed
- Full gamepad remap — see README for the button reference
- D-pad/stick navigation confined to the focused pane; LB/RB switches panes
- LB/RB can't reach the toolbar while the actions/permissions menu is open
- Focused row auto-scrolls into view; right-stick scrolling fixed
- Right pane zones stack compactly from the bottom instead of a fixed split
- Inactive panes dim instead of a highlight border on the active one
- Image preview fills the available pane space
- Refresh and "show all mounts" moved to the toolbar

### Fixed
- File/trash list wasn't refreshing after copy/paste or extract
- Misconfigured `XDG_DOWNLOAD_DIR` could point Downloads at Home
- Status bar and right-pane action buttons could get clipped at the bottom

## [0.9.0] — 2026-09-07

### Added
- Gamepad-first file browser for Linux: navigate directories, copy, cut,
  delete to trash, extract `.zip` archives, and change file permissions
- Fullscreen-by-default window (native output resolution), with an
  overridable resolution/windowed config file
- Collapsible sidebar: XDG places (Home/Desktop/Documents/…), real
  block-device mounts with USB/microSD detection, and a trash view
- Gamepad navigation: d-pad/stick directional focus, face-button
  activate/back, sidebar toggle, and a button-driven actions panel in
  place of a mouse-style right-click menu
- Quick preview pane for images and text files, toggleable
- Material Design icons throughout, with an icon-scale control
- CJK (Japanese/Chinese/Korean) filename rendering support
