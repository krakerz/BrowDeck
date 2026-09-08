use crate::{deleted, fileops, fonts, gamepad, mounts, permissions, places, preview};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_ARROW_UPWARD, ICON_CLOSE, ICON_CONTENT_COPY, ICON_CONTENT_CUT, ICON_CONTENT_PASTE,
    ICON_DELETE, ICON_DELETE_SWEEP, ICON_DESCRIPTION, ICON_FOLDER, ICON_GAMEPAD, ICON_LOCK,
    ICON_MENU, ICON_OPEN_IN_NEW, ICON_PREVIEW, ICON_REFRESH, ICON_RESTORE_FROM_TRASH, ICON_SEARCH,
    ICON_UNARCHIVE, ICON_ZOOM_IN, ICON_ZOOM_OUT,
};
use std::path::PathBuf;
use std::time::Duration;

/// Base size (px) icons render at before `icon_scale` is applied.
const BASE_ICON_SIZE: f32 = 18.0;
const ICON_SCALE_STEP: f32 = 0.15;
const ICON_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.7..=2.5;
/// Points scrolled per frame per unit of right-stick tilt.
const SCROLL_SPEED: f32 = 12.0;

struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

struct Clipboard {
    paths: Vec<PathBuf>,
    cut: bool,
}

struct PermEditor {
    path: PathBuf,
    mode: u32,
}

enum View {
    Dir,
    Trash,
}

/// Which of the three left-to-right panes currently holds keyboard focus —
/// used only to draw a highlight border for gamepad/keyboard navigation.
#[derive(Clone, Copy, PartialEq)]
enum Pane {
    Sidebar,
    FileList,
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
    preview_enabled: bool,
    /// Gamepad Select toggles this — widens the right pane to 40% of the
    /// window instead of its normal (default/resizable) width.
    preview_wide: bool,
    /// Cached text-preview content, keyed by path so it's only re-read from
    /// disk when the selection actually changes, not every frame.
    preview_text: Option<(PathBuf, String)>,
    context_menu_open: bool,
    perm_editor: Option<PermEditor>,
    search_query: String,
    search_open: bool,
    /// One-shot flags consumed the next time the relevant widget is drawn.
    focus_search: bool,
    focus_first_action: bool,
    /// The hamburger button's id, refreshed every frame, so the gamepad's
    /// "Select" button can jump focus straight to the toolbar.
    top_focus_id: egui::Id,
    /// Last frame's pane rects, used to classify the currently-focused
    /// widget into a pane (one-frame-stale, imperceptible in practice).
    sidebar_rect: Option<egui::Rect>,
    central_rect: Option<egui::Rect>,
    right_rect: Option<egui::Rect>,
    /// Ids of last frame's file-list rows paired with their path — used to
    /// look up which entry is focused when toggling multi-select.
    entry_ids: Vec<(egui::Id, PathBuf)>,
    multi_select: bool,
    multi_selected: std::collections::HashSet<PathBuf>,
}

/// How long a finished copy/move job stays visible before auto-closing.
const JOB_LINGER: Duration = Duration::from_secs(2);

impl BrowDeckApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
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
            preview_enabled: false,
            preview_wide: false,
            preview_text: None,
            context_menu_open: false,
            perm_editor: None,
            search_query: String::new(),
            search_open: false,
            focus_search: false,
            focus_first_action: false,
            top_focus_id: egui::Id::new("toolbar_hamburger"),
            sidebar_rect: None,
            central_rect: None,
            right_rect: None,
            entry_ids: Vec::new(),
            multi_select: false,
            multi_selected: std::collections::HashSet::new(),
        };
        app.refresh();
        app
    }

    /// Renders a [`MaterialIcon`] at the current icon scale — use this
    /// instead of a bare `ICON_*` constant everywhere in the UI so the
    /// zoom +/- controls affect every icon consistently.
    fn icon(&self, icon: MaterialIcon) -> egui::RichText {
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
                Entry { name, path, is_dir }
            })
            .collect();
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        self.entries = entries;
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
        if self.multi_select {
            self.multi_select = false;
            self.multi_selected.clear();
        } else if self.context_menu_open {
            self.context_menu_open = false;
        } else if self.perm_editor.is_some() {
            self.perm_editor = None;
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
    fn ensure_focus_anchor(&self, ctx: &egui::Context) {
        if ctx.memory(|m| m.focused()).is_none() {
            // Prefer whatever's already selected (typically via mouse,
            // which doesn't grant egui keyboard focus on its own — see
            // NOTES.md) so a first d-pad/stick nudge continues on from
            // there instead of jumping to the top of the pane, which read
            // as the selection itself moving.
            let fallback = self
                .selected
                .as_ref()
                .and_then(|sel| self.entry_ids.iter().find(|(_, p)| p == sel))
                .or_else(|| self.entry_ids.first())
                .map(|(id, _)| *id)
                .unwrap_or(self.top_focus_id);
            ctx.memory_mut(|m| m.request_focus(fallback));
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

    /// Whether a real gamepad is connected — gates the bottom legend bar.
    fn has_connected_pad(&self) -> bool {
        self.gamepad.as_ref().is_some_and(|g| g.has_connected_pad())
    }

    /// Colored circular badge for a face button (A/B/X/Y), matching
    /// standard controller button colors.
    fn face_badge(ui: &mut egui::Ui, letter: &str, color: egui::Color32) {
        egui::Frame::default()
            .fill(color)
            .corner_radius(8.0)
            .inner_margin(egui::Margin {
                left: 6,
                right: 6,
                top: 1,
                bottom: 1,
            })
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(letter)
                        .strong()
                        .color(egui::Color32::WHITE)
                        .size(12.0),
                );
            });
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

    /// Persistent bottom control legend, shown only while a real gamepad is
    /// connected — mirrors the "always-visible button hints" bar in
    /// DeckCrate (this project's inspiration) rather than scattering hints
    /// next to individual buttons.
    fn show_gamepad_legend(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
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
            ui.label("Back/Up dir");
            Self::face_badge(ui, "X", egui::Color32::from_rgb(0x3D, 0x7E, 0xD6));
            ui.label("Actions");
            Self::face_badge(ui, "Y", egui::Color32::from_rgb(0xD9, 0xB4, 0x33));
            ui.label("Search");
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
        if let Some(path) = self.selected.clone()
            && !path.is_dir()
            && let Err(e) = open::that(&path)
        {
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
    /// frame's pane rects — used only to paint a focus-highlight border.
    fn focused_pane(&self, ctx: &egui::Context) -> Option<Pane> {
        let id = ctx.memory(|m| m.focused())?;
        let center = ctx.read_response(id)?.rect.center();
        if self.sidebar_rect.is_some_and(|r| r.contains(center)) {
            Some(Pane::Sidebar)
        } else if self.right_rect.is_some_and(|r| r.contains(center)) {
            Some(Pane::Right)
        } else if self.central_rect.is_some_and(|r| r.contains(center)) {
            Some(Pane::FileList)
        } else {
            None
        }
    }

    fn paint_pane_highlight(ui: &egui::Ui, active: bool) {
        if active {
            let mut stroke = ui.visuals().selection.stroke;
            stroke.width = stroke.width.max(2.0);
            ui.painter()
                .rect_stroke(ui.max_rect(), 0.0, stroke, egui::StrokeKind::Inside);
        }
    }

    /// Whether the actions panel has anything to show for the current view
    /// — used both to enable the gamepad's West/context-menu button and to
    /// gate right-click on empty space.
    fn actions_available(&self) -> bool {
        match self.view {
            View::Dir => {
                self.selected.is_some()
                    || self.clipboard.is_some()
                    || !self.multi_selected.is_empty()
            }
            View::Trash => self.selected_trash.is_some() || !self.trash_entries.is_empty(),
        }
    }
}

impl eframe::App for BrowDeckApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        for job in &mut self.jobs {
            job.poll();
        }
        let has_active_job = self.jobs.iter().any(|j| !j.finished);
        self.jobs
            .retain(|j| !j.finished || j.finished_at.is_none_or(|t| t.elapsed() < JOB_LINGER));
        if has_active_job || self.jobs.iter().any(|j| j.finished) {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        let focused_pane = self.focused_pane(ui.ctx());

        if let Some(gamepad) = &mut self.gamepad {
            let actions = gamepad.poll();
            // Keep polling at a game-loop-ish rate so held-direction repeat
            // and stick input feel responsive, not just event-driven.
            ui.ctx().request_repaint_after(Duration::from_millis(16));
            for action in actions {
                match action {
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
                        ui.ctx().memory_mut(|m| m.move_focus(dir));
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
                        if self.actions_available() {
                            self.context_menu_open = true;
                            self.focus_first_action = true;
                        }
                    }
                    gamepad::Action::SwapPaneLeft => {
                        self.ensure_focus_anchor(ui.ctx());
                        ui.ctx()
                            .memory_mut(|m| m.move_focus(egui::FocusDirection::Left));
                    }
                    gamepad::Action::SwapPaneRight => {
                        self.ensure_focus_anchor(ui.ctx());
                        ui.ctx()
                            .memory_mut(|m| m.move_focus(egui::FocusDirection::Right));
                    }
                    gamepad::Action::TogglePreviewWidth => {
                        self.preview_wide = !self.preview_wide;
                    }
                    gamepad::Action::Search => {
                        self.search_open = true;
                        self.focus_search = true;
                    }
                    gamepad::Action::ScaleDown => self.scale_down(),
                    gamepad::Action::ScaleUp => self.scale_up(),
                    gamepad::Action::Open => self.open_selected(),
                    gamepad::Action::Scroll(amount) => {
                        ui.ctx().input_mut(|i| {
                            i.events.push(egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, amount * SCROLL_SPEED),
                                phase: egui::TouchPhase::Move,
                                modifiers: egui::Modifiers::NONE,
                            });
                        });
                    }
                }
            }
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                let hamburger = ui.button(self.icon(ICON_MENU));
                if hamburger.clicked() {
                    self.sidebar_open = !self.sidebar_open;
                }
                self.top_focus_id = hamburger.id;
                if ui.button((self.icon(ICON_ARROW_UPWARD), "Up")).clicked() {
                    self.go_back();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        *ICON_SCALE_RANGE.start() < self.icon_scale,
                        egui::Button::new(self.icon(ICON_ZOOM_OUT)),
                    )
                    .on_hover_text("Smaller icons")
                    .clicked()
                {
                    self.scale_down();
                }
                if ui
                    .add_enabled(
                        self.icon_scale < *ICON_SCALE_RANGE.end(),
                        egui::Button::new(self.icon(ICON_ZOOM_IN)),
                    )
                    .on_hover_text("Bigger icons")
                    .clicked()
                {
                    self.scale_up();
                }
                ui.separator();
                if ui
                    .selectable_label(self.preview_enabled, (self.icon(ICON_PREVIEW), "Preview"))
                    .on_hover_text("Auto-preview images/text files on select")
                    .clicked()
                {
                    self.preview_enabled = !self.preview_enabled;
                    self.refresh_preview();
                }
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
        });

        if self.has_connected_pad() {
            egui::Panel::bottom("gamepad_legend")
                .exact_size(28.0)
                .show(ui, |ui| self.show_gamepad_legend(ui));
        }

        if self.sidebar_open {
            let resp = egui::Panel::left("sidebar").show(ui, |ui| {
                Self::paint_pane_highlight(ui, focused_pane == Some(Pane::Sidebar));
                ui.heading("Places");
                let mut clicked_place = None;
                for place in &self.places {
                    if ui
                        .selectable_label(false, (self.icon(place.icon), place.label.as_str()))
                        .clicked()
                    {
                        clicked_place = Some(place.path.clone());
                    }
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.heading("Mounts");
                    if ui.small_button(self.icon(ICON_REFRESH)).clicked() {
                        self.mounts = mounts::list_mounts();
                    }
                    ui.checkbox(&mut self.show_all_mounts, "Show all");
                });
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
                        .selectable_label(false, (self.icon(mount.icon()), mount_label.as_str()))
                        .on_hover_text(&mount.device);
                    if response.clicked() {
                        clicked_mount = Some(mount.mount_point.clone());
                    }
                }
                if let Some(dir) = clicked_place.or(clicked_mount) {
                    self.navigate_to(dir);
                }

                ui.add_space(8.0);
                if ui
                    .selectable_label(
                        matches!(self.view, View::Trash),
                        (self.icon(ICON_DELETE), "Trash"),
                    )
                    .clicked()
                {
                    self.open_trash();
                }
            });
            self.sidebar_rect = Some(resp.response.rect);
        } else {
            self.sidebar_rect = None;
        }

        let preview_visible = self.preview_enabled
            && self
                .selected
                .as_deref()
                .and_then(preview::classify)
                .is_some();
        let show_actions =
            self.context_menu_open || self.perm_editor.is_some() || !self.jobs.is_empty();
        // Only reserve the right pane's screen space when it actually has
        // something to show — otherwise it's just an empty strip.
        if preview_visible || show_actions {
            let mut right_panel = egui::Panel::right("right_pane");
            if self.preview_wide {
                right_panel = right_panel.exact_size(ui.ctx().content_rect().width() * 0.4);
            }
            let right_resp = right_panel.show(ui, |ui| {
                Self::paint_pane_highlight(ui, focused_pane == Some(Pane::Right));
                if preview_visible {
                    self.show_preview_panel(ui);
                }
                if show_actions {
                    if preview_visible {
                        ui.separator();
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("right_pane_lower")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.context_menu_open {
                                self.show_actions_zone(ui);
                                ui.add_space(8.0);
                            }
                            if self.perm_editor.is_some() {
                                self.show_permission_editor(ui);
                                ui.add_space(8.0);
                            }
                            if !self.jobs.is_empty() {
                                self.show_progress_zone(ui);
                            }
                        });
                }
            });
            self.right_rect = Some(right_resp.response.rect);
        } else {
            self.right_rect = None;
        }

        let central_resp = egui::CentralPanel::default().show(ui, |ui| {
            Self::paint_pane_highlight(ui, focused_pane == Some(Pane::FileList));
            match self.view {
                View::Dir => self.show_dir(ui),
                View::Trash => self.show_trash(ui),
            }
        });
        self.central_rect = Some(central_resp.response.rect);
    }
}

impl BrowDeckApp {
    fn show_dir(&mut self, ui: &mut egui::Ui) {
        if self.search_open {
            ui.horizontal(|ui| {
                ui.label(self.icon(ICON_SEARCH));
                let search_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Search this folder…")
                        .desired_width(f32::INFINITY),
                );
                if self.focus_search {
                    search_resp.request_focus();
                    self.focus_search = false;
                }
                if ui.button(self.icon(ICON_CLOSE)).clicked() {
                    self.search_open = false;
                    self.search_query.clear();
                }
            });
        } else if ui.button(self.icon(ICON_SEARCH)).clicked() {
            self.search_open = true;
            self.focus_search = true;
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("dir_list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let query = self.search_query.to_lowercase();
                let mut new_selection = None;
                let mut next_dir = None;
                let mut open_file = None;
                let mut empty_area_secondary_click = false;
                let mut entry_ids = Vec::new();
                for entry in &self.entries {
                    if !query.is_empty() && !entry.name.to_lowercase().contains(&query) {
                        continue;
                    }
                    let icon = if entry.is_dir {
                        ICON_FOLDER
                    } else {
                        ICON_DESCRIPTION
                    };
                    let is_selected = if self.multi_select {
                        self.multi_selected.contains(&entry.path)
                    } else {
                        self.selected.as_deref() == Some(entry.path.as_path())
                    };
                    let response =
                        ui.selectable_label(is_selected, (self.icon(icon), entry.name.as_str()));
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
            .show(ui, |ui| {
                let mut restore_idx = None;
                let mut empty_area_secondary_click = false;
                for (i, entry) in self.trash_entries.iter().enumerate() {
                    let label = format!(
                        "{}  (from {})",
                        entry.name,
                        entry.original_path.to_string_lossy()
                    );
                    let is_selected = self.selected_trash == Some(i);
                    let response = ui
                        .selectable_label(is_selected, (self.icon(ICON_RESTORE_FROM_TRASH), label));
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

    fn show_preview_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Preview");
        let Some(path) = self.selected.clone() else {
            return;
        };
        match preview::classify(&path) {
            Some(preview::Kind::Image) => {
                egui::ScrollArea::vertical()
                    .id_salt("preview_image")
                    .auto_shrink([false, false])
                    .max_height(ui.available_height() * 0.5)
                    .show(ui, |ui| {
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
                        .max_height(ui.available_height() * 0.5)
                        .show(ui, |ui| {
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
    fn show_actions_zone(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Actions");
            if ui.button(self.icon(ICON_CLOSE)).clicked() {
                self.context_menu_open = false;
            }
        });
        match self.view {
            View::Dir => self.show_dir_actions(ui),
            View::Trash => self.show_trash_actions(ui),
        }
    }

    fn show_dir_actions(&mut self, ui: &mut egui::Ui) {
        let paths = self.selected_paths();
        let single = (paths.len() == 1).then(|| paths[0].clone());
        if let Some(path) = &single {
            ui.label(path.to_string_lossy());
        } else if paths.len() > 1 {
            ui.label(format!("{} items selected", paths.len()));
        }
        let mut first_id = None;
        ui.horizontal_wrapped(|ui| {
            if self.clipboard.is_some() && self.jobs.is_empty() {
                let r = ui.button((self.icon(ICON_CONTENT_PASTE), "Paste"));
                first_id.get_or_insert(r.id);
                if r.clicked() {
                    self.paste();
                    // Deliberately left open: the copy/move progress zone
                    // stacks below Actions rather than replacing it.
                }
            }
            if let Some(path) = &single
                && !path.is_dir()
            {
                let r = ui.button((self.icon(ICON_OPEN_IN_NEW), "Open"));
                first_id.get_or_insert(r.id);
                if r.clicked() {
                    self.open_selected();
                    self.context_menu_open = false;
                }
            }
            if !paths.is_empty() {
                let r = ui.button((self.icon(ICON_CONTENT_COPY), "Copy"));
                first_id.get_or_insert(r.id);
                if r.clicked() {
                    self.clipboard = Some(Clipboard {
                        paths: paths.clone(),
                        cut: false,
                    });
                    self.context_menu_open = false;
                }
                let r = ui.button((self.icon(ICON_CONTENT_CUT), "Cut"));
                first_id.get_or_insert(r.id);
                if r.clicked() {
                    self.clipboard = Some(Clipboard {
                        paths: paths.clone(),
                        cut: true,
                    });
                    self.context_menu_open = false;
                }
                let r = ui.button((self.icon(ICON_DELETE), "Delete"));
                first_id.get_or_insert(r.id);
                if r.clicked() {
                    self.delete_paths(&paths);
                    self.context_menu_open = false;
                }
            }
            if let Some(path) = &single
                && fileops::is_archive(path)
                && self.jobs.is_empty()
            {
                let r = ui.button((self.icon(ICON_UNARCHIVE), "Extract"));
                first_id.get_or_insert(r.id);
                if r.clicked() {
                    let dest_dir = self.current_dir.clone();
                    self.jobs
                        .push(fileops::spawn_extract(path.clone(), dest_dir));
                    // Deliberately left open: the extract progress zone
                    // stacks below Actions rather than replacing it.
                }
            }
            if let Some(path) = &single {
                let r = ui.button((self.icon(ICON_LOCK), "Permissions"));
                first_id.get_or_insert(r.id);
                if r.clicked()
                    && let Some(mode) = permissions::read_mode(path)
                {
                    self.perm_editor = Some(PermEditor {
                        path: path.clone(),
                        mode,
                    });
                    self.focus_first_action = true;
                    // Deliberately left open: the permission editor stacks
                    // below Actions rather than replacing it.
                }
            }
        });
        if self.focus_first_action
            && let Some(id) = first_id
        {
            ui.ctx().memory_mut(|m| m.request_focus(id));
            self.focus_first_action = false;
        }
    }

    fn show_trash_actions(&mut self, ui: &mut egui::Ui) {
        let mut first_id = None;
        ui.horizontal_wrapped(|ui| {
            if let Some(i) = self.selected_trash {
                let r = ui.button((self.icon(ICON_RESTORE_FROM_TRASH), "Restore"));
                first_id.get_or_insert(r.id);
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
                let r = ui.button((self.icon(ICON_DELETE_SWEEP), "Empty All"));
                first_id.get_or_insert(r.id);
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
        let path = editor.path.clone();
        let mut first_id = None;
        ui.horizontal(|ui| {
            ui.heading("Permissions");
            let r = ui.button(self.icon(ICON_CLOSE));
            first_id.get_or_insert(r.id);
            if r.clicked() {
                cancel = true;
            }
        });
        ui.label(path.to_string_lossy());
        egui::Grid::new("perm_grid").show(ui, |ui| {
            ui.label("");
            ui.label("Read");
            ui.label("Write");
            ui.label("Execute");
            ui.end_row();
            for (row_label, shift) in [("Owner", 6), ("Group", 3), ("Other", 0)] {
                ui.label(row_label);
                for bit in [0o4u32, 0o2, 0o1] {
                    let mask = bit << shift;
                    let mut checked = mode & mask != 0;
                    if ui.checkbox(&mut checked, "").changed() {
                        if checked {
                            mode |= mask;
                        } else {
                            mode &= !mask;
                        }
                    }
                }
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                apply = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
        if apply {
            if let Err(e) = permissions::set_mode(&path, mode) {
                eprintln!("chmod failed: {e}");
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
        ui.heading("Progress");
        for job in &self.jobs {
            if let Some(err) = &job.error {
                ui.colored_label(egui::Color32::RED, format!("{}: {err}", job.label));
            } else {
                ui.label(&job.label);
                let fraction = job.done as f32 / job.total as f32;
                ui.add(egui::ProgressBar::new(fraction).show_percentage());
            }
        }
    }
}
