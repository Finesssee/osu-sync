//! Event handling for keyboard input

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

/// Poll for keyboard events with a timeout
pub fn poll(timeout: Duration) -> std::io::Result<Option<KeyEvent>> {
    if event::poll(timeout)? {
        if let Event::Key(key) = event::read()? {
            // Ignore key release events on Windows
            if key.kind == event::KeyEventKind::Press {
                return Ok(Some(key));
            }
        }
    }
    Ok(None)
}

/// Check if a key event is a quit command
pub fn is_quit(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Char('q' | 'Q'),
            modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
            ..
        } | KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
    )
}

/// Check if a key event is an escape/back command
pub fn is_escape(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc)
}

/// Check if a key event is navigation down
pub fn is_down(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Down | KeyCode::Char('j' | 'J'))
}

/// Check if a key event is navigation up
pub fn is_up(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Up | KeyCode::Char('k' | 'K'))
}

/// Check if a key event is enter/select
pub fn is_enter(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Enter)
}

/// Check if a key event is space (toggle)
pub fn is_space(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char(' '))
}

/// Check if a key event is tab
pub fn is_tab(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Tab)
}

/// Check if a key event is navigation left
pub fn is_left(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Left | KeyCode::Char('h' | 'H'))
}

/// Check if a key event is navigation right
pub fn is_right(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Right | KeyCode::Char('l' | 'L'))
}

/// Check if a key event is a specific character (case-insensitive)
pub fn is_key(key: &KeyEvent, c: char) -> bool {
    matches!(key.code, KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&c))
}

/// Check if a key event is page down
pub fn is_page_down(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::PageDown)
}

/// Check if a key event is page up
pub fn is_page_up(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::PageUp)
}

/// Check if a key event is a help command (? or h)
pub fn is_help(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Char('?'),
            modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
            ..
        } | KeyEvent {
            code: KeyCode::Char('h' | 'H'),
            modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
            ..
        }
    )
}

/// Check if a key event is Ctrl+A (select all)
pub fn is_ctrl_a(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
    )
}

/// Check if a key event is Ctrl+D (deselect all)
pub fn is_ctrl_d(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Char('d'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
    )
}

/// Check if a key event is Ctrl+I (invert selection)
pub fn is_ctrl_i(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Char('i'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
    )
}
