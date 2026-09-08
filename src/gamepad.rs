use gilrs::{Axis, Button, EventType, Gilrs};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Delay before a held direction starts auto-repeating.
const REPEAT_DELAY: Duration = Duration::from_millis(400);
/// Interval between repeats once auto-repeat has kicked in.
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);
const STICK_THRESHOLD: f32 = 0.5;
/// How long Y must be held before it opens Search instead of Refresh.
const SEARCH_HOLD: Duration = Duration::from_secs(3);

pub enum Action {
    Move(egui::FocusDirection),
    /// Simulates pressing the currently-focused widget (egui treats
    /// Space/Enter as a click on whatever has keyboard focus).
    Activate,
    /// L3 (stick click) — toggle multi-select mode on/off.
    ToggleMultiSelect,
    /// Esc — closes whichever menu/dialog is open, or cancels multi-select,
    /// in priority order; falls back to going up a directory if none of
    /// those apply.
    Back,
    /// Reveal the actions panel and move focus into it.
    ContextMenu,
    /// LB/RB — swap focus horizontally between the file-list/right pane,
    /// reusing the same directional focus egui already supports.
    SwapPaneLeft,
    SwapPaneRight,
    /// Select — toggle the preview pane's width between 40% of the window
    /// and its normal size.
    TogglePreviewWidth,
    /// Y tap — refresh whichever pane is active (Sidebar's places/mounts,
    /// or the main file/trash list); a no-op elsewhere.
    Refresh,
    /// Y held 3s — open/focus the in-folder search box (also triggers any
    /// on-screen keyboard the session provides, via egui's normal IME
    /// request when a text field gains focus).
    Search,
    ScaleDown,
    ScaleUp,
    /// Start — open the selected file directly, bypassing the actions
    /// panel. Deliberately the *only* thing Start does — holding it was
    /// tried for `Quit` and turned out to trigger Steam Input's own
    /// mouse/gamepad-mode remapping regardless of what BrowDeck does in
    /// response, so Start can't be repurposed for anything beyond a
    /// plain tap.
    Open,
    /// Right stick — continuous scroll, sent every frame it's tilted past
    /// the deadzone (magnitude/direction, not an edge-triggered move).
    Scroll(f32),
    /// R3 — asks for confirmation before closing the app (no keyboard
    /// Alt+F4 to rely on under gamescope/Game Mode). A plain press, not
    /// held — the confirmation strip it opens (Quit/Cancel) is already
    /// the safety gate, no separate hold timer needed on top of it.
    Quit,
}

pub struct GamepadInput {
    gilrs: Gilrs,
    held_directions: HashMap<Button, Instant>,
    last_repeat: HashMap<Button, Instant>,
    right_stick_y: f32,
    /// Left stick is a plain analog equivalent of the d-pad — same
    /// edge-triggered `Move` action, not restricted to any one pane.
    left_stick_direction: Option<egui::FocusDirection>,
    north_pressed_at: Option<Instant>,
    north_hold_fired: bool,
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
                left_stick_direction: None,
                north_pressed_at: None,
                north_hold_fired: false,
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
                EventType::ButtonPressed(Button::North, _) => {
                    self.north_pressed_at = Some(Instant::now());
                    self.north_hold_fired = false;
                }
                EventType::ButtonReleased(Button::North, _) => {
                    if let Some(pressed_at) = self.north_pressed_at.take()
                        && !self.north_hold_fired
                        && pressed_at.elapsed() < SEARCH_HOLD
                    {
                        actions.push(Action::Refresh);
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
        if let Some(pressed_at) = self.north_pressed_at
            && !self.north_hold_fired
            && pressed_at.elapsed() >= SEARCH_HOLD
        {
            actions.push(Action::Search);
            self.north_hold_fired = true;
        }
        actions
    }

    fn handle_axis(&mut self, axis: Axis, value: f32, actions: &mut Vec<Action>) {
        let dir = match axis {
            Axis::LeftStickX if value > STICK_THRESHOLD => Some(egui::FocusDirection::Right),
            Axis::LeftStickX if value < -STICK_THRESHOLD => Some(egui::FocusDirection::Left),
            Axis::LeftStickY if value > STICK_THRESHOLD => Some(egui::FocusDirection::Up),
            Axis::LeftStickY if value < -STICK_THRESHOLD => Some(egui::FocusDirection::Down),
            Axis::LeftStickX | Axis::LeftStickY => None,
            _ => return,
        };
        if dir != self.left_stick_direction {
            self.left_stick_direction = dir;
            if let Some(dir) = dir {
                actions.push(Action::Move(dir));
            }
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
        Button::South => Some(Action::Activate),
        Button::East => Some(Action::Back),
        Button::West => Some(Action::ContextMenu),
        Button::Start => Some(Action::Open),
        Button::Select => Some(Action::TogglePreviewWidth),
        Button::LeftTrigger => Some(Action::SwapPaneLeft),
        Button::RightTrigger => Some(Action::SwapPaneRight),
        Button::LeftTrigger2 => Some(Action::ScaleDown),
        Button::RightTrigger2 => Some(Action::ScaleUp),
        Button::LeftThumb => Some(Action::ToggleMultiSelect),
        Button::RightThumb => Some(Action::Quit),
        _ => None,
    }
}
