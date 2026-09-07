use gilrs::{Axis, Button, EventType, Gilrs};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Delay before a held direction starts auto-repeating.
const REPEAT_DELAY: Duration = Duration::from_millis(400);
/// Interval between repeats once auto-repeat has kicked in.
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);
const STICK_THRESHOLD: f32 = 0.5;

pub enum Action {
    Move(egui::FocusDirection),
    /// Simulates pressing the currently-focused widget (egui treats
    /// Space/Enter as a click on whatever has keyboard focus).
    Activate,
    /// Go up a directory / back out of the trash view.
    Back,
    ToggleSidebar,
}

pub struct GamepadInput {
    gilrs: Gilrs,
    held_directions: HashMap<Button, Instant>,
    last_repeat: HashMap<Button, Instant>,
    stick_direction: Option<egui::FocusDirection>,
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
                stick_direction: None,
            }),
            Err(e) => {
                eprintln!("gamepad input unavailable: {e}");
                None
            }
        }
    }

    pub fn poll(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        while let Some(event) = self.gilrs.next_event() {
            match event.event {
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
                    self.handle_axis(axis, value, &mut actions);
                }
                _ => {}
            }
        }
        self.apply_repeats(&mut actions);
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
        if dir != self.stick_direction {
            self.stick_direction = dir;
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
        Button::Start => Some(Action::ToggleSidebar),
        _ => None,
    }
}
