## [Unreleased]

## [1.1.0] — 2026-09-08

### Added
- Show/hide hidden files toggle in the toolbar (hidden by default)
- IProLaunch action badge shows a checkmark and disables itself once a file
  is already registered in the library
- `packaging/install.sh` and `uninstall.sh` for the plain-archive release
- Automated GitHub Actions release build (tar.gz, draft release)

### Changed
- Right-stick scroll moves the selection directly instead of a free-floating
  view offset, and is ~1.75x faster
- Refresh-mounts and "show all mounts" moved next to the Up button
- Scrollbars are always visible instead of only on mouse hover
- D-pad/stick navigation inside the Actions/Permissions strip now steps
  row-aware between badges instead of using geometric search
- Closing Actions and Permissions together now closes Permissions first
- More toolbar buttons have hover tooltips

### Fixed
- Actions/Permissions strip could render blank
- D-pad/stick navigation could escape the Actions/Permissions strip into
  the file list behind it
- Action badges visibly shifted size when gaining/losing focus
- Right-stick scroll could stop entirely once the focused row scrolled
  off-screen

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
