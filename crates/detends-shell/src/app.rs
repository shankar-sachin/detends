//! What détends runs.
//!
//! This replaces the five modes. Modes were *environments* — one at a time,
//! filling the workspace, switched rather than opened. Apps are things you open
//! and close, several at once, each in a window you can move.
//!
//! The thing that does not change is restraint. This is not a system where
//! anyone installs anything: it is a fixed, small set of apps that détends
//! ships, and adding one is a decision rather than a download. A dock of six is
//! a different object from a dock of sixty.

use detends_paint::IconShape;

/// One of the apps détends ships.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum App {
    /// The browser. Called Surf everywhere the user can see.
    Surf,
    Spotify,
    Files,
    Clock,
    Mail,
    Studio,
    Settings,
}

impl App {
    /// The order they sit in the dock.
    ///
    /// Surf first because it is the one that gets opened most in a system that
    /// has a browser at all, then the rest in the order they were built.
    pub const ALL: [App; 7] = [
        App::Surf,
        App::Spotify,
        App::Files,
        App::Clock,
        App::Mail,
        App::Studio,
        App::Settings,
    ];

    /// What it is called on screen.
    ///
    /// The browser's full name is détends Surf, and nowhere in the interface
    /// says so: a product that introduces itself by its full name every time
    /// you look at it is a product that thinks about itself more than you do.
    pub fn name(self) -> &'static str {
        match self {
            App::Surf => "Surf",
            App::Spotify => "Spotify",
            App::Files => "Files",
            App::Clock => "Clock",
            App::Mail => "Mail",
            App::Studio => "Studio",
            App::Settings => "Settings",
        }
    }

    pub fn icon(self) -> IconShape {
        match self {
            App::Surf => IconShape::Globe,
            App::Spotify => IconShape::Music,
            App::Files => IconShape::Files,
            App::Clock => IconShape::Clock,
            App::Mail => IconShape::Mail,
            App::Studio => IconShape::Studio,
            App::Settings => IconShape::Brightness,
        }
    }

    /// Whether this app is finished enough to do anything yet.
    ///
    /// An unbuilt app still appears in the dock and still opens — into a window
    /// that says what it will be. Hiding it until it works would make the
    /// system look finished and behave as though it were, which is worse than
    /// being visibly partway.
    pub fn built(self) -> bool {
        matches!(self, App::Spotify | App::Files | App::Clock | App::Settings)
    }

    /// How large a window wants to be, as a fraction of the workspace.
    ///
    /// Apps differ: Clock is a glance and wants a small window; Surf and Files
    /// are worked in and want most of the screen. A single default size would
    /// make one of the two wrong.
    pub fn preferred_size(self) -> (f32, f32) {
        match self {
            App::Surf => (0.78, 0.82),
            App::Spotify => (0.46, 0.76),
            App::Files => (0.62, 0.70),
            App::Clock => (0.40, 0.52),
            App::Mail => (0.66, 0.74),
            App::Studio => (0.72, 0.78),
            // A column of a few controls. Wider would be empty space.
            App::Settings => (0.34, 0.66),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_is_called_surf_and_nothing_longer() {
        assert_eq!(App::Surf.name(), "Surf");
        for app in App::ALL {
            assert!(!app.name().contains("détends"), "{app:?} introduces itself");
        }
    }

    #[test]
    fn every_app_has_its_own_icon() {
        let mut seen = Vec::new();
        for app in App::ALL {
            assert!(!seen.contains(&app.icon()), "{app:?} reuses an icon");
            seen.push(app.icon());
        }
    }

    #[test]
    fn every_app_has_a_name_and_a_sane_size() {
        for app in App::ALL {
            assert!(!app.name().is_empty());
            let (w, h) = app.preferred_size();
            assert!(w > 0.2 && w <= 1.0, "{app:?} width {w}");
            assert!(h > 0.2 && h <= 1.0, "{app:?} height {h}");
        }
    }

    #[test]
    fn the_dock_is_small_enough_to_be_a_dock() {
        // Six is a set you can take in at a glance. Sixty is a menu.
        assert!(App::ALL.len() <= 8, "the dock has grown into a launcher");
    }
}
