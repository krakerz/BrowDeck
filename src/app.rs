use crate::{deleted, fileops, fonts, gamepad, mounts, permissions, places, preview};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_ARROW_UPWARD, ICON_CLOSE, ICON_CONTENT_COPY, ICON_CONTENT_CUT, ICON_CONTENT_PASTE,
    ICON_DELETE, ICON_DESCRIPTION, ICON_FOLDER, ICON_LOCK, ICON_MENU, ICON_MORE_VERT,
    ICON_OPEN_IN_NEW, ICON_PREVIEW, ICON_REFRESH, ICON_RESTORE_FROM_TRASH, ICON_UNARCHIVE,
    ICON_ZOOM_IN, ICON_ZOOM_OUT,
};
use std::path::PathBuf;
use std::time::Duration;

/// Base size (px) icons render at before `icon_scale` is applied.
const BASE_ICON_SIZE: f32 = 18.0;
const ICON_SCALE_STEP: f32 = 0.15;
const ICON_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.7..=2.5;

struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

struct Clipboard {
    path: PathBuf,
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

pub struct BrowDeckApp {
    sidebar_open: bool,
    current_dir: PathBuf,
    entries: Vec<Entry>,
    view: View,
    trash_entries: Vec<deleted::TrashEntry>,
    places: Vec<places::Place>,
    mounts: Vec<mounts::Mount>,
    selected: Option<PathBuf>,
    clipboard: Option<Clipboard>,
    jobs: Vec<fileops::Job>,
    gamepad: Option<gamepad::GamepadInput>,
    icon_scale: f32,
    show_all_mounts: bool,
    preview_enabled: bool,
    /// Cached text-preview content, keyed by path so it's only re-read from
    /// disk when the selection actually changes, not every frame.
    preview_text: Option<(PathBuf, String)>,
    context_menu_open: bool,
    perm_editor: Option<PermEditor>,
}

/// How long a finished copy/move job stays visible in the overlay before
/// auto-closing.
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
            places: places::list_places(),
            mounts: mounts::list_mounts(),
            selected: None,
            clipboard: None,
            jobs: Vec::new(),
            gamepad: gamepad::GamepadInput::new(),
            icon_scale: 1.0,
            show_all_mounts: false,
            preview_enabled: false,
            preview_text: None,
            context_menu_open: false,
            perm_editor: None,
        };
        app.refresh();
        app
    }

    /// Renders a [`MaterialIcon`] at the current icon scale — use this
    /// instead of a bare `ICON_*` constant everywhere in the UI so the
    /// zoom +/- toolbar buttons affect every icon consistently.
    fn icon(&self, icon: MaterialIcon) -> egui::RichText {
        icon.rich_text().size(BASE_ICON_SIZE * self.icon_scale)
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
        self.set_selected(None);
        self.refresh();
    }

    fn refresh(&mut self) {
        let mut entries: Vec<Entry> = std::fs::read_dir(&self.current_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                let is_dir = e.file_type().ok()?.is_dir();
                Some(Entry { name, path, is_dir })
            })
            .collect();
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        self.entries = entries;
    }

    fn open_trash(&mut self) {
        self.view = View::Trash;
        self.set_selected(None);
        self.trash_entries = deleted::list_trash();
    }

    /// Up a directory, or back out of the trash view — shared by the
    /// toolbar "Up" button and the gamepad's East/Back button.
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

    fn delete_selected(&mut self) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        if let Err(e) = trash::delete(&path) {
            eprintln!("delete failed: {e}");
        }
        self.set_selected(None);
        self.refresh();
    }

    fn paste(&mut self) {
        let Some(clip) = self.clipboard.take() else {
            return;
        };
        let sources = vec![clip.path];
        let job = if clip.cut {
            fileops::spawn_move(sources, self.current_dir.clone())
        } else {
            fileops::spawn_copy(sources, self.current_dir.clone())
        };
        self.jobs.push(job);
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

        if let Some(gamepad) = &mut self.gamepad {
            let actions = gamepad.poll();
            // Keep polling at a game-loop-ish rate so held-direction repeat
            // and stick input feel responsive, not just event-driven.
            ui.ctx().request_repaint_after(Duration::from_millis(16));
            for action in actions {
                match action {
                    gamepad::Action::Move(dir) => ui.ctx().memory_mut(|m| m.move_focus(dir)),
                    gamepad::Action::Activate => {
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
                    gamepad::Action::Back => self.go_back(),
                    gamepad::Action::ToggleSidebar => self.sidebar_open = !self.sidebar_open,
                    gamepad::Action::ContextMenu => {
                        if self.selected.is_some() {
                            self.context_menu_open = !self.context_menu_open;
                        }
                    }
                }
            }
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button(self.icon(ICON_MENU)).clicked() {
                    self.sidebar_open = !self.sidebar_open;
                }
                if ui.button((self.icon(ICON_ARROW_UPWARD), "Up")).clicked() {
                    self.go_back();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new((self.icon(ICON_CONTENT_COPY), "Copy")),
                    )
                    .clicked()
                    && let Some(path) = self.selected.clone()
                {
                    self.clipboard = Some(Clipboard { path, cut: false });
                }
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new((self.icon(ICON_CONTENT_CUT), "Cut")),
                    )
                    .clicked()
                    && let Some(path) = self.selected.clone()
                {
                    self.clipboard = Some(Clipboard { path, cut: true });
                }
                if ui
                    .add_enabled(
                        self.clipboard.is_some() && matches!(self.view, View::Dir),
                        egui::Button::new((self.icon(ICON_CONTENT_PASTE), "Paste")),
                    )
                    .clicked()
                {
                    self.paste();
                }
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new((self.icon(ICON_DELETE), "Delete")),
                    )
                    .clicked()
                {
                    self.delete_selected();
                }
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new(self.icon(ICON_MORE_VERT)),
                    )
                    .on_hover_text("More actions")
                    .clicked()
                {
                    self.context_menu_open = !self.context_menu_open;
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
                    self.icon_scale = (self.icon_scale - ICON_SCALE_STEP)
                        .clamp(*ICON_SCALE_RANGE.start(), *ICON_SCALE_RANGE.end());
                }
                if ui
                    .add_enabled(
                        self.icon_scale < *ICON_SCALE_RANGE.end(),
                        egui::Button::new(self.icon(ICON_ZOOM_IN)),
                    )
                    .on_hover_text("Bigger icons")
                    .clicked()
                {
                    self.icon_scale = (self.icon_scale + ICON_SCALE_STEP)
                        .clamp(*ICON_SCALE_RANGE.start(), *ICON_SCALE_RANGE.end());
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
            });
        });

        if self.sidebar_open {
            egui::Panel::left("sidebar").show(ui, |ui| {
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
                });
                ui.checkbox(&mut self.show_all_mounts, "Show all");
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
        }

        if self.context_menu_open {
            egui::Panel::bottom("context_menu")
                .exact_size(110.0)
                .show(ui, |ui| self.show_context_menu(ui));
        }
        self.show_permission_editor(ui.ctx());

        let show_preview = self.preview_enabled
            && self
                .selected
                .as_deref()
                .and_then(preview::classify)
                .is_some();
        if show_preview {
            egui::Panel::right("preview").show(ui, |ui| self.show_preview_panel(ui));
        }

        egui::CentralPanel::default().show(ui, |ui| match self.view {
            View::Dir => self.show_dir(ui),
            View::Trash => self.show_trash(ui),
        });

        self.show_progress_overlay(ui.ctx());
    }
}

impl BrowDeckApp {
    fn show_dir(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut new_selection = None;
                let mut next_dir = None;
                let mut open_file = None;
                for entry in &self.entries {
                    let icon = if entry.is_dir {
                        ICON_FOLDER
                    } else {
                        ICON_DESCRIPTION
                    };
                    let is_selected = self.selected.as_deref() == Some(entry.path.as_path());
                    let response =
                        ui.selectable_label(is_selected, (self.icon(icon), entry.name.as_str()));
                    if response.clicked() {
                        new_selection = Some(entry.path.clone());
                        if entry.is_dir {
                            // Single click/gamepad-activate enters a directory —
                            // double-click is reserved for opening files (mouse).
                            next_dir = Some(entry.path.clone());
                        }
                    }
                    if response.double_clicked() && !entry.is_dir {
                        open_file = Some(entry.path.clone());
                    }
                    if response.secondary_clicked() {
                        new_selection = Some(entry.path.clone());
                        self.context_menu_open = true;
                    }
                }
                if let Some(path) = new_selection {
                    self.set_selected(Some(path));
                }
                if let Some(dir) = next_dir {
                    self.navigate_to(dir);
                }
                if let Some(path) = open_file
                    && let Err(e) = open::that(&path)
                {
                    eprintln!("open failed: {e}");
                }
            });
    }

    fn show_trash(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut restore_idx = None;
                for (i, entry) in self.trash_entries.iter().enumerate() {
                    let label = format!(
                        "{}  (from {})",
                        entry.name,
                        entry.original_path.to_string_lossy()
                    );
                    if ui
                        .selectable_label(false, (self.icon(ICON_RESTORE_FROM_TRASH), label))
                        .double_clicked()
                    {
                        restore_idx = Some(i);
                    }
                }
                if let Some(i) = restore_idx {
                    let entry = &self.trash_entries[i];
                    if let Err(e) = deleted::restore(entry) {
                        eprintln!("restore failed: {e}");
                    }
                    self.trash_entries = deleted::list_trash();
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
                    .auto_shrink([false, false])
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
                        .auto_shrink([false, false])
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
    /// gamescope was the thing this app exists to not be).
    fn show_context_menu(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.selected.clone() else {
            self.context_menu_open = false;
            return;
        };
        ui.horizontal(|ui| {
            ui.heading("Actions");
            if ui.button(self.icon(ICON_CLOSE)).clicked() {
                self.context_menu_open = false;
            }
        });
        ui.label(path.to_string_lossy());
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if !path.is_dir() && ui.button((self.icon(ICON_OPEN_IN_NEW), "Open")).clicked() {
                if let Err(e) = open::that(&path) {
                    eprintln!("open failed: {e}");
                }
                self.context_menu_open = false;
            }
            if ui.button((self.icon(ICON_CONTENT_COPY), "Copy")).clicked() {
                self.clipboard = Some(Clipboard {
                    path: path.clone(),
                    cut: false,
                });
                self.context_menu_open = false;
            }
            if ui.button((self.icon(ICON_CONTENT_CUT), "Cut")).clicked() {
                self.clipboard = Some(Clipboard {
                    path: path.clone(),
                    cut: true,
                });
                self.context_menu_open = false;
            }
            if ui.button((self.icon(ICON_DELETE), "Delete")).clicked() {
                self.delete_selected();
                self.context_menu_open = false;
            }
            if fileops::is_archive(&path)
                && ui.button((self.icon(ICON_UNARCHIVE), "Extract")).clicked()
            {
                let dest_dir = self.current_dir.clone();
                self.jobs
                    .push(fileops::spawn_extract(path.clone(), dest_dir));
                self.context_menu_open = false;
            }
            if ui.button((self.icon(ICON_LOCK), "Permissions")).clicked() {
                if let Some(mode) = permissions::read_mode(&path) {
                    self.perm_editor = Some(PermEditor {
                        path: path.clone(),
                        mode,
                    });
                }
                self.context_menu_open = false;
            }
        });
    }

    fn show_permission_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.perm_editor else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let mut mode = editor.mode;
        let path = editor.path.clone();
        egui::Area::new(egui::Id::new("perm_editor"))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(280.0);
                    ui.horizontal(|ui| {
                        ui.heading("Change permissions");
                        if ui.button(self.icon(ICON_CLOSE)).clicked() {
                            cancel = true;
                        }
                    });
                    ui.label(path.to_string_lossy());
                    ui.separator();
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
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            apply = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
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
    }

    fn show_progress_overlay(&self, ctx: &egui::Context) {
        if self.jobs.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("job_overlay"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
            .show(ctx, |ui| {
                for job in &self.jobs {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(220.0);
                        if let Some(err) = &job.error {
                            ui.colored_label(egui::Color32::RED, format!("{}: {err}", job.label));
                        } else {
                            ui.label(&job.label);
                            let fraction = job.done as f32 / job.total as f32;
                            ui.add(egui::ProgressBar::new(fraction).show_percentage());
                        }
                    });
                }
            });
    }
}
