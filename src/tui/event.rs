use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    None,
}

pub fn poll_event(timeout: Duration) -> std::io::Result<AppEvent> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => Ok(AppEvent::Key(key)),
            Event::Resize(w, h) => Ok(AppEvent::Resize(w, h)),
            _ => Ok(AppEvent::None),
        }
    } else {
        Ok(AppEvent::None)
    }
}

/// Helper to check for quit keys.
pub fn is_quit(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q'))
        || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
}
