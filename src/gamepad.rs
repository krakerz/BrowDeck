use gilrs::{Axis, Button, EventType, Gilrs};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Delay before a held direction starts auto-repeating.
const REPEAT_DELAY: Duration = Duration::from_millis(400);
/// Interval between repeats once auto-repeat has kicked in.
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);
const STICK_THRESHOLD: f32 = 0.5;
/// How long South must be held before it enters multi-select instead of
/// acting as a normal tap-to-activate.
const MULTI_SELECT_HOLD: Duration = Duration::from_secs(3);

pub enum Action {
    Move(egui::FocusDirection),
    /// Simulates pressing the currently-focused widget (egui treats
    /// Space/Enter as a click on whatever has keyboard focus). Only sent
    /// on a quick South tap — a 3s hold sends [`Action::EnterMultiSelect`]
    /// instead.
    Activate,
    /// South held for 3s — enter multi-select mode.
    EnterMultiSelect,
    /// Esc — closes whichever menu/dialog is open, or cancels multi-select,
    /// in priority order.
    Back,
    ToggleSidebar,
    /// Reveal the actions panel and move focus into it.
    ContextMenu,
    /// LB/RB (and the left stick tilted left/right) — swap focus
    /// horizontally between the file-list/right pane, reusing the same
    /// directional focus egui already supports.
    SwapPaneLeft,
    SwapPaneRight,
    /// Select — jump focus to the top toolbar.
    FocusTop,
    /// Y — open/focus the in-folder search box (also triggers any
    /// on-screen keyboard the session provides, via egui's normal IME
    /// request when a text field gains focus).
    Search,
    ScaleDown,
    ScaleUp,
    /// L3 (stick click) — always goes up a directory, regardless of open
    /// menus. Distinct from the left stick's *tilt*, which drives sidebar
    /// navigation instead (see [`Action::SidebarMove`]).
    UpDirectory,
    /// R3 — open the selected file directly, bypassing the actions panel.
    Open,
    /// Right stick — continuous scroll, sent every frame it's tilted past
    /// the deadzone (magnitude/direction, not an edge-triggered move).
    Scroll(f32),
    /// Left stick tilted up/down — dedicated sidebar-row navigation (+1
    /// down, -1 up), so you can change folder there without needing to
    /// swap panes first. D-pad still drives whichever pane has focus.
    SidebarMove(i32),
}

pub struct GamepadInput {
    gilrs: Gilrs,
    held_directions: HashMap<Button, Instant>,
    last_repeat: HashMap<Button, Instant>,
    right_stick_y: f32,
    left_stick_x_dir: Option<bool>,
    left_stick_y_dir: Option<i32>,
    south_pressed_at: Option<Instant>,
    south_hold_fired: bool,
}

impl GamepadInput {
    /// `None` if no gamepad backend is available on this system — the app
    /// still runs fine on keyboard/mouse in that case.
    pub fn new() -> Option<Self> {
        match Gilrs::new() {
            Ok(gilrs) => Some(Self {
                gilrs,
                held_directions: HashMap::new(),
                last_repeat: HashMap::new(),
                right_stick_y: 0.0,
                left_stick_x_dir: None,
                left_stick_y_dir: None,
                south_pressed_at: None,
                south_hold_fired: false,
            }),
            Err(e) => {
                eprintln!("gamepad input unavailable: {e}");
                None
            }
        }
    }

    /// Whether an actual gamepad is plugged in right now (as opposed to
    /// just `gilrs` having initialized successfully) — used to gate
    /// gamepad-only UI like button-glyph hints.
    pub fn has_connected_pad(&self) -> bool {
        self.gilrs.gamepads().next().is_some()
    }

    pub fn poll(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        while let Some(event) = self.gilrs.next_event() {
            match event.event {
                EventType::ButtonPressed(Button::South, _) => {
                    self.south_pressed_at = Some(Instant::now());
                    self.south_hold_fired = false;
                }
                EventType::ButtonReleased(Button::South, _) => {
                    if let Some(pressed_at) = self.south_pressed_at.take()
                        && !self.south_hold_fired
                        && pressed_at.elapsed() < MULTI_SELECT_HOLD
                    {
                        actions.push(Action::Activate);
                    }
                }
                EventType::ButtonPressed(button, _) => {
                    if let Some(dir) = direction_for(button) {
                        actions.push(Action::Move(dir));
                        let now = Instant::now();
                        self.held_directions.insert(button, now);
                        self.last_repeat.insert(button, now);
                    } else if let Some(action) = action_for(button) {
                        actions.push(action);
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    self.held_directions.remove(&button);
                    self.last_repeat.remove(&button);
                }
                EventType::AxisChanged(axis, value, _) => {
                    if axis == Axis::RightStickY {
                        self.right_stick_y = value;
                    }
                    self.handle_axis(axis, value, &mut actions);
                }
                _ => {}
            }
        }
        self.apply_repeats(&mut actions);
        if self.right_stick_y.abs() > STICK_THRESHOLD {
            actions.push(Action::Scroll(self.right_stick_y));
        }
        if let Some(pressed_at) = self.south_pressed_at
            && !self.south_hold_fired
            && pressed_at.elapsed() >= MULTI_SELECT_HOLD
        {
            actions.push(Action::EnterMultiSelect);
            self.south_hold_fired = true;
        }
        actions
    }

    fn handle_axis(&mut self, axis: Axis, value: f32, actions: &mut Vec<Action>) {
        match axis {
            Axis::LeftStickX => {
                let dir = threshold_dir(value);
                if dir != self.left_stick_x_dir {
                    self.left_stick_x_dir = dir;
                    match dir {
                        Some(true) => actions.push(Action::SwapPaneRight),
                        Some(false) => actions.push(Action::SwapPaneLeft),
                        None => {}
                    }
                }
            }
            Axis::LeftStickY => {
                let dir = threshold_dir(value);
                if dir != self.left_stick_y_dir.map(|d| d > 0) {
                    self.left_stick_y_dir = dir.map(|up| if up { 1 } else { -1 });
                    match dir {
                        // Tilting up should move the sidebar cursor up (-1).
                        Some(true) => actions.push(Action::SidebarMove(-1)),
                        Some(false) => actions.push(Action::SidebarMove(1)),
                        None => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn apply_repeats(&mut self, actions: &mut Vec<Action>) {
        let now = Instant::now();
        for (button, pressed_at) in &self.held_directions {
            if now.duration_since(*pressed_at) < REPEAT_DELAY {
                continue;
            }
            let last = self.last_repeat.get(button).copied().unwrap_or(*pressed_at);
            if now.duration_since(last) >= REPEAT_INTERVAL
                && let Some(dir) = direction_for(*button)
            {
                actions.push(Action::Move(dir));
                self.last_repeat.insert(*button, now);
            }
        }
    }
}

fn threshold_dir(value: f32) -> Option<bool> {
    if value > STICK_THRESHOLD {
        Some(true)
    } else if value < -STICK_THRESHOLD {
        Some(false)
    } else {
        None
    }
}

fn direction_for(button: Button) -> Option<egui::FocusDirection> {
    match button {
        Button::DPadUp => Some(egui::FocusDirection::Up),
        Button::DPadDown => Some(egui::FocusDirection::Down),
        Button::DPadLeft => Some(egui::FocusDirection::Left),
        Button::DPadRight => Some(egui::FocusDirection::Right),
        _ => None,
    }
}

fn action_for(button: Button) -> Option<Action> {
    match button {
        Button::East => Some(Action::Back),
        Button::West => Some(Action::ContextMenu),
        Button::North => Some(Action::Search),
        Button::Start => Some(Action::ToggleSidebar),
        Button::Select => Some(Action::FocusTop),
        Button::LeftTrigger => Some(Action::SwapPaneLeft),
        Button::RightTrigger => Some(Action::SwapPaneRight),
        Button::LeftTrigger2 => Some(Action::ScaleDown),
        Button::RightTrigger2 => Some(Action::ScaleUp),
        Button::LeftThumb => Some(Action::UpDirectory),
        Button::RightThumb => Some(Action::Open),
        _ => None,
    }
}
