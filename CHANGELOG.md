## [Unreleased]

## [1.5.1] — 2026-09-09

### Fixed
- The sidebar info card kept showing the old permissions string after
  applying a permission change — it caches per-selection and chmod
  doesn't change the selection, so nothing told it to recompute

## [1.5.0] — 2026-09-09

### Added
- The Permissions action greys out (with a tooltip) instead of opening
  when you don't own the selected file(s) and aren't root — changing
  permissions would just fail otherwise

## [1.4.0] — 2026-09-08

### Changed
- Relicensed from MIT to GPL-3.0-or-later
- Quit is now R3 (a plain press), not holding Start — holding Start
  turned out to break gamepad input on its own, regardless of what the
  confirmation UI was; R3 previously opened the selected item, which
  Start alone still does
- The Quit confirmation is now a strip row (like Rename/Permissions)
  instead of a popup
- Default `config.toml` is now windowed at 1280x800 (Steam Deck LCD's
  native resolution) instead of fullscreen — confirmed the reliable
  combination on real hardware; fullscreen is still available by setting
  `fullscreen = true`

### Fixed
- The Quit confirmation popup could break gamepad input entirely on real
  Steam Deck hardware (switched to acting like a mouse, requiring a hard
  restart)
- Windowed mode could still render narrower than configured on real
  gamescope hardware (status bar legend cut off) despite the earlier
  cold-launch sizing fix — the window size is now re-asserted for the
  first several frames after launch, not just requested once at creation

## [1.3.0] — 2026-09-08

### Added
- `config.toml` is now generated automatically on first launch (fullscreen
  and the header both explicitly enabled), instead of only existing if a
  user copies the example file in themselves
- The title header can be turned off via `show_header = false` in
  `config.toml`
- README screenshot

### Changed
- The bottom status bar (gamepad button legend)'s content is now centered
  as a group instead of left-aligned, with the version tucked in its own
  spot on the right — it was getting cut off at narrower/windowed sizes
  with no way to see the rest
- The button legend always shows, even with no gamepad connected, so it
  also documents the mouse/keyboard equivalents
- Trimmed a couple of legend labels ("Back/Up dir" → "Back", "Refresh
  (hold: Search)" → "Refresh (Search)") to leave more room
- Status bar is a bit shorter
- A/B/X/Y face-button badges are drawn as true circles (fixed diameter,
  painted directly), matching real controller buttons, instead of a
  rounded-rectangle guess that came out oval
- Actions/Permissions/Rename strip badges are now a uniform height
  whether or not they have an icon — a text-only badge ("Cancel",
  "Apply") used to render noticeably shorter than an icon+text one
- The Actions/Permissions/Rename/Progress strip auto-fits its actual
  content height instead of a formula guess that always left a gap
  below the last row
- LT/RT (icon scale) now only zooms the sidebar (Places/Mounts/Trash)
  and the main pane (file list, Trash view, in-folder search) — the
  toolbar, Preview, and the Actions/Permissions/Rename/Progress strip
  stay a fixed size
- Small top/bottom padding in the Actions/Permissions/Rename/Progress
  strip, so the first row doesn't sit flush against its own top border
- Zoom in/out tooltips renamed "Smaller/Bigger icons" → "Smaller/Bigger
  scale"

### Fixed
- Renaming/creating a folder auto-focused the name field for real editing
  the instant the panel opened, so the very next d-pad press could move
  the text cursor instead of navigating — the field now opens as a plain
  navigable badge; pressing A (or clicking) enters editing, B leaves it
  (without closing the panel)

## [1.2.2] — 2026-09-08

### Added
- Thin title header above the toolbar ("BrowDeck — Browse on Deck") so the
  toolbar's own icons no longer sit directly under Steam/gamescope's
  performance overlay

### Fixed
- D-pad/stick navigation across the toolbar could snap back to the
  hamburger button when it reached a disabled zoom button (icon scale
  already at its min/max)

## [1.2.1] — 2026-09-08

### Fixed
- First cold launch under gamescope (Steam Deck Game Mode) could render at
  a small, blurry, stretched 800x600 instead of the real display resolution
- The installed `.desktop` entry's `Exec=browdeck` failed outside a shell
  that has `~/.local/bin` on `$PATH` — now points at the absolute installed
  path instead

## [1.2.0] — 2026-09-08

### Added
- Mimetype-aware file icons (archive/image/audio/video/pdf/code/data/font/
  text categories, generic fallback for the rest)
- Sort the file list by name/size/modified date, ascending or descending,
  from the toolbar
- Rename and New Folder actions in the Actions strip
- Mass permission change (chmod) when multiple files/folders are selected
- Sidebar info card for the current single selection: owner, group, size
  (recursive total for folders), permissions, and link target for symlinks
- Preview pane is now reachable via LB/RB alongside Sidebar/Active/Toolbar
- Hold Start for 3s to bring up a Yes/Cancel Quit confirmation — no
  keyboard Alt+F4 to rely on under gamescope/Game Mode

### Changed
- LB/RB pane order is now Places/Active/Preview/Toolbar
- Permissions badge is available for any selection, not just a single file

### Fixed
- Preview via LB/RB could silently fail to focus except right after using
  the Actions strip (stale shared focus state)
- Opening Actions/Permissions from within Actions could steal that zone's
  own closing check before it rendered

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
- Opening an executable script could fail with a desktop-portal security
  dialog instead of running
- Opening an `.exe`/`.bat` now launches it through IProLaunch directly
  when available, instead of going through the desktop portal at all

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
