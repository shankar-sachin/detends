//! Translating winit input into the shell's vocabulary.
//!
//! The only place in détends that knows winit's key representation exists.

pub use detends_shell::input::{Event, Key, Modifiers, MouseButton};

pub fn from_winit(key: &winit::keyboard::Key) -> Key {
    use winit::keyboard::{Key as WKey, NamedKey};

    match key {
        WKey::Named(NamedKey::Space) => Key::Space,
        WKey::Named(NamedKey::Escape) => Key::Escape,
        WKey::Named(NamedKey::Enter) => Key::Enter,
        WKey::Named(NamedKey::Backspace) => Key::Backspace,
        WKey::Named(NamedKey::Tab) => Key::Tab,
        WKey::Named(NamedKey::ArrowLeft) => Key::Left,
        WKey::Named(NamedKey::ArrowRight) => Key::Right,
        WKey::Named(NamedKey::ArrowUp) => Key::Up,
        WKey::Named(NamedKey::ArrowDown) => Key::Down,
        WKey::Character(s) => match s.chars().next() {
            Some(c @ '1'..='5') => Key::Mode(c as u8 - b'0'),
            Some(c) => Key::Character(c),
            None => Key::Other,
        },
        _ => Key::Other,
    }
}

pub fn button_from_winit(button: winit::event::MouseButton) -> Option<MouseButton> {
    use winit::event::MouseButton as W;
    Some(match button {
        W::Left => MouseButton::Left,
        W::Right => MouseButton::Right,
        W::Middle => MouseButton::Middle,
        _ => return None,
    })
}
