//! Input, in the shell's own vocabulary.
//!
//! The other half of the compositor seam. If the shell took `winit` events
//! directly it would be as coupled to the windowing system as it would be to
//! `wgpu` if it drew its own pixels — and under a Wayland compositor input
//! arrives from libinput, not winit, so that coupling would have to be undone
//! exactly when there is least appetite for it.
//!
//! The host translates into these types; nothing below this line knows what a
//! window system is.

/// A key, named by what it means rather than by scancode.
///
/// Only the keys détends actually binds. Anything else arrives as
/// [`Key::Character`], which is all that text entry needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    /// Digits 1–5, which select the five modes.
    Mode(u8),
    Space,
    Escape,
    Enter,
    Backspace,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Character(char),
    Other,
}

/// Held modifiers.
///
/// `sys` is the key détends calls **Super** — Command on macOS, the Windows or
/// Meta key elsewhere. Naming it by role keeps the shell's bindings identical
/// across platforms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub sys: bool,
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
}

impl Modifiers {
    pub fn none(&self) -> bool {
        !self.sys && !self.shift && !self.alt && !self.control
    }

    /// Super alone — the détends command modifier, with nothing else held.
    pub fn only_sys(&self) -> bool {
        self.sys && !self.shift && !self.alt && !self.control
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// One thing the user did.
#[derive(Clone, Debug)]
pub enum Event {
    KeyDown {
        key: Key,
        modifiers: Modifiers,
    },
    KeyUp {
        key: Key,
        modifiers: Modifiers,
    },
    /// Cursor position in logical units.
    PointerMoved {
        x: f32,
        y: f32,
    },
    PointerDown {
        x: f32,
        y: f32,
        button: MouseButton,
    },
    PointerUp {
        x: f32,
        y: f32,
        button: MouseButton,
    },
    Scroll {
        dx: f32,
        dy: f32,
    },
    /// The workspace changed size, in logical units.
    Resized {
        width: f32,
        height: f32,
    },
    /// The platform's light/dark preference changed.
    AppearanceChanged {
        prefers_dark: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_sys_rejects_extra_modifiers() {
        // Super+Space opens Search; Super+Shift+Space must not, or the
        // screenshot chords would collide with it.
        let plain = Modifiers {
            sys: true,
            ..Default::default()
        };
        assert!(plain.only_sys());

        let shifted = Modifiers {
            sys: true,
            shift: true,
            ..Default::default()
        };
        assert!(!shifted.only_sys());
        assert!(!shifted.none());
    }

    #[test]
    fn no_modifiers_is_not_the_system_modifier() {
        assert!(Modifiers::default().none());
        assert!(!Modifiers::default().only_sys());
    }
}
