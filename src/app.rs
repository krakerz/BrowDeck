use crate::{deleted, fileops, gamepad, mounts, places};
use egui_material_icons::icons::{
    ICON_ARROW_UPWARD, ICON_CONTENT_COPY, ICON_CONTENT_CUT, ICON_CONTENT_PASTE, ICON_DELETE,
    ICON_DESCRIPTION, ICON_FOLDER, ICON_MENU, ICON_REFRESH, ICON_RESTORE_FROM_TRASH,
};
use std::path::PathBuf;
use std::time::Duration;

struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

struct Clipboard {
    path: PathBuf,
    cut: bool,
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
}

/// How long a finished copy/move job stays visible in the overlay before
/// auto-closing.
const JOB_LINGER: Duration = Duration::from_secs(2);

impl BrowDeckApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_material_icons::initialize(&cc.egui_ctx);
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
        };
        app.refresh();
        app
    }

    fn navigate_to(&mut self, dir: PathBuf) {
        self.current_dir = dir;
        self.view = View::Dir;
        self.selected = None;
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
        self.selected = None;
        self.trash_entries = deleted::list_trash();
    }

    /// Up a directory, or back out of the trash view — shared by the
    /// toolbar "Up" button and the gamepad's East/Back button.
    fn go_back(&mut self) {
        match self.view {
            View::Trash => {
                self.view = View::Dir;
                self.selected = None;
            }
            View::Dir => {
                if let Some(parent) = self.current_dir.parent() {
                    self.navigate_to(parent.to_path_buf());
                }
            }
        }
    }

    fn delete_selected(&mut self) {
        let Some(path) = self.selected.take() else {
            return;
        };
        if let Err(e) = trash::delete(&path) {
            eprintln!("delete failed: {e}");
        }
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
                }
            }
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button(ICON_MENU).clicked() {
                    self.sidebar_open = !self.sidebar_open;
                }
                if ui.button((ICON_ARROW_UPWARD, "Up")).clicked() {
                    self.go_back();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new((ICON_CONTENT_COPY, "Copy")),
                    )
                    .clicked()
                    && let Some(path) = self.selected.clone()
                {
                    self.clipboard = Some(Clipboard { path, cut: false });
                }
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new((ICON_CONTENT_CUT, "Cut")),
                    )
                    .clicked()
                    && let Some(path) = self.selected.clone()
                {
                    self.clipboard = Some(Clipboard { path, cut: true });
                }
                if ui
                    .add_enabled(
                        self.clipboard.is_some() && matches!(self.view, View::Dir),
                        egui::Button::new((ICON_CONTENT_PASTE, "Paste")),
                    )
                    .clicked()
                {
                    self.paste();
                }
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new((ICON_DELETE, "Delete")),
                    )
                    .clicked()
                {
                    self.delete_selected();
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
                        .selectable_label(false, (place.icon, place.label.as_str()))
                        .clicked()
                    {
                        clicked_place = Some(place.path.clone());
                    }
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.heading("Mounts");
                    if ui.small_button(ICON_REFRESH).clicked() {
                        self.mounts = mounts::list_mounts();
                    }
                });
                if self.mounts.is_empty() {
                    ui.weak("(none)");
                }
                let mut clicked_mount = None;
                for mount in &self.mounts {
                    let mount_label = mount.mount_point.to_string_lossy().into_owned();
                    let response = ui
                        .selectable_label(false, (mount.icon(), mount_label.as_str()))
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
                    .selectable_label(matches!(self.view, View::Trash), (ICON_DELETE, "Trash"))
                    .clicked()
                {
                    self.open_trash();
                }
            });
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
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut next_dir = None;
            let mut open_file = None;
            for entry in &self.entries {
                let icon = if entry.is_dir {
                    ICON_FOLDER
                } else {
                    ICON_DESCRIPTION
                };
                let is_selected = self.selected.as_deref() == Some(entry.path.as_path());
                let response = ui.selectable_label(is_selected, (icon, entry.name.as_str()));
                if response.clicked() {
                    self.selected = Some(entry.path.clone());
                    if entry.is_dir {
                        // Single click/gamepad-activate enters a directory —
                        // double-click is reserved for opening files (mouse).
                        next_dir = Some(entry.path.clone());
                    }
                }
                if response.double_clicked() && !entry.is_dir {
                    open_file = Some(entry.path.clone());
                }
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
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut restore_idx = None;
            for (i, entry) in self.trash_entries.iter().enumerate() {
                let label = format!(
                    "{}  (from {})",
                    entry.name,
                    entry.original_path.to_string_lossy()
                );
                if ui
                    .selectable_label(false, (ICON_RESTORE_FROM_TRASH, label))
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
