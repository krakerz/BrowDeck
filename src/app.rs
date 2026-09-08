use crate::{
    deleted, fileicons, fileinfo, fileops, fonts, gamepad, iprolaunch, mounts, permissions, places,
    preview,
};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_ACCOUNT_TREE, ICON_ARROW_DOWNWARD, ICON_ARROW_UPWARD, ICON_CHECK_CIRCLE, ICON_CLOSE,
    ICON_CONTENT_COPY, ICON_CONTENT_CUT, ICON_CONTENT_PASTE, ICON_CREATE_NEW_FOLDER, ICON_DELETE,
    ICON_DELETE_SWEEP, ICON_DRIVE_FILE_RENAME_OUTLINE, ICON_GAMEPAD, ICON_LOCK, ICON_MENU,
    ICON_OPEN_IN_NEW, ICON_PREVIEW, ICON_PRIORITY_HIGH, ICON_REFRESH, ICON_RESTORE_FROM_TRASH,
    ICON_ROCKET_LAUNCH, ICON_SEARCH, ICON_SORT, ICON_UNARCHIVE, ICON_VISIBILITY,
    ICON_VISIBILITY_OFF, ICON_ZOOM_IN, ICON_ZOOM_OUT,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Default `ScrollBarVisibility::VisibleWhenNeeded` only paints the bar on
/// mouse hover, not just because content overflows — useless without a
/// cursor during gamepad play. All `ScrollArea`s use this instead.
const ALWAYS_VISIBLE_SCROLLBAR: egui::containers::scroll_area::ScrollBarVisibility =
    egui::containers::scroll_area::ScrollBarVisibility::AlwaysVisible;
/// Base size (px) icons render at before `icon_scale` is applied.
const BASE_ICON_SIZE: f32 = 18.0;
const ICON_SCALE_STEP: f32 = 0.15;
const ICON_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.7..=2.5;
/// Points scrolled per frame per unit of right-stick tilt — used for the
/// Right pane (Preview/Actions), which has no "selection" concept.
const SCROLL_SPEED: f32 = 12.0;
/// Throttle for right-stick-driven selection movement in Sidebar/Active —
/// `Action::Scroll` fires every frame the stick is tilted, unlike the
/// d-pad's edge-triggered presses.
const SCROLL_MOVE_INTERVAL: Duration = Duration::from_millis(51);
/// The right pane's width when Select's preview-width toggle is off.
const PREVIEW_NORMAL_WIDTH: f32 = 320.0;
/// Fixed height of the sidebar's selected-item info card — deliberately
/// not auto-fit (see NOTES.md's running lesson on that), and internally
/// scrollable so a long symlink target or a mounts list squeezing it
/// doesn't overflow.
const SIDEBAR_INFO_HEIGHT: f32 = 150.0;
/// Height of the thin title bar reserved above the toolbar — its whole
/// purpose is to push the real, interactive toolbar icons down out of the
/// top-left corner, where Steam/gamescope's own performance overlay
/// (MangoHud-style FPS/CPU/GPU readout) renders on top of the window and
/// would otherwise sit right on top of them. Tall enough to clear that
/// overlay's usual height with a little headroom.
const HEADER_HEIGHT: f32 = 30.0;

struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    /// 0 for directories — sorting by size only makes sense for files;
    /// dirs already always sort first regardless of `SortBy`.
    size: u64,
    modified: Option<std::time::SystemTime>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SortBy {
    Name,
    Size,
    Modified,
}

impl SortBy {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::Size,
            Self::Size => Self::Modified,
            Self::Modified => Self::Name,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Size => "Size",
            Self::Modified => "Modified",
        }
    }
}

/// Dirs always sort before files, regardless of `sort_by`/`ascending` —
/// only the order *within* dirs and *within* files follows those. A free
/// function (not a method) so it's unit-testable without constructing a
/// full `BrowDeckApp`.
fn compare_entries(a: &Entry, b: &Entry, sort_by: SortBy, ascending: bool) -> std::cmp::Ordering {
    let primary = match sort_by {
        SortBy::Name => a.name.cmp(&b.name),
        SortBy::Size => a.size.cmp(&b.size),
        SortBy::Modified => a.modified.cmp(&b.modified),
    };
    let primary = if ascending {
        primary
    } else {
        primary.reverse()
    };
    b.is_dir.cmp(&a.is_dir).then(primary)
}

struct Clipboard {
    paths: Vec<PathBuf>,
    cut: bool,
}

struct PermEditor {
    /// One file, or every path in a multi-selection — Apply sets the same
    /// mode on all of them.
    paths: Vec<PathBuf>,
    mode: u32,
}

struct RenameEditor {
    /// `None` — creating a new folder in `current_dir`. `Some(path)` —
    /// renaming that path.
    target: Option<PathBuf>,
    text: String,
}

enum View {
    Dir,
    Trash,
}

/// Which pane currently holds keyboard focus. `Sidebar`/`Active`/`Toolbar`
/// always cycle via LB/RB (see `pane_cycle`); `Right` joins that cycle
/// only while Preview is showing — the Actions/Permissions/Progress strip
/// (also `Right`) is reached only via its own explicit triggers (X, R3,
/// the Permissions button, …), never LB/RB. Whichever way it's reached,
/// dpad/stick input stays confined to it like any other pane while it's
/// focused.
#[derive(Clone, Copy, PartialEq)]
enum Pane {
    Sidebar,
    Active,
    Toolbar,
    Right,
}

pub struct BrowDeckApp {
    sidebar_open: bool,
    current_dir: PathBuf,
    entries: Vec<Entry>,
    view: View,
    trash_entries: Vec<deleted::TrashEntry>,
    selected_trash: Option<usize>,
    places: Vec<places::Place>,
    mounts: Vec<mounts::Mount>,
    selected: Option<PathBuf>,
    clipboard: Option<Clipboard>,
    jobs: Vec<fileops::Job>,
    gamepad: Option<gamepad::GamepadInput>,
    icon_scale: f32,
    show_all_mounts: bool,
    /// Whether dotfile entries (name starting with `.`) show up in the
    /// file list — off by default, matching every mainstream file manager.
    /// Filtered at render time (see the dir-list loop), same approach as
    /// `show_all_mounts`, so toggling doesn't need a re-`refresh()`.
    show_hidden_files: bool,
    /// Applied in `sort_entries` — folders always sort first regardless
    /// of either of these, only the order within dirs/within files
    /// changes.
    sort_by: SortBy,
    sort_ascending: bool,
    preview_enabled: bool,
    /// Gamepad Select toggles this — widens the right pane to 40% of the
    /// window instead of its normal (default/resizable) width.
    preview_wide: bool,
    /// Cached text-preview content, keyed by path so it's only re-read from
    /// disk when the selection actually changes, not every frame.
    preview_text: Option<(PathBuf, String)>,
    context_menu_open: bool,
    perm_editor: Option<PermEditor>,
    rename_editor: Option<RenameEditor>,
    /// True while the Rename/New Folder name field has *real* keyboard
    /// focus (not just d-pad selection) — while set, the field renders as
    /// an actual `TextEdit` and gamepad d-pad/left-stick input is
    /// swallowed entirely rather than routed through the normal strip
    /// navigation. See `show_rename_editor`'s doc comment for why: once
    /// the field has genuine focus, Steam Input's own text-field handling
    /// can route the same physical d-pad press into moving the text
    /// cursor, and letting our own navigation *also* act on it at the
    /// same time fought over focus.
    rename_editing: bool,
    /// One-shot: focus the name field (whichever form — badge or real
    /// `TextEdit` — actually renders) the *next* time `show_rename_editor`
    /// runs. Deliberately separate from `focus_first_action` — see
    /// `show_rename_editor`'s local `focus_pending` doc comment for why.
    rename_focus_pending: bool,
    search_query: String,
    search_open: bool,
    /// One-shot flags consumed the next time the relevant widget is drawn.
    focus_search: bool,
    focus_first_action: bool,
    /// The hamburger button's id — last-resort fallback focus target when
    /// nothing else is available.
    top_focus_id: egui::Id,
    /// Last frame's pane rects, used to classify the currently-focused
    /// widget into a pane (one-frame-stale, imperceptible in practice).
    sidebar_rect: Option<egui::Rect>,
    central_rect: Option<egui::Rect>,
    right_rect: Option<egui::Rect>,
    /// The Actions/Permissions/Progress strip's rect — a separate bottom
    /// bar (not part of `right_rect`/Preview), but still classified as
    /// `Pane::Right` for focus/highlight purposes.
    actions_strip_rect: Option<egui::Rect>,
    /// The strip's actual content height, measured from last frame's
    /// `ScrollAreaOutput::content_size` — used (clamped) to reserve
    /// *this* frame's `strip_height`, instead of a `ROW_HEIGHT * row
    /// count` guess. That guess was tuned to the tallest (icon) row, so
    /// once every badge became a uniform height (see `badge_min_height`)
    /// it *still* over-reserved — the true per-row height (badges plus
    /// `horizontal_wrapped`'s own spacing) was never going to match a
    /// guessed formula exactly. One frame of lag (the very first frame
    /// a new row count appears, before it's measured) is an acceptable
    /// trade for genuinely auto-fitting afterward.
    strip_measured_height: f32,
    toolbar_rect: Option<egui::Rect>,
    /// Ids of last frame's file-list rows paired with their path — used to
    /// look up which entry is focused when toggling multi-select, and to
    /// confine d-pad/stick movement to the Active pane.
    entry_ids: Vec<(egui::Id, PathBuf)>,
    /// Same idea as `entry_ids`, for the Trash view's rows.
    trash_ids: Vec<egui::Id>,
    /// Ids of last frame's sidebar rows (places, then mounts, then Trash),
    /// in visual top-to-bottom order — confines d-pad/stick movement to
    /// the Sidebar pane.
    sidebar_ids: Vec<egui::Id>,
    /// Ids of last frame's toolbar buttons, in visual left-to-right order —
    /// confines d-pad/stick movement to the Toolbar pane.
    toolbar_ids: Vec<egui::Id>,
    /// Last frame's Actions/Permissions strip widgets (badges, checkboxes),
    /// one inner `Vec` per row (Actions row, then Permissions row, in that
    /// order — whichever are actually open) in left-to-right order. Lets
    /// d-pad/stick movement step through them by index instead of relying
    /// on egui's geometric `move_focus`, which has no concept of the
    /// strip's boundary and can walk focus into the file list behind it:
    /// Left/Right step within the current row, Up/Down switch rows while
    /// keeping roughly the same column (see `move_focus_confined`'s
    /// guarded branch).
    right_focus_rows: Vec<Vec<egui::Id>>,
    /// The last focused widget id seen in each pane, remembered so
    /// switching panes (LB/RB) restores the cursor to "where you left off"
    /// instead of landing nowhere.
    sidebar_last_focus: Option<egui::Id>,
    active_last_focus: Option<egui::Id>,
    toolbar_last_focus: Option<egui::Id>,
    right_last_focus: Option<egui::Id>,
    /// Set alongside every programmatic `request_focus` so the newly
    /// focused row can scroll itself into view the next time it's drawn
    /// (egui doesn't do this on its own) — consumed and cleared by
    /// whichever list draws a row matching this id.
    scroll_to_focus: Option<egui::Id>,
    /// The right stick's per-frame scroll delta (points) for the Right
    /// pane only (Preview/Actions — no "selection" concept there), reset
    /// to `None` every frame and applied via `Ui::scroll_with_delta` — see
    /// NOTES.md "gamepad scrolling needs `scroll_with_delta`, not a fake
    /// pointer hover" for why a synthetic `MouseWheel` event alone doesn't
    /// work. Sidebar/Active use `scroll_move_last` instead — see NOTES.md
    /// "right-stick scroll now moves the selection, not a free-floating
    /// view offset".
    pending_scroll: Option<f32>,
    /// Last time right-stick tilt moved the selection by a row in the
    /// Sidebar/Active panes — throttles `Action::Scroll` (which fires
    /// every frame the stick is tilted) to `SCROLL_MOVE_INTERVAL`.
    scroll_move_last: Option<std::time::Instant>,
    /// Whether the in-folder search also walks subfolders, not just the
    /// current directory.
    recursive_search: bool,
    /// Cached results of the last recursive walk — recomputed only when
    /// the query/toggle actually changes (see `recursive_dirty`), since
    /// the walk itself is synchronous and can be slow on a large tree.
    recursive_results: Vec<Entry>,
    /// True from the moment the query/toggle changes until the walk that
    /// answers it has run. Consumed over two frames on purpose — see
    /// NOTES.md "recursive search runs synchronously, not threaded" — so
    /// a "Searching…" spinner gets a real frame on screen before the
    /// (blocking) walk happens, instead of the UI just freezing with no
    /// feedback.
    recursive_dirty: bool,
    /// Set on the first frame `recursive_dirty` is seen — the walk runs
    /// on the *next* frame after this is already true, not immediately.
    recursive_spinner_shown: bool,
    /// The IProLaunch CLI binary, if `~/.config/iprolaunch/bin-path` exists
    /// — detected once at startup, gates the "Add to IProLaunch" action.
    iprolaunch_bin: Option<PathBuf>,
    /// Cached result of the last `iprolaunch::is_registered` check, keyed
    /// by path — `library search` shells out to the CLI, so this is only
    /// recomputed when the selection changes (see `iprolaunch_registered`),
    /// not every frame the Actions badge is drawn. Set directly (bypassing
    /// a re-check) right after a successful `add`.
    iprolaunch_status: Option<(PathBuf, bool)>,
    multi_select: bool,
    multi_selected: std::collections::HashSet<PathBuf>,
    /// Cached `fileinfo::compute` result for the sidebar's info card, keyed
    /// by path — a directory's size is a bounded but potentially slow
    /// recursive walk, so this is only recomputed when the selection
    /// actually changes, not every frame the card is drawn.
    selected_info: Option<(PathBuf, fileinfo::FileInfo)>,
    /// The last pane `focused_pane()` was able to classify — "sticky"
    /// across frames where it returns `None`. `focused_pane()` checks
    /// whether the *focused widget's* on-screen rect falls inside a pane's
    /// bounds, but scrolling moves that rect outside those bounds (a row
    /// scrolled off-screen still has a real rect there), which made
    /// `apply_pending_scroll` stop applying scroll input once the focused
    /// row scrolled out of view. Updated only when a live classification
    /// succeeds; every "what pane are we in" decision reads this instead
    /// of calling `focused_pane()` directly.
    current_pane: Option<Pane>,
    /// True while the Start-held-3s Quit confirmation is showing — a
    /// strip zone (`FooterZone::Quit`) like Rename/Permissions, not a
    /// popup (see NOTES.md "quit confirmation modal broke gamepad
    /// input" for why a real `egui::Modal` isn't used here).
    quit_confirm: bool,
    /// Whether the title header above the toolbar shows — from
    /// `config.toml`'s `show_header`, fixed for the life of the app (no
    /// in-app toggle, unlike `show_hidden_files`/`show_all_mounts`).
    show_header: bool,
}

/// How long a finished copy/move job stays visible before auto-closing.
const JOB_LINGER: Duration = Duration::from_secs(2);

impl BrowDeckApp {
    pub fn new(cc: &eframe::CreationContext<'_>, show_header: bool) -> Self {
        egui_material_icons::initialize(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        fonts::install_cjk_fallback(&cc.egui_ctx);
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/"));
        let mut app = Self {
            sidebar_open: true,
            current_dir: home,
            entries: Vec::new(),
            view: View::Dir,
            trash_entries: Vec::new(),
            selected_trash: None,
            places: places::list_places(),
            mounts: mounts::list_mounts(),
            selected: None,
            clipboard: None,
            jobs: Vec::new(),
            gamepad: gamepad::GamepadInput::new(),
            icon_scale: 1.0,
            show_all_mounts: false,
            show_hidden_files: false,
            sort_by: SortBy::Name,
            sort_ascending: true,
            preview_enabled: false,
            preview_wide: false,
            preview_text: None,
            context_menu_open: false,
            perm_editor: None,
            rename_editor: None,
            rename_editing: false,
            rename_focus_pending: false,
            search_query: String::new(),
            search_open: false,
            focus_search: false,
            focus_first_action: false,
            top_focus_id: egui::Id::new("toolbar_hamburger"),
            sidebar_rect: None,
            central_rect: None,
            right_rect: None,
            actions_strip_rect: None,
            // A reasonable single-row starting guess for the very first
            // time any zone opens, before there's a real measurement to
            // use yet — see the field's own doc comment.
            strip_measured_height: 36.0,
            toolbar_rect: None,
            entry_ids: Vec::new(),
            trash_ids: Vec::new(),
            sidebar_ids: Vec::new(),
            toolbar_ids: Vec::new(),
            right_focus_rows: Vec::new(),
            sidebar_last_focus: None,
            active_last_focus: None,
            toolbar_last_focus: None,
            right_last_focus: None,
            scroll_to_focus: None,
            pending_scroll: None,
            scroll_move_last: None,
            recursive_search: false,
            recursive_results: Vec::new(),
            recursive_dirty: false,
            recursive_spinner_shown: false,
            iprolaunch_bin: iprolaunch::detect_bin(),
            iprolaunch_status: None,
            selected_info: None,
            multi_select: false,
            multi_selected: std::collections::HashSet::new(),
            current_pane: None,
            quit_confirm: false,
            show_header,
        };
        app.refresh();
        app
    }

    /// Renders a [`MaterialIcon`] at the current icon scale — use this
    /// instead of a bare `ICON_*` constant everywhere in the UI so the
    /// zoom +/- controls affect every icon consistently.
    /// Fixed-size icon, ignoring `icon_scale` — the toolbar, Preview, and
    /// Actions/Permissions/Rename/Progress strip all use this so LT/RT
    /// zoom doesn't change their size; see `icon_scaled` for the handful
    /// of places that *should* zoom.
    fn icon(&self, icon: MaterialIcon) -> egui::RichText {
        icon.rich_text().size(BASE_ICON_SIZE)
    }

    /// Same as `icon`, but scales with `icon_scale` (LT/RT) — reserved
    /// for the sidebar (Places/Mounts/Trash) and the main pane (file
    /// list, trash view, in-folder search), the only things the user
    /// wants LT/RT to actually zoom.
    fn icon_scaled(&self, icon: MaterialIcon) -> egui::RichText {
        icon.rich_text().size(BASE_ICON_SIZE * self.icon_scale)
    }

    fn scale_down(&mut self) {
        self.icon_scale = (self.icon_scale - ICON_SCALE_STEP)
            .clamp(*ICON_SCALE_RANGE.start(), *ICON_SCALE_RANGE.end());
    }

    fn scale_up(&mut self) {
        self.icon_scale = (self.icon_scale + ICON_SCALE_STEP)
            .clamp(*ICON_SCALE_RANGE.start(), *ICON_SCALE_RANGE.end());
    }

    /// Changes the selected file/folder and, if the preview toggle is on,
    /// loads a preview for it — the single place selection changes so the
    /// two always stay in sync.
    fn set_selected(&mut self, path: Option<PathBuf>) {
        self.selected = path;
        self.refresh_preview();
    }

    fn refresh_preview(&mut self) {
        self.preview_text = None;
        if !self.preview_enabled {
            return;
        }
        let Some(path) = self.selected.clone() else {
            return;
        };
        if preview::classify(&path) == Some(preview::Kind::Text) {
            let text = preview::read_text_preview(&path)
                .unwrap_or_else(|e| format!("(couldn't read: {e})"));
            self.preview_text = Some((path, text));
        }
    }

    fn navigate_to(&mut self, dir: PathBuf) {
        self.current_dir = dir;
        self.view = View::Dir;
        self.search_query.clear();
        self.recursive_results.clear();
        self.recursive_dirty = false;
        self.recursive_spinner_shown = false;
        self.set_selected(None);
        self.refresh();
    }

    fn refresh(&mut self) {
        let mut entries: Vec<Entry> = std::fs::read_dir(&self.current_dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| {
                let path = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                // `path.is_dir()` follows symlinks (unlike `file_type()`,
                // which reports a symlink-to-directory as not-a-directory).
                let is_dir = path.is_dir();
                let meta = std::fs::metadata(&path).ok();
                let size = if is_dir {
                    0
                } else {
                    meta.as_ref().map(|m| m.len()).unwrap_or(0)
                };
                let modified = meta.and_then(|m| m.modified().ok());
                Entry {
                    name,
                    path,
                    is_dir,
                    size,
                    modified,
                }
            })
            .collect();
        self.sort_entries(&mut entries);
        self.entries = entries;
    }

    /// Called after `sort_by`/`sort_ascending` changes — `refresh()` is
    /// the only place `self.entries` actually gets re-sorted, and a live
    /// recursive search has its own separate result list that needs the
    /// same nudge to re-walk-and-resort.
    fn resort_current_view(&mut self) {
        self.refresh();
        if self.recursive_search && self.search_open && !self.search_query.is_empty() {
            self.recursive_dirty = true;
            self.recursive_spinner_shown = false;
        }
    }

    /// Dirs always sort first, regardless of `sort_by`/`sort_ascending` —
    /// only the order *within* dirs and *within* files follows those.
    fn sort_entries(&self, entries: &mut [Entry]) {
        entries.sort_by(|a, b| compare_entries(a, b, self.sort_by, self.sort_ascending));
    }

    /// Walks `current_dir` recursively looking for entries (at any depth)
    /// whose name contains `query`. Runs synchronously on the UI thread —
    /// see NOTES.md "recursive search runs synchronously, not threaded" —
    /// so both how much it scans and how many results it keeps are capped
    /// to bound the worst case on a huge tree (`~/.cache`, node_modules,
    /// etc.). Never follows symlinked directories, to avoid an infinite
    /// loop from one that points back at an ancestor.
    fn recursive_search(&self, query: &str) -> Vec<Entry> {
        const MAX_SCANNED: usize = 20_000;
        const MAX_RESULTS: usize = 500;

        let mut results = Vec::new();
        let mut scanned = 0usize;
        let mut stack = vec![self.current_dir.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(read_dir) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read_dir.flatten() {
                if scanned >= MAX_SCANNED || results.len() >= MAX_RESULTS {
                    return results;
                }
                scanned += 1;
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let is_dir = path.is_dir();
                if name.to_lowercase().contains(query) {
                    let display = path
                        .strip_prefix(&self.current_dir)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned();
                    let meta = entry.metadata().ok();
                    let size = if is_dir {
                        0
                    } else {
                        meta.as_ref().map(|m| m.len()).unwrap_or(0)
                    };
                    let modified = meta.and_then(|m| m.modified().ok());
                    results.push(Entry {
                        name: display,
                        path: path.clone(),
                        is_dir,
                        size,
                        modified,
                    });
                }
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    stack.push(path);
                }
            }
        }
        self.sort_entries(&mut results);
        results
    }

    fn open_trash(&mut self) {
        self.view = View::Trash;
        self.selected_trash = None;
        self.set_selected(None);
        self.trash_entries = deleted::list_trash();
    }

    /// Mouse "Up" toolbar button: up a directory, or back out of the trash
    /// view. The gamepad's L3/East buttons use the more specific
    /// [`Self::up_directory`]/[`Self::escape`] instead.
    fn go_back(&mut self) {
        match self.view {
            View::Trash => {
                self.view = View::Dir;
                self.set_selected(None);
            }
            View::Dir => {
                if let Some(parent) = self.current_dir.parent() {
                    self.navigate_to(parent.to_path_buf());
                }
            }
        }
    }

    /// Goes up a directory (used directly by the toolbar's Up button, and
    /// as gamepad B's fallback when there's nothing to back out of).
    fn up_directory(&mut self) {
        if matches!(self.view, View::Dir)
            && let Some(parent) = self.current_dir.parent()
        {
            self.navigate_to(parent.to_path_buf());
        }
    }

    /// Gamepad East/B — cancels multi-select, or closes whichever menu/
    /// dialog is open, in priority order; otherwise backs out of the trash
    /// view. Returns whether it closed something, so [`Self::back_or_up`]
    /// knows whether to fall back to going up a directory instead.
    fn escape(&mut self) -> bool {
        if self.quit_confirm {
            // Highest priority — it's the most recent thing to have
            // opened regardless of whatever else is open underneath.
            self.quit_confirm = false;
            // If something else was open underneath (rendering was just
            // suppressed for the frame, not closed — see `active_zones`),
            // its focus went stale while the Quit row had it instead;
            // re-prime so it lands somewhere real again next frame,
            // same as every other zone-closes-but-another-stays-open
            // transition below.
            if self.context_menu_open || self.rename_editor.is_some() || self.perm_editor.is_some()
            {
                self.focus_first_action = true;
            }
        } else if self.multi_select {
            self.multi_select = false;
            self.multi_selected.clear();
        } else if self.rename_editor.is_some() {
            // Same reasoning as the perm_editor branch below: Rename/New
            // Folder also opens *from* Actions, so it's the newer of the
            // two if both somehow ended up open. (Only reachable while
            // not mid-edit — B during real editing is intercepted earlier,
            // in the gamepad dispatch loop, and only exits edit mode.)
            self.rename_editor = None;
            self.rename_editing = false;
            if self.context_menu_open {
                self.focus_first_action = true;
            }
        } else if self.perm_editor.is_some() {
            // Permissions opens *from* Actions, never the other way
            // around, so it's always the more-recently-opened of the two
            // — B closes it first, leaving Actions. Re-priming
            // `focus_first_action` below (when Actions stays open) avoids
            // leaving focus on a now-gone widget, which otherwise read as
            // the remaining strip going dim/inactive.
            self.perm_editor = None;
            if self.context_menu_open {
                self.focus_first_action = true;
            }
        } else if self.context_menu_open {
            self.context_menu_open = false;
        } else if matches!(self.view, View::Trash) {
            self.view = View::Dir;
            self.set_selected(None);
        } else {
            return false;
        }
        true
    }

    /// Gamepad B — combines the old East/B (close menus) and L3
    /// (up-directory) behaviors into one button: close whatever's open, or
    /// if nothing was, go up a directory instead. L3 was freed up for
    /// multi-select toggling.
    fn back_or_up(&mut self) {
        if !self.escape() {
            self.up_directory();
        }
    }

    /// egui's directional focus movement (`move_focus`) only works
    /// *relative to* an already-focused widget — with nothing focused
    /// (fresh launch, or the previously-focused widget disappeared) it's a
    /// silent no-op. Called right before a directional gamepad action, so
    /// it only intervenes when gamepad nav is actually about to be used —
    /// never on an idle frame, where it would otherwise keep fighting a
    /// mouse-driven selection that never set keyboard focus to begin with.
    fn ensure_focus_anchor(&mut self, ctx: &egui::Context) {
        if ctx.memory(|m| m.focused()).is_none() {
            // Prefer whatever's already selected (typically via mouse,
            // which doesn't grant egui keyboard focus on its own — see
            // NOTES.md) so a first d-pad/stick nudge continues on from
            // there instead of jumping to the top of the pane, which read
            // as the selection itself moving.
            let active_ids = self.pane_ids(Pane::Active);
            let fallback = self
                .selected
                .as_ref()
                .and_then(|sel| self.entry_ids.iter().find(|(_, p)| p == sel))
                .map(|(id, _)| *id)
                .or_else(|| active_ids.first().copied())
                .unwrap_or(self.top_focus_id);
            self.set_focus(ctx, fallback);
        }
    }

    /// Gives a widget keyboard focus *and* marks it to be scrolled into
    /// view the next time it's drawn — every programmatic focus change
    /// should go through this, not `ctx.memory_mut(|m| m.request_focus(..))`
    /// directly, or the newly focused row can end up off-screen with no
    /// way to tell (see NOTES.md, "programmatic focus needs to scroll
    /// itself into view too").
    fn set_focus(&mut self, ctx: &egui::Context, id: egui::Id) {
        ctx.memory_mut(|m| m.request_focus(id));
        self.scroll_to_focus = Some(id);
    }

    /// The ids of the currently-navigable widgets in a pane, in visual
    /// order — the set d-pad/stick movement is confined to while that pane
    /// has focus, and what LB/RB land on when switching into it.
    /// Only meaningful for Sidebar/Active/Toolbar — see [`Self::move_focus_confined`]
    /// for why Right isn't included.
    fn pane_ids(&self, pane: Pane) -> Vec<egui::Id> {
        match pane {
            Pane::Sidebar => self.sidebar_ids.clone(),
            Pane::Active => match self.view {
                View::Dir => self.entry_ids.iter().map(|(id, _)| *id).collect(),
                View::Trash => self.trash_ids.clone(),
            },
            Pane::Toolbar => self.toolbar_ids.clone(),
            Pane::Right => Vec::new(),
        }
    }

    /// Whether focus is currently on a widget inside `actions_strip_rect`.
    fn focus_inside_strip(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.focused())
            .and_then(|id| ctx.read_response(id))
            .is_some_and(|r| {
                self.actions_strip_rect
                    .is_some_and(|strip| strip.contains(r.rect.center()))
            })
    }

    /// Called unconditionally at the top of every frame to catch focus
    /// starting outside the strip while Actions and/or Permissions is
    /// open — e.g. the frame the strip first opens, before
    /// `focus_first_action`'s `request_focus` has taken effect, or if
    /// `ensure_focus_anchor` re-seeded focus into the file list because
    /// nothing was focused that frame. Once focus is inside,
    /// `move_focus_confined`'s guarded branch keeps it there by
    /// index-stepping through `right_focus_rows`, which can't ever
    /// produce an out-of-strip id — nothing left for this to correct in
    /// the steady state.
    fn enforce_strip_focus_guard(&mut self, ctx: &egui::Context) {
        let guard_active = self.context_menu_open
            || self.perm_editor.is_some()
            || self.rename_editor.is_some()
            || self.quit_confirm;
        if guard_active
            && !self.focus_inside_strip(ctx)
            && let Some(id) = self.right_last_focus
        {
            self.set_focus(ctx, id);
        }
    }

    /// Moves focus by one step within whichever pane currently has it,
    /// never letting a direction press leave that pane (that's LB/RB's
    /// job now, see [`Self::switch_pane`]). Sidebar/Active are vertical
    /// lists (Up/Down move; Left/Right are no-ops); Toolbar is a
    /// horizontal row (the reverse). The Actions/Permissions strip
    /// (`Pane::Right`) index-steps through `right_focus_rows`; Preview/
    /// Progress (also `Pane::Right`, when the strip isn't open) have no
    /// navigable content at all, so the d-pad is just a no-op there.
    fn move_focus_confined(&mut self, ctx: &egui::Context, dir: egui::FocusDirection) {
        // While Actions and/or Permissions is open, step through
        // `right_focus_rows` by index instead of egui's geometric
        // `move_focus` — geometric search has no concept of the strip's
        // boundary and can walk focus off the end of it (e.g. pressing
        // Left from the leftmost badge), and `move_focus` only *queues* a
        // direction resolved later in `end_pass()`, so there's nothing to
        // synchronously validate or revert if it does. Index-stepping
        // through a known list can't ever produce an out-of-strip id.
        // Left/Right step within the current row; Up/Down switch rows
        // (Actions <-> Permissions) while keeping roughly the same column.
        let guard_active = self.context_menu_open
            || self.perm_editor.is_some()
            || self.rename_editor.is_some()
            || self.quit_confirm;
        if guard_active {
            let inside = self.focus_inside_strip(ctx);
            if inside && !self.right_focus_rows.is_empty() {
                let current = ctx.memory(|m| m.focused());
                let pos = current.and_then(|id| {
                    self.right_focus_rows
                        .iter()
                        .enumerate()
                        .find_map(|(r, row)| row.iter().position(|i| *i == id).map(|c| (r, c)))
                });
                let next_id = match dir {
                    egui::FocusDirection::Left | egui::FocusDirection::Right => {
                        let delta: isize = if dir == egui::FocusDirection::Right {
                            1
                        } else {
                            -1
                        };
                        match pos {
                            Some((r, c)) => {
                                let row = &self.right_focus_rows[r];
                                let next_c = (c as isize + delta)
                                    .clamp(0, row.len().saturating_sub(1) as isize)
                                    as usize;
                                row.get(next_c).copied()
                            }
                            None => self
                                .right_focus_rows
                                .first()
                                .and_then(|row| row.first())
                                .copied(),
                        }
                    }
                    egui::FocusDirection::Up | egui::FocusDirection::Down => {
                        let delta: isize = if dir == egui::FocusDirection::Down {
                            1
                        } else {
                            -1
                        };
                        match pos {
                            Some((r, c)) => {
                                let next_r = (r as isize + delta).clamp(
                                    0,
                                    self.right_focus_rows.len().saturating_sub(1) as isize,
                                ) as usize;
                                let target_row = &self.right_focus_rows[next_r];
                                let next_c = c.min(target_row.len().saturating_sub(1));
                                target_row.get(next_c).copied()
                            }
                            None => self
                                .right_focus_rows
                                .first()
                                .and_then(|row| row.first())
                                .copied(),
                        }
                    }
                    _ => None,
                };
                if let Some(id) = next_id {
                    self.set_focus(ctx, id);
                }
            }
            return;
        }
        let Some(pane) = self.current_pane else {
            // No pane classified yet (e.g. focus landed somewhere odd) —
            // fall back to egui's own geometric search rather than doing
            // nothing.
            ctx.memory_mut(|m| m.move_focus(dir));
            return;
        };
        if pane == Pane::Right {
            // Reached here means the strip isn't open (that's the guarded
            // branch above) — so this is either Preview or a Progress-
            // only display, neither of which has real navigable content
            // for the d-pad (only the right stick scrolls Preview).
            // Deliberately a no-op rather than falling back to egui's
            // geometric `move_focus`: that has no concept of the pane
            // boundary and can walk focus straight into the file list —
            // the same class of leak fixed for the Actions/Permissions
            // strip, just via a different mechanism since there's no
            // list of ids to index-step through here.
            return;
        }
        let delta: isize = match (pane, dir) {
            (Pane::Toolbar, egui::FocusDirection::Right) => 1,
            (Pane::Toolbar, egui::FocusDirection::Left) => -1,
            (Pane::Toolbar, _) => 0,
            (_, egui::FocusDirection::Down) => 1,
            (_, egui::FocusDirection::Up) => -1,
            (_, _) => 0,
        };
        if delta == 0 {
            return;
        }
        let ids = self.pane_ids(pane);
        let current = ctx.memory(|m| m.focused());
        let idx = current.and_then(|id| ids.iter().position(|i| *i == id));
        let next = match idx {
            Some(i) => (i as isize + delta).clamp(0, ids.len().saturating_sub(1) as isize) as usize,
            None => 0,
        };
        if let Some(id) = ids.get(next).copied() {
            self.set_focus(ctx, id);
        }
    }

    /// Whether the Preview pane is currently showing (enabled, and the
    /// selection is a previewable image/text file).
    fn preview_visible(&self) -> bool {
        self.preview_enabled
            && self
                .selected
                .as_deref()
                .and_then(preview::classify)
                .is_some()
    }

    /// The panes LB/RB cycle between, in order. Right/Preview is included
    /// only while it's actually showing; the Actions/Permissions strip
    /// (also `Pane::Right`) stays reachable only via its own explicit
    /// triggers (X, R3, Permissions), never LB/RB — moot anyway, since
    /// `switch_pane` fully disables LB/RB while it's open.
    fn pane_cycle(&self) -> Vec<Pane> {
        let mut cycle = Vec::new();
        if self.sidebar_open {
            cycle.push(Pane::Sidebar);
        }
        cycle.push(Pane::Active);
        if self.preview_visible() {
            cycle.push(Pane::Right);
        }
        cycle.push(Pane::Toolbar);
        cycle
    }

    /// LB/RB — move to the next/previous pane in [`Self::pane_cycle`] and
    /// restore its remembered cursor position (see [`Self::focus_pane`]).
    fn switch_pane(&mut self, ctx: &egui::Context, delta: isize) {
        // LB/RB is fully disabled — not just restricted — while Actions,
        // Permissions, and/or Rename/New Folder is open. Progress alone
        // doesn't count: it's a plain display, nothing to interact with,
        // so no reason to trap focus there.
        if self.context_menu_open
            || self.perm_editor.is_some()
            || self.rename_editor.is_some()
            || self.quit_confirm
        {
            return;
        }
        let cycle = self.pane_cycle();
        if cycle.is_empty() {
            return;
        }
        let current = self.current_pane;
        let idx = current.and_then(|p| cycle.iter().position(|c| *c == p));
        let next = match idx {
            Some(i) => (i as isize + delta).rem_euclid(cycle.len() as isize) as usize,
            None => 0,
        };
        self.focus_pane(ctx, cycle[next]);
    }

    /// Gives keyboard focus to a pane, preferring its last-remembered
    /// widget (see the `*_last_focus` fields) so the cursor lands "where
    /// you left off" instead of always snapping to the first row — this is
    /// what fixes focus disappearing when switching to the sidebar/mounts.
    fn focus_pane(&mut self, ctx: &egui::Context, pane: Pane) {
        let remembered = match pane {
            Pane::Sidebar => self.sidebar_last_focus,
            Pane::Active => self.active_last_focus,
            Pane::Toolbar => self.toolbar_last_focus,
            // `right_last_focus` is shared with the Actions/Permissions
            // strip (also `Pane::Right`) — LB/RB can only ever reach
            // Right while that strip is closed (switch_pane blocks LB/RB
            // entirely while it's open), so if `right_last_focus` still
            // points at a strip widget, that widget is *always* gone by
            // now and `read_response` will confirm it. Falling back to
            // Preview's own focus anchor in that case (or the first time
            // ever, when nothing's been focused there yet) — same class
            // of stale-focus bug as the escape()-priority fix, just a
            // different path to the same dangling id.
            Pane::Right => {
                let still_valid = self
                    .right_last_focus
                    .is_some_and(|id| ctx.read_response(id).is_some());
                if still_valid {
                    self.right_last_focus
                } else {
                    Some(Self::preview_focus_id())
                }
            }
        };
        // Right has no tracked id list (see `pane_ids`), so there's
        // nothing to validate `remembered` against — just trust it
        // directly; it's kept up to date every frame regardless of
        // `pane_ids`, via the same bookkeeping the other panes use.
        let target = if pane == Pane::Right {
            remembered
        } else {
            let ids = self.pane_ids(pane);
            remembered
                .filter(|id| ids.contains(id))
                .or_else(|| ids.first().copied())
        };
        if let Some(id) = target {
            self.set_focus(ctx, id);
        }
    }

    /// The paths an action should apply to: the multi-selection if that
    /// mode is active and non-empty, otherwise the single selection.
    fn selected_paths(&self) -> Vec<PathBuf> {
        if self.multi_select && !self.multi_selected.is_empty() {
            self.multi_selected.iter().cloned().collect()
        } else {
            self.selected.iter().cloned().collect()
        }
    }

    /// Gamepad A while multi-select is active — toggles whichever file-list
    /// entry currently has focus, looked up via last frame's `entry_ids`.
    fn toggle_focused_entry(&mut self, ctx: &egui::Context) {
        let Some(id) = ctx.memory(|m| m.focused()) else {
            return;
        };
        let Some(path) = self
            .entry_ids
            .iter()
            .find(|(eid, _)| *eid == id)
            .map(|(_, p)| p.clone())
        else {
            return;
        };
        if !self.multi_selected.remove(&path) {
            self.multi_selected.insert(path);
        }
    }

    /// Gamepad X — selects whichever file-list entry currently has focus,
    /// *without* navigating into it even if it's a folder (unlike A/
    /// Activate, which always enters a focused folder). This is how a
    /// folder gets selected for Copy/Cut/Delete/Permissions via gamepad
    /// alone — mirrors what a mouse right-click already does for any
    /// entry (see the `secondary_clicked()` handling in `show_dir`).
    fn select_focused_entry(&mut self, ctx: &egui::Context) {
        let Some(id) = ctx.memory(|m| m.focused()) else {
            return;
        };
        let Some(path) = self
            .entry_ids
            .iter()
            .find(|(eid, _)| *eid == id)
            .map(|(_, p)| p.clone())
        else {
            return;
        };
        self.set_selected(Some(path));
    }

    /// Whether a real gamepad is connected — gates the bottom legend bar.
    fn has_connected_pad(&self) -> bool {
        self.gamepad.as_ref().is_some_and(|g| g.has_connected_pad())
    }

    /// Whether `pane` should render un-dimmed — always true without a
    /// connected gamepad (mouse clicks don't grant egui focus, so "dim
    /// whatever isn't focused" would otherwise leave the whole app dimmed
    /// for mouse-only use).
    fn pane_undimmed(&self, focused_pane: Option<Pane>, pane: Pane) -> bool {
        !self.has_connected_pad() || focused_pane == Some(pane)
    }

    /// Applies this frame's pending right-stick scroll (if any) to
    /// whichever `ScrollArea` is currently drawing content for `pane`, if
    /// that's the focused one. Must be called from *inside* the target
    /// `ScrollArea`'s own closure — that's what `Ui::scroll_with_delta`
    /// scrolls.
    fn apply_pending_scroll(&self, ui: &egui::Ui, pane: Pane) {
        if self.current_pane == Some(pane)
            && let Some(delta) = self.pending_scroll
        {
            ui.scroll_with_delta(egui::vec2(0.0, delta));
        }
    }

    /// Colored circular badge for a face button (A/B/X/Y), matching
    /// standard controller button colors.
    /// Diameter of the A/B/X/Y face-button badges.
    const FACE_BADGE_SIZE: f32 = 20.0;

    fn face_badge(ui: &mut egui::Ui, letter: &str, color: egui::Color32) {
        // A `Frame` with margins guessed to make a single glyph's box
        // roughly square (an earlier attempt) still came out visibly
        // wider than tall — text width doesn't reliably match line
        // height, so "roughly square corners rounded a lot" never
        // actually reads as a true circle. Painting a real
        // `circle_filled` at a fixed diameter, independent of the
        // letter's own size, is exact regardless of which letter.
        let size = egui::vec2(Self::FACE_BADGE_SIZE, Self::FACE_BADGE_SIZE);
        let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), Self::FACE_BADGE_SIZE / 2.0, color);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            letter,
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }

    /// Gray rounded badge for a non-face-button control (LB/RB/LT/RT/L3/R3).
    fn key_badge(ui: &mut egui::Ui, label: &str) {
        egui::Frame::default()
            .fill(egui::Color32::from_gray(70))
            .corner_radius(4.0)
            .inner_margin(egui::Margin {
                left: 5,
                right: 5,
                top: 1,
                bottom: 1,
            })
            .show(ui, |ui| {
                ui.label(egui::RichText::new(label).strong().size(11.0));
            });
    }

    /// A badge with an icon (e.g. `self.icon(ICON_CONTENT_COPY)`) naturally
    /// renders taller than one that's just text (e.g. "Cancel") — the
    /// icon glyph's own line height exceeds the body text's. Passed as
    /// `min_height` to `action_badge`/`action_badge_disabled` so every
    /// badge in a row — icon or not — ends up the same height instead of
    /// the row looking visibly uneven (reported by the user, comparing
    /// the Actions row against Rename/Permissions' text-only badges).
    /// Not `icon_scale`-aware — the strip's own icons are now fixed-size
    /// (`self.icon`, not `self.icon_scaled`) per the user's ask that
    /// LT/RT zoom not affect it, so this stays fixed too.
    fn badge_min_height(&self) -> f32 {
        BASE_ICON_SIZE + 8.0
    }

    /// A compact, clickable button styled like the bottom bar's badges
    /// (small, rounded, gray) instead of a full-size default `Button` —
    /// used throughout the Actions/Permissions/Progress strip so it reads
    /// as one cohesive row instead of a heavier-looking separate widget.
    /// `min_height` (see `badge_min_height`) keeps icon and non-icon
    /// badges in the same row the same height.
    fn action_badge<'a>(
        min_height: f32,
        ui: &mut egui::Ui,
        content: impl egui::IntoAtoms<'a>,
    ) -> egui::Response {
        // Scoped so it doesn't bleed into anything drawn after this badge.
        // egui's `button_style()` computes `inner_margin` as
        // `button_padding + expansion - bg_stroke.width` — default
        // *inactive* stroke width is 0, but *hovered*/*active* (focus
        // counts as `active`) is 1.0, so a badge's own allocated size
        // shrinks/grows ~2px the instant it gains/loses focus, shifting
        // every badge after it in the row. Giving `inactive` a same-width
        // but transparent stroke keeps the reserved size constant across
        // states — only the visible border color changes.
        ui.scope(|ui| {
            let width = ui.visuals().widgets.hovered.bg_stroke.width;
            ui.visuals_mut().widgets.inactive.bg_stroke =
                egui::Stroke::new(width, egui::Color32::TRANSPARENT);
            ui.add(
                egui::Button::new(content)
                    .small()
                    .corner_radius(4.0)
                    .fill(egui::Color32::from_gray(70))
                    .min_size(egui::vec2(0.0, min_height)),
            )
        })
        .inner
    }

    /// Same look as `action_badge`, but disabled/non-interactive — used for
    /// an action that's already been done (e.g. IProLaunch's "already in
    /// the library" state), so it reads as a status indicator rather than
    /// a button that happens to do nothing.
    fn action_badge_disabled<'a>(
        min_height: f32,
        ui: &mut egui::Ui,
        content: impl egui::IntoAtoms<'a>,
    ) -> egui::Response {
        ui.add_enabled(
            false,
            egui::Button::new(content)
                .small()
                .corner_radius(4.0)
                .fill(egui::Color32::from_gray(70))
                .min_size(egui::vec2(0.0, min_height)),
        )
    }

    /// Whether `path` is already a registered IProLaunch library profile —
    /// cached per-path (`iprolaunch_status`) since checking shells out to
    /// the CLI (`iprolaunch::is_registered`); only actually re-runs it when
    /// `path` differs from whatever was last checked.
    fn iprolaunch_registered(&mut self, path: &Path) -> bool {
        if let Some((cached_path, registered)) = &self.iprolaunch_status
            && cached_path == path
        {
            return *registered;
        }
        let registered = self
            .iprolaunch_bin
            .as_deref()
            .is_some_and(|bin| iprolaunch::is_registered(bin, path));
        self.iprolaunch_status = Some((path.to_path_buf(), registered));
        registered
    }

    /// `fileinfo::compute(path)`, cached per-path — see `selected_info`'s
    /// field doc comment for why.
    fn selected_info(&mut self, path: &Path) -> &fileinfo::FileInfo {
        let needs_recompute = !matches!(&self.selected_info, Some((p, _)) if p == path);
        if needs_recompute {
            self.selected_info = Some((path.to_path_buf(), fileinfo::compute(path)));
        }
        &self.selected_info.as_ref().unwrap().1
    }

    /// The Start-held-3s Quit confirmation, rendered as a strip zone
    /// (like Rename/Permissions) rather than a popup — a real
    /// `egui::Modal` here (the previous approach) appeared to trigger
    /// Steam Input's own desktop/mouse-mode remapping on real Deck
    /// hardware, badly enough that gamepad input stayed broken until a
    /// hard restart. A plain strip row is just ordinary `Panel` content,
    /// like everything else here that's never had that problem. See
    /// NOTES.md.
    fn show_quit_zone(&mut self, ui: &mut egui::Ui) {
        // Captured *before* Cancel's click handler below can set it —
        // that sets it for the *next* frame's zone (Actions/Permissions,
        // once this one's gone), not this one, same reasoning as
        // `show_rename_editor`'s `focus_pending` local.
        let focus_pending_at_start = self.focus_first_action;
        let mut confirm = false;
        let mut cancel = false;
        let mut first_id = None;
        let mut row_ids: Vec<egui::Id> = Vec::new();
        ui.horizontal_wrapped(|ui| {
            let close = Self::action_badge(self.badge_min_height(), ui, self.icon(ICON_CLOSE));
            row_ids.push(close.id);
            if close.clicked() {
                cancel = true;
            }
            ui.strong("Quit BrowDeck?");
            ui.separator();
            let quit_btn = Self::action_badge(self.badge_min_height(), ui, "Quit");
            row_ids.push(quit_btn.id);
            if quit_btn.clicked() {
                confirm = true;
            }
            let cancel_btn = Self::action_badge(self.badge_min_height(), ui, "Cancel");
            // Defaults to Cancel, not Quit — unlike every other zone
            // here, which auto-focuses its primary/confirm action first
            // (e.g. Permissions' Apply). A stray Activate on this one
            // closes the whole app, so the safe default matters more
            // than the usual convenience.
            first_id.get_or_insert(cancel_btn.id);
            row_ids.push(cancel_btn.id);
            if cancel_btn.clicked() {
                cancel = true;
            }
        });
        self.right_focus_rows.push(row_ids);
        if confirm {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else if cancel {
            self.quit_confirm = false;
            // Same re-priming as the `escape()` path for this — see its
            // comment.
            if self.context_menu_open || self.rename_editor.is_some() || self.perm_editor.is_some()
            {
                self.focus_first_action = true;
            }
        }
        if focus_pending_at_start && let Some(id) = first_id {
            ui.ctx().memory_mut(|m| m.request_focus(id));
            self.focus_first_action = false;
        }
    }

    /// Persistent bottom bar: an always-visible gamepad button legend
    /// (only while a real gamepad is connected) plus the app version,
    /// rather than scattering button hints next to individual controls.
    /// Fixed width reserved for the version label on the right — sized
    /// generously for "v99.99.99", not measured, since it's just a small
    /// tuck-in-the-corner label, not something that needs pixel-exact
    /// fitting.
    const STATUS_BAR_VERSION_WIDTH: f32 = 60.0;

    fn show_status_bar(&self, ui: &mut egui::Ui) {
        // Two independent child `Ui`s over explicit, non-overlapping
        // rects (the pattern used everywhere else in this app for a
        // fixed-size region — see e.g. the Actions strip) rather than
        // sibling `ui.with_layout` calls sharing one `Ui`'s cursor:
        // mixing a `right_to_left` and a `left_to_right` layout as
        // siblings of the same `Ui` left the second one with nothing to
        // show (its content never painted at all) — not investigated
        // further since this reservation approach is already proven
        // reliable elsewhere in the codebase.
        let full_rect = ui.available_rect_before_wrap();
        ui.allocate_rect(full_rect, egui::Sense::hover());
        let mut version_rect = full_rect;
        version_rect.set_left(full_rect.right() - Self::STATUS_BAR_VERSION_WIDTH);
        let mut legend_rect = full_rect;
        legend_rect.set_right(version_rect.left());

        let mut version_ui = ui.new_child(egui::UiBuilder::new().max_rect(version_rect));
        version_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(format!("v{}", env!("CARGO_PKG_VERSION")));
        });

        // `Layout::left_to_right` centers on the main axis by default —
        // see its own doc comment ("looks best to e.g. center text
        // within a button") — so no extra `with_main_align` is needed
        // for this to also center horizontally, not just vertically.
        let mut legend_ui = ui.new_child(egui::UiBuilder::new().max_rect(legend_rect));
        legend_ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            // Always shown, even with no gamepad connected — the legend
            // also documents what the mouse/keyboard equivalents are for
            // someone who hasn't plugged one in yet, not just a live
            // reference for one that's already connected.
            //
            // Deliberately not `self.icon(...)` — this bar has a fixed
            // height (`.exact_size` on its Panel) and must stay that way
            // regardless of the icon-scale setting, unlike the toolbar
            // (which has no fixed size and simply grows/shrinks with it).
            ui.label(ICON_GAMEPAD.rich_text().size(BASE_ICON_SIZE));
            ui.label("Move");
            ui.separator();
            Self::face_badge(ui, "A", egui::Color32::from_rgb(0x5A, 0xB4, 0x4B));
            ui.label("Open");
            Self::face_badge(ui, "B", egui::Color32::from_rgb(0xD1, 0x4A, 0x4A));
            ui.label("Back");
            Self::face_badge(ui, "X", egui::Color32::from_rgb(0x3D, 0x7E, 0xD6));
            ui.label("Actions");
            Self::face_badge(ui, "Y", egui::Color32::from_rgb(0xD9, 0xB4, 0x33));
            ui.label("Refresh (Search)");
            ui.separator();
            Self::key_badge(ui, "LB/RB");
            ui.label("Pane");
            Self::key_badge(ui, "LT/RT");
            ui.label("Zoom");
            Self::key_badge(ui, "L3");
            ui.label("Multi-select");
            Self::key_badge(ui, "R3/Start");
            ui.label("Open");
            Self::key_badge(ui, "Select");
            ui.label("Preview width");
            if self.multi_select {
                ui.separator();
                ui.colored_label(
                    egui::Color32::from_rgb(100, 150, 255),
                    format!("Multi-select: {} selected", self.multi_selected.len()),
                );
            }
        });
    }

    /// Gamepad R3 and the actions panel's "Open" button.
    fn open_selected(&mut self) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        if path.is_dir() {
            return;
        }
        // `.exe`/`.bat` go through IProLaunch directly (`<bin> run
        // <path>`) when it's available — a freshly-extracted `.exe`
        // usually doesn't have the exec bit set, so it wouldn't take the
        // direct-spawn path below, and `open::that()`/the desktop
        // portal's OpenURI can't run it either way (see below). Falls
        // through to the exec-bit/open::that() handling otherwise.
        if iprolaunch::is_launchable(&path)
            && let Some(bin) = self.iprolaunch_bin.clone()
        {
            if let Err(e) = iprolaunch::run(&bin, &path) {
                eprintln!("iprolaunch run failed: {e}");
            }
            return;
        }
        // Executable files (the exec bit set) are spawned directly
        // instead of through `open::that()`. On Linux that shells out to
        // `xdg-open`/the desktop portal's OpenURI, and modern portal
        // backends refuse to launch anything with the exec bit set as a
        // security measure — silently, from BrowDeck's perspective: the
        // portal client pops its own "not allowed in this context"
        // dialog rather than returning an error `open::that()` can see,
        // so this doesn't even show up as an `open failed` line. A
        // direct `spawn()` matches normal double-click-to-run semantics
        // and bypasses the portal entirely.
        use std::os::unix::fs::PermissionsExt;
        let is_executable =
            std::fs::metadata(&path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0);
        let result = if is_executable {
            std::process::Command::new(&path).spawn().map(|_| ())
        } else {
            open::that(&path)
        };
        if let Err(e) = result {
            eprintln!("open failed: {e}");
        }
    }

    fn delete_paths(&mut self, paths: &[PathBuf]) {
        for path in paths {
            if let Err(e) = trash::delete(path) {
                eprintln!("delete failed: {e}");
            }
        }
        self.multi_selected.clear();
        self.set_selected(None);
        self.refresh();
    }

    fn paste(&mut self) {
        let Some(clip) = self.clipboard.take() else {
            return;
        };
        let job = if clip.cut {
            fileops::spawn_move(clip.paths, self.current_dir.clone())
        } else {
            fileops::spawn_copy(clip.paths, self.current_dir.clone())
        };
        self.jobs.push(job);
    }

    /// Classifies the currently-focused widget into a pane, using last
    /// frame's pane rects — used to paint a focus-highlight border and to
    /// confine d-pad/stick movement and LB/RB pane-switching to it.
    fn focused_pane(&self, ctx: &egui::Context) -> Option<Pane> {
        let id = ctx.memory(|m| m.focused())?;
        let center = ctx.read_response(id)?.rect.center();
        if self.sidebar_rect.is_some_and(|r| r.contains(center)) {
            Some(Pane::Sidebar)
        } else if self.right_rect.is_some_and(|r| r.contains(center))
            || self.actions_strip_rect.is_some_and(|r| r.contains(center))
        {
            Some(Pane::Right)
        } else if self.toolbar_rect.is_some_and(|r| r.contains(center)) {
            Some(Pane::Toolbar)
        } else if self.central_rect.is_some_and(|r| r.contains(center)) {
            Some(Pane::Active)
        } else {
            None
        }
    }

    /// Marks the active pane by dimming every *other* pane instead of
    /// drawing a border on the active one — calmer and still unambiguous.
    /// `active` should already account for whether a gamepad
    /// is connected at all (see call sites) — mouse-only use never sets
    /// egui focus just by clicking, so unconditionally dimming "whatever
    /// isn't focused" would leave the whole app permanently dimmed for a
    /// mouse-only user.
    fn paint_pane_highlight(ui: &egui::Ui, active: bool) {
        if !active {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, egui::Color32::from_black_alpha(100));
        }
    }

    /// Whether the actions panel has anything to show for the current view
    /// — used both to enable the gamepad's West/context-menu button and to
    /// gate right-click on empty space.
    fn actions_available(&self) -> bool {
        match self.view {
            // Always true — New Folder is a valid action even with
            // nothing selected, same as any file manager's empty-space
            // right-click.
            View::Dir => true,
            View::Trash => self.selected_trash.is_some() || !self.trash_entries.is_empty(),
        }
    }
}

impl eframe::App for BrowDeckApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let previously_unfinished: Vec<bool> = self.jobs.iter().map(|j| !j.finished).collect();
        for job in &mut self.jobs {
            job.poll();
        }
        // A copy/move/extract job writes into `current_dir` but, unlike
        // delete/restore/empty-trash, has no synchronous call site to
        // follow up with a `refresh()` — the write happens on a
        // background thread. Catch the exact frame a job transitions to
        // finished (not every frame it lingers) and refresh then instead.
        let any_just_finished = self
            .jobs
            .iter()
            .zip(previously_unfinished)
            .any(|(job, was_unfinished)| was_unfinished && job.finished);
        if any_just_finished && matches!(self.view, View::Dir) {
            self.refresh();
        }
        let has_active_job = self.jobs.iter().any(|j| !j.finished);
        self.jobs
            .retain(|j| !j.finished || j.finished_at.is_none_or(|t| t.elapsed() < JOB_LINGER));
        if has_active_job || self.jobs.iter().any(|j| j.finished) {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        self.enforce_strip_focus_guard(ui.ctx());
        // Only update `current_pane` when a live classification actually
        // succeeds — see its field doc comment for why a `None` here
        // (typically the focused widget having scrolled off-screen) must
        // not overwrite it.
        if let Some(pane) = self.focused_pane(ui.ctx()) {
            self.current_pane = Some(pane);
        }
        let focused_pane = self.current_pane;
        // Reset every frame — only set again below if the right stick is
        // actually tilted past the deadzone this frame.
        self.pending_scroll = None;
        // Remember which widget was focused in each pane, so switching back
        // to it (LB/RB) restores the cursor there instead of landing on
        // nothing (or always the first row).
        if let Some(id) = ui.ctx().memory(|m| m.focused()) {
            match focused_pane {
                Some(Pane::Sidebar) => self.sidebar_last_focus = Some(id),
                Some(Pane::Active) => self.active_last_focus = Some(id),
                Some(Pane::Toolbar) => self.toolbar_last_focus = Some(id),
                Some(Pane::Right) => self.right_last_focus = Some(id),
                None => {}
            }
        }

        if let Some(gamepad) = &mut self.gamepad {
            let actions = gamepad.poll();
            // Keep polling at a game-loop-ish rate so held-direction repeat
            // and stick input feel responsive, not just event-driven.
            ui.ctx().request_repaint_after(Duration::from_millis(16));
            for action in actions {
                // While the Rename/New Folder text field has real
                // keyboard focus, gamepad d-pad/left-stick input is
                // swallowed instead of driving the normal strip
                // navigation — see `rename_editing`'s field doc comment.
                // Back exits edit mode (returns to navigating the strip
                // as elements, doesn't close the zone); everything else,
                // Activate included, is a no-op here since actual typing
                // happens via a real/on-screen keyboard, not this action
                // system.
                if self.rename_editing {
                    if matches!(action, gamepad::Action::Back) {
                        self.rename_editing = false;
                        self.rename_focus_pending = true;
                    }
                    continue;
                }
                match action {
                    gamepad::Action::Quit => {
                        self.quit_confirm = true;
                        self.focus_first_action = true;
                    }
                    gamepad::Action::Move(dir) => {
                        // egui's directional focus movement only works
                        // *relative to* an already-focused widget — if
                        // nothing has focus (fresh launch, or mouse use left
                        // it unset), seed one so this input isn't a silent
                        // no-op. Only done reactively, here, not every idle
                        // frame — otherwise it would keep stealing focus
                        // back from a mouse-driven selection that never set
                        // it in the first place.
                        self.ensure_focus_anchor(ui.ctx());
                        self.move_focus_confined(ui.ctx(), dir);
                    }
                    gamepad::Action::Activate => {
                        if self.multi_select && matches!(self.view, View::Dir) {
                            self.toggle_focused_entry(ui.ctx());
                        } else {
                            ui.ctx().input_mut(|i| {
                                i.events.push(egui::Event::Key {
                                    key: egui::Key::Space,
                                    physical_key: None,
                                    pressed: true,
                                    repeat: false,
                                    modifiers: egui::Modifiers::NONE,
                                });
                            });
                        }
                    }
                    gamepad::Action::ToggleMultiSelect => {
                        if matches!(self.view, View::Dir) {
                            self.multi_select = !self.multi_select;
                            if !self.multi_select {
                                self.multi_selected.clear();
                            }
                        }
                    }
                    gamepad::Action::Back => self.back_or_up(),
                    gamepad::Action::ContextMenu => {
                        if !self.multi_select && matches!(self.view, View::Dir) {
                            self.select_focused_entry(ui.ctx());
                        }
                        if self.actions_available() {
                            self.context_menu_open = true;
                            self.focus_first_action = true;
                        }
                    }
                    gamepad::Action::SwapPaneLeft => self.switch_pane(ui.ctx(), -1),
                    gamepad::Action::SwapPaneRight => self.switch_pane(ui.ctx(), 1),
                    gamepad::Action::TogglePreviewWidth => {
                        self.preview_wide = !self.preview_wide;
                    }
                    gamepad::Action::Refresh => match focused_pane {
                        Some(Pane::Sidebar) => {
                            self.places = places::list_places();
                            self.mounts = mounts::list_mounts();
                        }
                        Some(Pane::Active) => match self.view {
                            View::Dir => self.refresh(),
                            View::Trash => self.trash_entries = deleted::list_trash(),
                        },
                        _ => {}
                    },
                    gamepad::Action::Search => {
                        self.search_open = true;
                        self.focus_search = true;
                    }
                    gamepad::Action::ScaleDown => self.scale_down(),
                    gamepad::Action::ScaleUp => self.scale_up(),
                    gamepad::Action::Open => self.open_selected(),
                    gamepad::Action::Scroll(amount) => {
                        match focused_pane {
                            Some(Pane::Sidebar) | Some(Pane::Active) => {
                                // Right-stick scroll moves the actual
                                // selection here, not a free-floating view
                                // offset — letting the two drift apart made
                                // `scroll_to_me` (fired whenever focus does
                                // move) and manual `scroll_with_delta`
                                // fight each other. `Action::Scroll` fires
                                // every frame the stick is tilted (not
                                // edge-triggered like the d-pad), so it's
                                // throttled to `SCROLL_MOVE_INTERVAL`
                                // instead of moving a row every frame. Same
                                // sign convention as the left stick's
                                // Up/Down (`handle_axis` in gamepad.rs):
                                // positive Y = Up.
                                let now = std::time::Instant::now();
                                let ready = self
                                    .scroll_move_last
                                    .is_none_or(|t| now.duration_since(t) >= SCROLL_MOVE_INTERVAL);
                                if ready {
                                    let dir = if amount > 0.0 {
                                        egui::FocusDirection::Up
                                    } else {
                                        egui::FocusDirection::Down
                                    };
                                    self.ensure_focus_anchor(ui.ctx());
                                    self.move_focus_confined(ui.ctx(), dir);
                                    self.scroll_move_last = Some(now);
                                }
                            }
                            _ => {
                                // Stashed here and applied via
                                // `Ui::scroll_with_delta` from inside
                                // whichever pane's `ScrollArea` is
                                // currently focused — a synthetic
                                // `MouseWheel` event alone doesn't work
                                // (see NOTES.md, "gamepad scrolling needs
                                // `scroll_with_delta`, not a fake pointer
                                // hover"): egui only computes pointer
                                // hover once per pass, at its very start,
                                // so an event pushed mid-frame here is too
                                // late to affect it. No "selection" concept
                                // for the Right pane's Preview/Actions
                                // content, so this still free-scrolls.
                                self.pending_scroll = Some(amount * SCROLL_SPEED);
                            }
                        }
                    }
                }
            }
        }

        if self.show_header {
            egui::Panel::top("header_bar")
                .exact_size(HEADER_HEIGHT)
                .show(ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.weak("BrowDeck — Browse on Deck");
                    });
                });
        }

        let toolbar_resp = egui::Panel::top("toolbar").show(ui, |ui| {
            Self::paint_pane_highlight(ui, self.pane_undimmed(focused_pane, Pane::Toolbar));
            let mut toolbar_ids = Vec::new();
            ui.horizontal(|ui| {
                let hamburger = ui
                    .button(self.icon(ICON_MENU))
                    .on_hover_text("Show/hide the sidebar");
                if hamburger.clicked() {
                    self.sidebar_open = !self.sidebar_open;
                }
                self.top_focus_id = hamburger.id;
                toolbar_ids.push(hamburger.id);
                let up = ui
                    .button((self.icon(ICON_ARROW_UPWARD), "Up"))
                    .on_hover_text("Go up a directory");
                if up.clicked() {
                    self.go_back();
                }
                toolbar_ids.push(up.id);
                let refresh_mounts = ui
                    .button(self.icon(ICON_REFRESH))
                    .on_hover_text("Refresh mounts");
                if refresh_mounts.clicked() {
                    self.mounts = mounts::list_mounts();
                }
                toolbar_ids.push(refresh_mounts.id);
                let show_all = ui
                    .selectable_label(
                        self.show_all_mounts,
                        self.icon(if self.show_all_mounts {
                            ICON_VISIBILITY
                        } else {
                            ICON_VISIBILITY_OFF
                        }),
                    )
                    .on_hover_text("Show all mounts, not just common locations");
                if show_all.clicked() {
                    self.show_all_mounts = !self.show_all_mounts;
                }
                toolbar_ids.push(show_all.id);
                let hidden = ui
                    .selectable_label(self.show_hidden_files, self.icon(ICON_PRIORITY_HIGH))
                    .on_hover_text("Show hidden files");
                if hidden.clicked() {
                    self.show_hidden_files = !self.show_hidden_files;
                }
                toolbar_ids.push(hidden.id);
                let sort_by = ui
                    .button((self.icon(ICON_SORT), self.sort_by.label()))
                    .on_hover_text("Cycle sort: Name / Size / Modified");
                if sort_by.clicked() {
                    self.sort_by = self.sort_by.next();
                    self.resort_current_view();
                }
                toolbar_ids.push(sort_by.id);
                let ascending = self.sort_ascending;
                let sort_dir = ui
                    .button(self.icon(if ascending {
                        ICON_ARROW_UPWARD
                    } else {
                        ICON_ARROW_DOWNWARD
                    }))
                    .on_hover_text(if ascending {
                        "Ascending — click for descending"
                    } else {
                        "Descending — click for ascending"
                    });
                if sort_dir.clicked() {
                    self.sort_ascending = !self.sort_ascending;
                    self.resort_current_view();
                }
                toolbar_ids.push(sort_dir.id);
                ui.separator();
                // A disabled button (at either end of the scale range)
                // surrenders focus the instant egui draws it — see
                // NOTES.md "toolbar teleport at min/max zoom" — so its id
                // must never be handed to `set_focus`/land in
                // `toolbar_ids`, or d-pad/stick navigation stepping onto
                // it snaps back to index 0 (the hamburger) next frame.
                let zoom_out_enabled = *ICON_SCALE_RANGE.start() < self.icon_scale;
                let zoom_out = ui
                    .add_enabled(
                        zoom_out_enabled,
                        egui::Button::new(self.icon(ICON_ZOOM_OUT)),
                    )
                    .on_hover_text("Smaller scale");
                if zoom_out.clicked() {
                    self.scale_down();
                }
                if zoom_out_enabled {
                    toolbar_ids.push(zoom_out.id);
                }
                let zoom_in_enabled = self.icon_scale < *ICON_SCALE_RANGE.end();
                let zoom_in = ui
                    .add_enabled(zoom_in_enabled, egui::Button::new(self.icon(ICON_ZOOM_IN)))
                    .on_hover_text("Bigger scale");
                if zoom_in.clicked() {
                    self.scale_up();
                }
                if zoom_in_enabled {
                    toolbar_ids.push(zoom_in.id);
                }
                ui.separator();
                let preview = ui
                    .selectable_label(self.preview_enabled, (self.icon(ICON_PREVIEW), "Preview"))
                    .on_hover_text("Auto-preview images/text files on select");
                if preview.clicked() {
                    self.preview_enabled = !self.preview_enabled;
                    self.refresh_preview();
                }
                toolbar_ids.push(preview.id);
                ui.separator();
                match self.view {
                    View::Dir => ui.label(self.current_dir.to_string_lossy()),
                    View::Trash => ui.label("Trash"),
                };
                if self.multi_select {
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 150, 255),
                        format!("Multi-select ({} selected)", self.multi_selected.len()),
                    );
                }
            });
            self.toolbar_ids = toolbar_ids;
        });
        self.toolbar_rect = Some(toolbar_resp.response.rect);

        if self.sidebar_open {
            // Single-selection only ("multi select doesn't count") — the
            // card's own fixed height is carved out of the sidebar before
            // the scrollable Places/Mounts/Trash list, same "reserve the
            // exact rect first" approach as the Actions/Permissions strip
            // (see its own comment for why `allocate_ui` alone isn't
            // reliable for this).
            let show_info =
                !self.multi_select && self.selected.is_some() && matches!(self.view, View::Dir);
            let resp = egui::Panel::left("sidebar").show(ui, |ui| {
                Self::paint_pane_highlight(ui, self.pane_undimmed(focused_pane, Pane::Sidebar));
                let info_height = if show_info { SIDEBAR_INFO_HEIGHT } else { 0.0 };
                let mut list_rect = ui.available_rect_before_wrap();
                list_rect.set_height((list_rect.height() - info_height).max(0.0));
                ui.allocate_rect(list_rect, egui::Sense::hover());
                let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
                list_ui.set_clip_rect(list_rect);
                egui::ScrollArea::vertical()
                    .id_salt("sidebar_list")
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(ALWAYS_VISIBLE_SCROLLBAR)
                    .show(&mut list_ui, |ui| self.show_sidebar_contents(ui));
                if show_info {
                    self.show_selected_info(ui);
                }
            });
            self.sidebar_rect = Some(resp.response.rect);
        } else {
            self.sidebar_rect = None;
            self.sidebar_ids.clear();
        }

        let preview_visible = self.preview_visible();
        let show_actions = self.context_menu_open;
        let show_rename = self.rename_editor.is_some();
        let show_permissions = self.perm_editor.is_some();
        let show_progress = !self.jobs.is_empty();

        // Actions/Rename/Permissions/Progress are a horizontal strip
        // along the very bottom, above the status bar, spanning from the
        // sidebar/main-pane border to the window's right edge (drawn
        // *after* the sidebar so it naturally excludes that width, and
        // *before* the right/central panels so they get whatever's left
        // above it) — not part of the Preview pane. Rows stack in fixed
        // priority order (Actions, Rename, Permissions, Progress),
        // skipping whichever aren't active.
        enum FooterZone {
            Actions,
            Rename,
            Permissions,
            Progress,
            Quit,
        }
        let mut active_zones = Vec::new();
        if self.quit_confirm {
            // Exclusive — whatever else is open stays open underneath
            // (just not rendered this frame), so canceling out of Quit
            // doesn't lose it.
            active_zones.push(FooterZone::Quit);
        } else {
            if show_actions {
                active_zones.push(FooterZone::Actions);
            }
            if show_rename {
                active_zones.push(FooterZone::Rename);
            }
            if show_permissions {
                active_zones.push(FooterZone::Permissions);
            }
            if show_progress {
                active_zones.push(FooterZone::Progress);
            }
        }
        const STATUS_BAR_HEIGHT: f32 = 32.0;
        // Reserved height auto-fits the strip's *actual* content (see
        // `strip_measured_height`'s doc comment) rather than a
        // `ROW_HEIGHT * row count` guess, which left a visible gap even
        // after every badge became a uniform height — the true per-row
        // footprint (badges plus `horizontal_wrapped`'s own spacing)
        // never matched the guess exactly. Clamped to a sane minimum (in
        // case a stale/zero measurement ever gets cached) and the same
        // 220 max as before, so a great many simultaneous Progress rows
        // still scroll instead of taking over the screen.
        let strip_height = if active_zones.is_empty() {
            0.0
        } else {
            self.strip_measured_height.clamp(20.0, 220.0)
        };
        // The strip and the status bar are laid out inside a *single*
        // bottom Panel, always called (even when `active_zones` is
        // empty, at just `STATUS_BAR_HEIGHT` tall) — two things learned
        // the hard way earlier this session, both on this exact Panel:
        // (1) splitting the status bar into its own separate, earlier,
        // full-width `Panel::bottom` (called before the sidebar's
        // `Panel::left`, to recover the sidebar's width for the legend)
        // produced real layout bugs once this strip was showing too —
        // an uneven pane-highlight border, the strip's top edge
        // overlapping the file list, and a stray gap between the strip
        // and the status bar. Not root-caused further; reverted back to
        // one combined Panel scoped to exclude the sidebar's column
        // (like this whole strip always has been), just with the
        // legend's *content* still centered within that narrower width
        // rather than the full window — the earlier fixes (no
        // `ScrollArea`, trimmed labels, legend always shown) already
        // make it fit comfortably there anyway. (2) Stacking a *third*
        // `Panel::bottom` (after `status_bar`, then `sidebar`'s left
        // panel) inside the same `ui` produced a panel whose content
        // `ui.max_rect()` didn't match its own `response.rect` by ~20px
        // every frame — the content silently painted into the file
        // list's territory. One Panel with internal rows sidesteps both
        // of these entirely.
        let footer_height = STATUS_BAR_HEIGHT + strip_height;
        egui::Panel::bottom("footer")
            .exact_size(footer_height)
            .show(ui, |ui| {
                if !active_zones.is_empty() {
                    // `ui.allocate_ui(size, ...)` does *not* hard-cap the
                    // child to `size` — per its own docs, overflowing
                    // content gets more space, and the parent's cursor
                    // only advances by what was actually used.
                    // `ScrollArea::vertical()` with `auto_shrink([false,
                    // false])`, even with `.max_height()` set, sizes
                    // itself against a larger ambient bound than the
                    // nested-`allocate_ui` child it was given (reported
                    // ~64px tall against a 44px request in testing),
                    // silently growing the footer's cursor and pushing the
                    // status bar row down past its intended band. Fixed by
                    // reserving the exact rect *before* building the
                    // strip's content — `allocate_rect` advances the
                    // parent's cursor by exactly `strip_height` regardless
                    // of what the child does, and the child `Ui`'s
                    // `clip_rect` matches so nothing can visually bleed
                    // past it either.
                    let mut strip_rect = ui.available_rect_before_wrap();
                    strip_rect.set_height(strip_height);
                    ui.allocate_rect(strip_rect, egui::Sense::hover());
                    let mut strip_ui = ui.new_child(egui::UiBuilder::new().max_rect(strip_rect));
                    strip_ui.set_clip_rect(strip_rect);
                    Self::paint_pane_highlight(
                        &strip_ui,
                        self.pane_undimmed(focused_pane, Pane::Right),
                    );
                    let scroll_output = egui::ScrollArea::vertical()
                        .id_salt("actions_strip_scroll")
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(ALWAYS_VISIBLE_SCROLLBAR)
                        .max_height(strip_height)
                        .show(&mut strip_ui, |ui| {
                            self.apply_pending_scroll(ui, Pane::Right);
                            self.right_focus_rows.clear();
                            // A little breathing room above the first row
                            // and below the last — the auto-fit measurement
                            // (see `strip_measured_height`) tracks content
                            // exactly, so with none at all the first row
                            // sat flush against the strip's own top border,
                            // reading as too tight.
                            ui.add_space(2.0);
                            for zone in &active_zones {
                                match zone {
                                    FooterZone::Actions => self.show_actions_zone(ui),
                                    FooterZone::Rename => self.show_rename_editor(ui),
                                    FooterZone::Permissions => self.show_permission_editor(ui),
                                    FooterZone::Progress => self.show_progress_zone(ui),
                                    FooterZone::Quit => self.show_quit_zone(ui),
                                }
                            }
                            ui.add_space(1.0);
                        });
                    // Feeds `strip_height` above, next frame — see
                    // `strip_measured_height`'s doc comment.
                    self.strip_measured_height = scroll_output.content_size.y;
                    self.actions_strip_rect = Some(strip_rect);
                } else {
                    self.actions_strip_rect = None;
                }
                self.show_status_bar(ui);
            });

        // Preview is its own right-docked pane, independent of the strip
        // above — Select only ever resizes *this*.
        if preview_visible {
            // Always pin an exact width — an exact_size() panel's width is
            // *remembered* across frames (`ctx` state keyed by "right_pane"),
            // so only setting it while `preview_wide` is true and skipping
            // the call otherwise never reverted anything: with no new
            // constraint given, the panel just kept the last exact_size it
            // was ever given. Both states now set one explicitly.
            let width = if self.preview_wide {
                ui.ctx().content_rect().width() * 0.4
            } else {
                PREVIEW_NORMAL_WIDTH
            };
            let right_panel = egui::Panel::right("right_pane").exact_size(width);
            let right_resp = right_panel.show(ui, |ui| {
                Self::paint_pane_highlight(ui, self.pane_undimmed(focused_pane, Pane::Right));
                egui::Frame::group(ui.style())
                    .inner_margin(8.0)
                    .show(ui, |ui| self.show_preview_panel(ui));
            });
            self.right_rect = Some(right_resp.response.rect);
        } else {
            self.right_rect = None;
        }

        let central_resp = egui::CentralPanel::default().show(ui, |ui| {
            Self::paint_pane_highlight(ui, self.pane_undimmed(focused_pane, Pane::Active));
            match self.view {
                View::Dir => self.show_dir(ui),
                View::Trash => self.show_trash(ui),
            }
        });
        self.central_rect = Some(central_resp.response.rect);
    }
}

impl BrowDeckApp {
    fn show_sidebar_contents(&mut self, ui: &mut egui::Ui) {
        // No `apply_pending_scroll` here — right-stick scroll moves the
        // selection in this pane now (see the `Action::Scroll` handler),
        // which drives the view via `scroll_to_me` instead.
        let mut sidebar_ids = Vec::new();
        ui.heading("Places");
        let mut clicked_place = None;
        for place in &self.places {
            let r =
                ui.selectable_label(false, (self.icon_scaled(place.icon), place.label.as_str()));
            if self.scroll_to_focus == Some(r.id) {
                r.scroll_to_me(Some(egui::Align::Center));
                self.scroll_to_focus = None;
            }
            sidebar_ids.push(r.id);
            if r.clicked() {
                clicked_place = Some(place.path.clone());
            }
        }

        ui.add_space(8.0);
        ui.heading("Mounts");
        let visible_mounts: Vec<&mounts::Mount> = self
            .mounts
            .iter()
            .filter(|m| self.show_all_mounts || m.is_common_location())
            .collect();
        if visible_mounts.is_empty() {
            ui.weak("(none)");
        }
        let mut clicked_mount = None;
        for mount in visible_mounts {
            let mount_label = mount.mount_point.to_string_lossy().into_owned();
            let response = ui
                .selectable_label(
                    false,
                    (self.icon_scaled(mount.icon()), mount_label.as_str()),
                )
                .on_hover_text(&mount.device);
            if self.scroll_to_focus == Some(response.id) {
                response.scroll_to_me(Some(egui::Align::Center));
                self.scroll_to_focus = None;
            }
            sidebar_ids.push(response.id);
            if response.clicked() {
                clicked_mount = Some(mount.mount_point.clone());
            }
        }
        if let Some(dir) = clicked_place.or(clicked_mount) {
            self.navigate_to(dir);
        }

        ui.add_space(8.0);
        let trash_row = ui.selectable_label(
            matches!(self.view, View::Trash),
            (self.icon_scaled(ICON_DELETE), "Trash"),
        );
        sidebar_ids.push(trash_row.id);
        if trash_row.clicked() {
            self.open_trash();
        }
        self.sidebar_ids = sidebar_ids;
    }

    /// Basic info for the single selected file/folder — name, owner,
    /// group, size, permissions, and (if a symlink) what it points to.
    /// A passive display, nothing here is interactive or focusable — just
    /// a top separator line (like a section break elsewhere in the app),
    /// not a full boxed frame.
    fn show_selected_info(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("selected_info_scroll")
            .auto_shrink([false, false])
            .scroll_bar_visibility(ALWAYS_VISIBLE_SCROLLBAR)
            .show(ui, |ui| {
                let info = self.selected_info(&path);
                ui.add(egui::Label::new(egui::RichText::new(&info.name).strong()).truncate())
                    .on_hover_text(&info.name);
                // Right-space-padded to "Owner"/"Group"'s width (5 — the
                // longest of just these three, Permissions excluded) so
                // the colons line up — needs a monospace font to
                // actually align on screen; the app's regular
                // proportional font can't align via padding spaces
                // alone.
                ui.monospace(format!("{:<5} : {}", "Owner", info.owner));
                ui.monospace(format!("{:<5} : {}", "Group", info.group));
                ui.monospace(format!("{:<5} : {}", "Size", info.size));
                // No label — the mode string alone (e.g. `rwxr-xr-x`) is
                // self-explanatory.
                ui.monospace(info.permissions.as_str());
                if let Some(target) = &info.symlink_target {
                    // No "Real path:" label and no `.truncate()` — plain
                    // `ui.label` wraps by default, so the full target is
                    // visible across multiple lines instead of being cut
                    // off with an ellipsis.
                    ui.label(target);
                }
            });
    }

    fn show_dir(&mut self, ui: &mut egui::Ui) {
        if self.search_open {
            ui.horizontal(|ui| {
                ui.label(self.icon_scaled(ICON_SEARCH));
                let search_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Search this folder…")
                        .desired_width(f32::INFINITY),
                );
                if self.focus_search {
                    search_resp.request_focus();
                    self.focus_search = false;
                }
                if search_resp.changed() {
                    self.recursive_dirty = true;
                    self.recursive_spinner_shown = false;
                }
                let recursive_toggle = ui
                    .selectable_label(self.recursive_search, self.icon_scaled(ICON_ACCOUNT_TREE))
                    .on_hover_text("Also search subfolders");
                if recursive_toggle.clicked() {
                    self.recursive_search = !self.recursive_search;
                    self.recursive_dirty = true;
                    self.recursive_spinner_shown = false;
                }
                if ui
                    .button(self.icon_scaled(ICON_CLOSE))
                    .on_hover_text("Close search")
                    .clicked()
                {
                    self.search_open = false;
                    self.search_query.clear();
                    self.recursive_results.clear();
                    self.recursive_dirty = false;
                    self.recursive_spinner_shown = false;
                }
            });
        } else if ui
            .button(self.icon_scaled(ICON_SEARCH))
            .on_hover_text("Search this folder")
            .clicked()
        {
            self.search_open = true;
            self.focus_search = true;
        }
        ui.separator();

        let recursive_active =
            self.recursive_search && self.search_open && !self.search_query.is_empty();
        if recursive_active && self.recursive_dirty {
            // Deferred one frame on purpose: this lets the spinner
            // actually get painted before the (blocking, synchronous)
            // walk runs — see NOTES.md "recursive search runs
            // synchronously, not threaded".
            if self.recursive_spinner_shown {
                let query = self.search_query.to_lowercase();
                self.recursive_results = self.recursive_search(&query);
                self.recursive_dirty = false;
                self.recursive_spinner_shown = false;
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Searching…");
                });
                self.recursive_spinner_shown = true;
                ui.ctx().request_repaint();
                return;
            }
        }

        egui::ScrollArea::vertical()
            .id_salt("dir_list")
            .auto_shrink([false, false])
            .scroll_bar_visibility(ALWAYS_VISIBLE_SCROLLBAR)
            .show(ui, |ui| {
                // No `apply_pending_scroll` — right-stick scroll moves
                // selection here now, which drives the view itself.
                let query = self.search_query.to_lowercase();
                let source: &[Entry] = if recursive_active {
                    &self.recursive_results
                } else {
                    &self.entries
                };
                let mut new_selection = None;
                let mut next_dir = None;
                let mut open_file = None;
                let mut empty_area_secondary_click = false;
                let mut entry_ids = Vec::new();
                for entry in source {
                    if !self.show_hidden_files && entry.name.starts_with('.') {
                        continue;
                    }
                    // Recursive results are already filtered by the walk
                    // itself; only the flat (non-recursive) list needs
                    // filtering here.
                    if !recursive_active
                        && !query.is_empty()
                        && !entry.name.to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    let icon = fileicons::icon_for(&entry.path, entry.is_dir);
                    let is_selected = if self.multi_select {
                        self.multi_selected.contains(&entry.path)
                    } else {
                        self.selected.as_deref() == Some(entry.path.as_path())
                    };
                    let response = ui.selectable_label(
                        is_selected,
                        (self.icon_scaled(icon), entry.name.as_str()),
                    );
                    if self.scroll_to_focus == Some(response.id) {
                        response.scroll_to_me(Some(egui::Align::Center));
                        self.scroll_to_focus = None;
                    }
                    entry_ids.push((response.id, entry.path.clone()));
                    if response.clicked() {
                        if self.multi_select {
                            if !self.multi_selected.remove(&entry.path) {
                                self.multi_selected.insert(entry.path.clone());
                            }
                        } else {
                            new_selection = Some(entry.path.clone());
                            if entry.is_dir {
                                // Single click/gamepad-activate enters a directory —
                                // double-click is reserved for opening files (mouse).
                                next_dir = Some(entry.path.clone());
                            }
                        }
                    }
                    if response.double_clicked() && !entry.is_dir && !self.multi_select {
                        open_file = Some(entry.path.clone());
                    }
                    if response.secondary_clicked() {
                        new_selection = Some(entry.path.clone());
                        self.context_menu_open = true;
                        self.focus_first_action = true;
                    }
                }
                self.entry_ids = entry_ids;
                let remaining = ui.available_size();
                if remaining.y > 4.0 {
                    let empty_resp = ui.allocate_response(remaining, egui::Sense::click());
                    if empty_resp.secondary_clicked() {
                        empty_area_secondary_click = true;
                    }
                }
                if let Some(path) = new_selection {
                    self.set_selected(Some(path));
                }
                if let Some(dir) = next_dir {
                    self.navigate_to(dir);
                }
                if open_file.is_some() {
                    self.selected = open_file;
                    self.open_selected();
                }
                if empty_area_secondary_click {
                    self.set_selected(None);
                    if self.clipboard.is_some() {
                        self.context_menu_open = true;
                        self.focus_first_action = true;
                    }
                }
            });
    }

    fn show_trash(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("trash_list")
            .auto_shrink([false, false])
            .scroll_bar_visibility(ALWAYS_VISIBLE_SCROLLBAR)
            .show(ui, |ui| {
                // No `apply_pending_scroll` — right-stick scroll moves
                // selection here now, which drives the view itself.
                let mut restore_idx = None;
                let mut empty_area_secondary_click = false;
                let mut trash_ids = Vec::new();
                for (i, entry) in self.trash_entries.iter().enumerate() {
                    let label = format!(
                        "{}  (from {})",
                        entry.name,
                        entry.original_path.to_string_lossy()
                    );
                    let is_selected = self.selected_trash == Some(i);
                    let response = ui.selectable_label(
                        is_selected,
                        (self.icon_scaled(ICON_RESTORE_FROM_TRASH), label),
                    );
                    if self.scroll_to_focus == Some(response.id) {
                        response.scroll_to_me(Some(egui::Align::Center));
                        self.scroll_to_focus = None;
                    }
                    trash_ids.push(response.id);
                    if response.clicked() {
                        self.selected_trash = Some(i);
                    }
                    if response.double_clicked() {
                        restore_idx = Some(i);
                    }
                    if response.secondary_clicked() {
                        self.selected_trash = Some(i);
                        self.context_menu_open = true;
                        self.focus_first_action = true;
                    }
                }
                self.trash_ids = trash_ids;
                let remaining = ui.available_size();
                if remaining.y > 4.0 {
                    let empty_resp = ui.allocate_response(remaining, egui::Sense::click());
                    if empty_resp.secondary_clicked() {
                        empty_area_secondary_click = true;
                    }
                }
                if let Some(i) = restore_idx {
                    if let Err(e) = deleted::restore(&self.trash_entries[i]) {
                        eprintln!("restore failed: {e}");
                    }
                    self.trash_entries = deleted::list_trash();
                    self.selected_trash = None;
                }
                if empty_area_secondary_click && !self.trash_entries.is_empty() {
                    self.selected_trash = None;
                    self.context_menu_open = true;
                    self.focus_first_action = true;
                }
            });
    }

    /// Stable id for Preview's own focus anchor — Preview has no list of
    /// navigable widgets (just scrollable content), so this stands in for
    /// "Preview has focus" the same way `top_focus_id` stands in for the
    /// toolbar's hamburger button.
    fn preview_focus_id() -> egui::Id {
        egui::Id::new("preview_focus_anchor")
    }

    fn show_preview_panel(&mut self, ui: &mut egui::Ui) {
        // `focusable_noninteractive()` — no click/drag sense, so this
        // can't steal pointer interaction from anything drawn after it —
        // just gives LB/RB something to focus so Preview can be reached
        // and classified as `Pane::Right` like everything else.
        let anchor = ui.interact(
            ui.max_rect(),
            Self::preview_focus_id(),
            egui::Sense::focusable_noninteractive(),
        );
        if self.scroll_to_focus == Some(anchor.id) {
            self.scroll_to_focus = None;
        }
        ui.heading("Preview");
        let Some(path) = self.selected.clone() else {
            return;
        };
        match preview::classify(&path) {
            Some(preview::Kind::Image) => {
                // Fills whatever's left in the pane (the footer, if any,
                // already claimed its own space below) — `shrink_to_fit`
                // scales the image to fit that fully, up or down,
                // preserving aspect ratio, rather than sitting at its
                // intrinsic size with dead space around it.
                egui::ScrollArea::vertical()
                    .id_salt("preview_image")
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(ALWAYS_VISIBLE_SCROLLBAR)
                    .show(ui, |ui| {
                        self.apply_pending_scroll(ui, Pane::Right);
                        ui.add(egui::Image::new(preview::file_uri(&path)).shrink_to_fit());
                    });
            }
            Some(preview::Kind::Text) => {
                if let Some((cached_path, text)) = &self.preview_text
                    && *cached_path == path
                {
                    let text = text.clone();
                    egui::ScrollArea::vertical()
                        .id_salt("preview_text")
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(ALWAYS_VISIBLE_SCROLLBAR)
                        .show(ui, |ui| {
                            self.apply_pending_scroll(ui, Pane::Right);
                            ui.monospace(text);
                        });
                }
            }
            None => {}
        }
    }

    /// Flat, button-driven actions panel — not a mouse-style hover/right-click
    /// popup, so every action is a normal focusable/gamepad-activatable
    /// button (see NOTES.md on why: Dolphin's nested right-click menu under
    /// gamescope was the thing this app exists to not be). Lives in the
    /// right pane's lower section, revealed on demand.
    /// Dispatches to the Dir/Trash action row — deliberately no wrapper
    /// content of its own (title/close live in each, alongside their
    /// buttons) so everything renders as one continuous wrapped row, not
    /// a heading row plus a separate button row.
    fn show_actions_zone(&mut self, ui: &mut egui::Ui) {
        match self.view {
            View::Dir => self.show_dir_actions(ui),
            View::Trash => self.show_trash_actions(ui),
        }
    }

    fn show_dir_actions(&mut self, ui: &mut egui::Ui) {
        let paths = self.selected_paths();
        let single = (paths.len() == 1).then(|| paths[0].clone());
        let mut first_id = None;
        let mut row_ids: Vec<egui::Id> = Vec::new();
        ui.horizontal_wrapped(|ui| {
            let close = Self::action_badge(self.badge_min_height(), ui, self.icon(ICON_CLOSE));
            row_ids.push(close.id);
            if close.clicked() {
                self.context_menu_open = false;
            }
            ui.strong("Actions");
            if let Some(path) = &single {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                ui.weak(name).on_hover_text(path.to_string_lossy());
            } else if paths.len() > 1 {
                ui.weak(format!("{} items", paths.len()));
            }
            ui.separator();
            if self.clipboard.is_some() && self.jobs.is_empty() {
                let r = Self::action_badge(
                    self.badge_min_height(),
                    ui,
                    (self.icon(ICON_CONTENT_PASTE), "Paste"),
                );
                first_id.get_or_insert(r.id);
                row_ids.push(r.id);
                if r.clicked() {
                    self.paste();
                    // Deliberately left open: the copy/move progress zone
                    // stacks below Actions rather than replacing it.
                }
            }
            let r = Self::action_badge(
                self.badge_min_height(),
                ui,
                (self.icon(ICON_CREATE_NEW_FOLDER), "New Folder"),
            );
            first_id.get_or_insert(r.id);
            row_ids.push(r.id);
            if r.clicked() {
                self.rename_editor = Some(RenameEditor {
                    target: None,
                    text: "New Folder".to_string(),
                });
                self.focus_first_action = true;
                // Deliberately left open: the rename/new-folder zone
                // stacks below Actions rather than replacing it.
            }
            if let Some(path) = &single {
                let r = Self::action_badge(
                    self.badge_min_height(),
                    ui,
                    (self.icon(ICON_DRIVE_FILE_RENAME_OUTLINE), "Rename"),
                );
                first_id.get_or_insert(r.id);
                row_ids.push(r.id);
                if r.clicked() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.to_string_lossy().into_owned());
                    self.rename_editor = Some(RenameEditor {
                        target: Some(path.clone()),
                        text: name,
                    });
                    self.focus_first_action = true;
                    // Deliberately left open: the rename/new-folder zone
                    // stacks below Actions rather than replacing it.
                }
            }
            if let Some(path) = &single
                && !path.is_dir()
            {
                let r = Self::action_badge(
                    self.badge_min_height(),
                    ui,
                    (self.icon(ICON_OPEN_IN_NEW), "Open"),
                );
                first_id.get_or_insert(r.id);
                row_ids.push(r.id);
                if r.clicked() {
                    self.open_selected();
                    self.context_menu_open = false;
                }
            }
            if !paths.is_empty() {
                let r = Self::action_badge(
                    self.badge_min_height(),
                    ui,
                    (self.icon(ICON_CONTENT_COPY), "Copy"),
                );
                first_id.get_or_insert(r.id);
                row_ids.push(r.id);
                if r.clicked() {
                    self.clipboard = Some(Clipboard {
                        paths: paths.clone(),
                        cut: false,
                    });
                    self.context_menu_open = false;
                }
                let r = Self::action_badge(
                    self.badge_min_height(),
                    ui,
                    (self.icon(ICON_CONTENT_CUT), "Cut"),
                );
                first_id.get_or_insert(r.id);
                row_ids.push(r.id);
                if r.clicked() {
                    self.clipboard = Some(Clipboard {
                        paths: paths.clone(),
                        cut: true,
                    });
                    self.context_menu_open = false;
                }
                let r = Self::action_badge(
                    self.badge_min_height(),
                    ui,
                    (self.icon(ICON_DELETE), "Delete"),
                );
                first_id.get_or_insert(r.id);
                row_ids.push(r.id);
                if r.clicked() {
                    self.delete_paths(&paths);
                    self.context_menu_open = false;
                }
            }
            if let Some(path) = &single
                && fileops::is_archive(path)
                && self.jobs.is_empty()
            {
                let r = Self::action_badge(
                    self.badge_min_height(),
                    ui,
                    (self.icon(ICON_UNARCHIVE), "Extract"),
                );
                first_id.get_or_insert(r.id);
                row_ids.push(r.id);
                if r.clicked() {
                    let dest_dir = self.current_dir.clone();
                    self.jobs
                        .push(fileops::spawn_extract(path.clone(), dest_dir));
                    // Deliberately left open: the extract progress zone
                    // stacks below Actions rather than replacing it.
                }
            }
            if !paths.is_empty() {
                let r = Self::action_badge(
                    self.badge_min_height(),
                    ui,
                    (self.icon(ICON_LOCK), "Permissions"),
                );
                first_id.get_or_insert(r.id);
                row_ids.push(r.id);
                // Multi-selection: applies one mode to every selected
                // path on Apply. Initial checkbox state comes from just
                // the first path — they may not all start out the same,
                // but Apply always sets the same mode on all of them
                // regardless of what each one started as.
                if r.clicked()
                    && let Some(mode) = permissions::read_mode(&paths[0])
                {
                    self.perm_editor = Some(PermEditor {
                        paths: paths.clone(),
                        mode,
                    });
                    self.focus_first_action = true;
                    // Deliberately left open: the permission editor stacks
                    // below Actions rather than replacing it.
                }
            }
            if let Some(path) = &single
                && iprolaunch::is_launchable(path)
                && self.iprolaunch_bin.is_some()
            {
                if self.iprolaunch_registered(path) {
                    Self::action_badge_disabled(
                        self.badge_min_height(),
                        ui,
                        (self.icon(ICON_CHECK_CIRCLE), "IProLaunch"),
                    )
                    .on_hover_text("Already in the IProLaunch library");
                } else {
                    let r = Self::action_badge(
                        self.badge_min_height(),
                        ui,
                        (self.icon(ICON_ROCKET_LAUNCH), "IProLaunch"),
                    );
                    first_id.get_or_insert(r.id);
                    row_ids.push(r.id);
                    if r.clicked() {
                        let bin = self.iprolaunch_bin.clone().unwrap();
                        match iprolaunch::add(&bin, path) {
                            Ok(()) => self.iprolaunch_status = Some((path.clone(), true)),
                            Err(e) => eprintln!("IProLaunch add failed: {e}"),
                        }
                    }
                }
            }
        });
        self.right_focus_rows.push(row_ids);
        // Skip if Permissions/Rename was *just* opened by a click above:
        // `active_zones` was computed once, before this render, so that
        // new zone won't actually render (and consume `focus_first_action`
        // itself) until next frame — if this check didn't defer to it,
        // Actions' own `first_id` would steal the flag first, focusing
        // the wrong badge instead of the new zone's own first widget.
        if self.focus_first_action
            && self.perm_editor.is_none()
            && self.rename_editor.is_none()
            && let Some(id) = first_id
        {
            ui.ctx().memory_mut(|m| m.request_focus(id));
            self.focus_first_action = false;
        }
    }

    fn show_trash_actions(&mut self, ui: &mut egui::Ui) {
        let mut first_id = None;
        let mut row_ids: Vec<egui::Id> = Vec::new();
        ui.horizontal_wrapped(|ui| {
            let close = Self::action_badge(self.badge_min_height(), ui, self.icon(ICON_CLOSE));
            row_ids.push(close.id);
            if close.clicked() {
                self.context_menu_open = false;
            }
            ui.strong("Actions");
            ui.separator();
            if let Some(i) = self.selected_trash {
                let r = Self::action_badge(
                    self.badge_min_height(),
                    ui,
                    (self.icon(ICON_RESTORE_FROM_TRASH), "Restore"),
                );
                first_id.get_or_insert(r.id);
                row_ids.push(r.id);
                if r.clicked() {
                    if let Err(e) = deleted::restore(&self.trash_entries[i]) {
                        eprintln!("restore failed: {e}");
                    }
                    self.trash_entries = deleted::list_trash();
                    self.selected_trash = None;
                    self.context_menu_open = false;
                }
            }
            if !self.trash_entries.is_empty() {
                let r = Self::action_badge(
                    self.badge_min_height(),
                    ui,
                    (self.icon(ICON_DELETE_SWEEP), "Empty All"),
                );
                first_id.get_or_insert(r.id);
                row_ids.push(r.id);
                if r.clicked() {
                    if let Err(e) = deleted::empty_all() {
                        eprintln!("empty trash failed: {e}");
                    }
                    self.trash_entries = deleted::list_trash();
                    self.selected_trash = None;
                    self.context_menu_open = false;
                }
            }
        });
        self.right_focus_rows.push(row_ids);
        if self.focus_first_action
            && let Some(id) = first_id
        {
            ui.ctx().memory_mut(|m| m.request_focus(id));
            self.focus_first_action = false;
        }
    }

    /// Both "Rename" (`target: Some(path)`) and "New Folder"
    /// (`target: None`, creates in `current_dir`) — one text field, one
    /// zone, differing only in what Apply does and the label/hint text.
    ///
    /// The name field itself is only a real, focusable `TextEdit` while
    /// `rename_editing` is true; otherwise it renders as an ordinary
    /// `action_badge` showing the current text, navigable by d-pad like
    /// any other badge. This two-stage design (press A to start
    /// editing, B to stop) exists because a `TextEdit` that has genuine
    /// egui keyboard focus is exactly what Steam Input watches for to
    /// start routing the physical d-pad/left-stick into text-cursor
    /// movement — if the field were auto-focused the instant this zone
    /// opened (the old behavior), the very next d-pad press moved the
    /// text cursor instead of navigating to another badge, with no way
    /// to tell which one would happen.
    fn show_rename_editor(&mut self, ui: &mut egui::Ui) {
        let Some(editor) = &mut self.rename_editor else {
            return;
        };
        let is_new_folder = editor.target.is_none();
        let target = editor.target.clone();
        // Taken, not cloned, and put back after the row closure — the
        // `TextEdit` needs `&mut` on the live field, but the closure also
        // needs `&mut self` for `self.icon(...)`/`row_ids`/etc., and
        // borrowing `self.rename_editor` mutably for the whole closure
        // would conflict with that.
        let mut text = std::mem::take(&mut editor.text);
        let mut apply = false;
        let mut cancel = false;
        let mut first_id = None;
        let mut row_ids: Vec<egui::Id> = Vec::new();
        let editing = self.rename_editing;
        // Captured *before* the row below can set it — a click on the
        // field badge (inside that row, further down) also sets
        // `rename_focus_pending`, but that means "focus the real
        // `TextEdit` next frame, once it exists", not this one (this
        // frame already committed to drawing the badge, since `editing`
        // above was read before the click happens). Only a flag that was
        // already true *coming into* this frame — e.g. set by the B-exits-
        // editing handler in the gamepad dispatch loop, which runs earlier
        // in the same frame — should be consumed by this frame's render.
        let focus_pending = self.rename_focus_pending;
        let mut field_id = None;
        ui.horizontal_wrapped(|ui| {
            let close = Self::action_badge(self.badge_min_height(), ui, self.icon(ICON_CLOSE));
            row_ids.push(close.id);
            if close.clicked() {
                cancel = true;
            }
            ui.strong(if is_new_folder {
                "New Folder"
            } else {
                "Rename"
            });
            ui.separator();
            if editing {
                let text_resp = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .hint_text("Name")
                        .desired_width(220.0),
                );
                field_id = Some(text_resp.id);
                row_ids.push(text_resp.id);
                if text_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
            } else {
                let label = if text.is_empty() {
                    "Name".to_string()
                } else {
                    text.clone()
                };
                let field = Self::action_badge(self.badge_min_height(), ui, label)
                    .on_hover_text("Press A (or click) to edit the name");
                first_id.get_or_insert(field.id);
                field_id = Some(field.id);
                row_ids.push(field.id);
                if field.clicked() {
                    self.rename_editing = true;
                    self.rename_focus_pending = true;
                }
            }
            ui.separator();
            let apply_btn = Self::action_badge(
                self.badge_min_height(),
                ui,
                if is_new_folder { "Create" } else { "Rename" },
            );
            row_ids.push(apply_btn.id);
            if apply_btn.clicked() {
                apply = true;
            }
            let cancel_btn = Self::action_badge(self.badge_min_height(), ui, "Cancel");
            row_ids.push(cancel_btn.id);
            if cancel_btn.clicked() {
                cancel = true;
            }
        });
        self.right_focus_rows.push(row_ids);
        if focus_pending && let Some(id) = field_id {
            self.set_focus(ui.ctx(), id);
            self.rename_focus_pending = false;
        }
        let trimmed = text.trim().to_string();
        if let Some(editor) = &mut self.rename_editor {
            editor.text = text;
        }
        if apply && trimmed.is_empty() {
            eprintln!("name can't be empty");
        } else if apply {
            match &target {
                Some(old_path) => {
                    let new_path = old_path.with_file_name(&trimmed);
                    if new_path.exists() {
                        eprintln!("rename failed: {} already exists", new_path.display());
                    } else if let Err(e) = std::fs::rename(old_path, &new_path) {
                        eprintln!("rename failed: {e}");
                    } else {
                        if self.selected.as_deref() == Some(old_path.as_path()) {
                            self.set_selected(Some(new_path));
                        }
                        self.rename_editor = None;
                        self.context_menu_open = false;
                        self.refresh();
                    }
                }
                None => {
                    let new_dir = self.current_dir.join(&trimmed);
                    if let Err(e) = std::fs::create_dir(&new_dir) {
                        eprintln!("create folder failed: {e}");
                    } else {
                        self.rename_editor = None;
                        self.context_menu_open = false;
                        self.refresh();
                        self.set_selected(Some(new_dir));
                    }
                }
            }
        } else if cancel {
            self.rename_editor = None;
        }
        // Covers every path above that can close the zone (apply success,
        // cancel) in one place, including a mouse click on Close/Cancel
        // while mid-edit — without this, a stale `true` here would make
        // the *next* Rename/New Folder open straight into edit mode again,
        // exactly the auto-focus behavior this whole two-stage design
        // exists to avoid.
        if self.rename_editor.is_none() {
            self.rename_editing = false;
        }
        if self.focus_first_action
            && let Some(id) = first_id
        {
            ui.ctx().memory_mut(|m| m.request_focus(id));
            self.focus_first_action = false;
        }
    }

    fn show_permission_editor(&mut self, ui: &mut egui::Ui) {
        let Some(editor) = &mut self.perm_editor else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let mut mode = editor.mode;
        let paths = editor.paths.clone();
        let mut first_id = None;
        let mut row_ids: Vec<egui::Id> = Vec::new();
        ui.horizontal_wrapped(|ui| {
            let close = Self::action_badge(self.badge_min_height(), ui, self.icon(ICON_CLOSE));
            row_ids.push(close.id);
            if close.clicked() {
                cancel = true;
            }
            ui.strong("Permissions");
            if let [path] = paths.as_slice() {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                ui.weak(name).on_hover_text(path.to_string_lossy());
            } else {
                ui.weak(format!("{} items", paths.len()));
            }
            ui.separator();
            for (row_label, shift) in [("Owner", 6), ("Group", 3), ("Other", 0)] {
                ui.label(row_label);
                for (bit, letter) in [(0o4u32, "R"), (0o2, "W"), (0o1, "X")] {
                    let mask = bit << shift;
                    let mut checked = mode & mask != 0;
                    let cb = ui.checkbox(&mut checked, letter);
                    row_ids.push(cb.id);
                    if cb.changed() {
                        if checked {
                            mode |= mask;
                        } else {
                            mode &= !mask;
                        }
                    }
                }
            }
            ui.separator();
            let apply_btn = Self::action_badge(self.badge_min_height(), ui, "Apply");
            first_id.get_or_insert(apply_btn.id);
            row_ids.push(apply_btn.id);
            if apply_btn.clicked() {
                apply = true;
            }
            let cancel_btn = Self::action_badge(self.badge_min_height(), ui, "Cancel");
            row_ids.push(cancel_btn.id);
            if cancel_btn.clicked() {
                cancel = true;
            }
        });
        self.right_focus_rows.push(row_ids);
        if apply {
            for path in &paths {
                if let Err(e) = permissions::set_mode(path, mode) {
                    eprintln!("chmod failed for {}: {e}", path.display());
                }
            }
            self.perm_editor = None;
        } else if cancel {
            self.perm_editor = None;
        } else {
            self.perm_editor.as_mut().unwrap().mode = mode;
        }
        if self.focus_first_action
            && let Some(id) = first_id
        {
            ui.ctx().memory_mut(|m| m.request_focus(id));
            self.focus_first_action = false;
        }
    }

    fn show_progress_zone(&self, ui: &mut egui::Ui) {
        for job in &self.jobs {
            ui.horizontal(|ui| {
                ui.strong("Progress");
                if let Some(err) = &job.error {
                    ui.colored_label(egui::Color32::RED, format!("{}: {err}", job.label));
                } else {
                    ui.label(&job.label);
                    let fraction = job.done as f32 / job.total as f32;
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .show_percentage()
                            .desired_width(200.0),
                    );
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool, size: u64, modified_secs: u64) -> Entry {
        Entry {
            name: name.to_string(),
            path: PathBuf::from(name),
            is_dir,
            size,
            modified: Some(std::time::UNIX_EPOCH + Duration::from_secs(modified_secs)),
        }
    }

    #[test]
    fn dirs_always_sort_before_files_regardless_of_criteria_or_direction() {
        let dir = entry("zzz_dir", true, 999, 0);
        let file = entry("aaa_file", false, 1, 100);
        for sort_by in [SortBy::Name, SortBy::Size, SortBy::Modified] {
            for ascending in [true, false] {
                assert_eq!(
                    compare_entries(&dir, &file, sort_by, ascending),
                    std::cmp::Ordering::Less
                );
            }
        }
    }

    #[test]
    fn sorts_by_size_within_files() {
        let small = entry("a", false, 10, 0);
        let big = entry("b", false, 100, 0);
        assert_eq!(
            compare_entries(&small, &big, SortBy::Size, true),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_entries(&small, &big, SortBy::Size, false),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn sorts_by_modified_within_files() {
        let older = entry("a", false, 0, 100);
        let newer = entry("b", false, 0, 200);
        assert_eq!(
            compare_entries(&older, &newer, SortBy::Modified, true),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_entries(&older, &newer, SortBy::Modified, false),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn sort_by_cycles_through_all_three_and_back() {
        assert_eq!(SortBy::Name.next(), SortBy::Size);
        assert_eq!(SortBy::Size.next(), SortBy::Modified);
        assert_eq!(SortBy::Modified.next(), SortBy::Name);
    }
}
