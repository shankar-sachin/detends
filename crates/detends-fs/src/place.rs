//! Destinations.
//!
//! "Primary destinations: Recents, Studio, Downloads, Screenshots… Search
//! should be emphasized over deeply navigating folder trees." (§9)
//!
//! So the vault has four fixed destinations and whatever folders the user
//! makes. A destination is not a bookmark into a tree — it is where a kind of
//! thing lives, and the tree underneath it is an implementation detail the user
//! only meets if they go looking.

use std::path::{Path, PathBuf};

/// A fixed destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    /// Not a directory: everything in the vault, newest first.
    Recents,
    /// Where `.dpg`, `.dek` and `.dgr` live.
    Studio,
    Downloads,
    /// Captures land here on their own (§14).
    Screenshots,
    /// Deleted things, until they are restored or purged (§9).
    Deleted,
}

impl Place {
    /// The order they appear in, which is the order of §9 — and Deleted last,
    /// because it is a safety net rather than a destination anyone aims for.
    pub const ALL: [Place; 5] = [
        Place::Recents,
        Place::Studio,
        Place::Downloads,
        Place::Screenshots,
        Place::Deleted,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Place::Recents => "Recents",
            Place::Studio => "Studio",
            Place::Downloads => "Downloads",
            Place::Screenshots => "Screenshots",
            Place::Deleted => "Recently Deleted",
        }
    }

    /// The directory name inside the vault, for the places that have one.
    ///
    /// `Recents` has none: it is a question asked of the whole vault, not a
    /// folder, and giving it one would be a lie the user could navigate into.
    pub fn directory(self) -> Option<&'static str> {
        match self {
            Place::Recents => None,
            Place::Studio => Some("Studio"),
            Place::Downloads => Some("Downloads"),
            Place::Screenshots => Some("Screenshots"),
            Place::Deleted => Some(crate::trash::DIRECTORY),
        }
    }

    /// Whether new things may be made here.
    ///
    /// Recents is a view, and Recently Deleted is a holding area — creating a
    /// folder in either would have nowhere to put it.
    pub fn writable(self) -> bool {
        matches!(self, Place::Studio | Place::Downloads | Place::Screenshots)
    }

    /// What to say when there is nothing here yet.
    ///
    /// Never a bare "Empty": an empty place should say what it is for, so the
    /// first time a user opens it they learn something instead of nothing.
    pub fn empty_message(self) -> &'static str {
        match self {
            Place::Recents => "Nothing recent",
            Place::Studio => "No documents yet",
            Place::Downloads => "No downloads",
            Place::Screenshots => "No screenshots",
            Place::Deleted => "Nothing deleted",
        }
    }

    /// Find the destination a path belongs to, if any.
    pub fn of(root: &Path, path: &Path) -> Option<Self> {
        let relative = path.strip_prefix(root).ok()?;
        let first = relative.components().next()?.as_os_str().to_str()?;
        Place::ALL
            .into_iter()
            .filter(|p| *p != Place::Recents)
            .find(|p| p.directory() == Some(first))
    }
}

/// Where the vault lives on disk.
///
/// A single directory holding all five destinations, rather than destinations
/// scattered across a Unix home. That is what lets §13's rule hold — "Linux
/// implementation details remain invisible" — and it means the vault can be
/// moved, backed up or synced as one object.
pub fn default_root() -> Option<PathBuf> {
    directories::UserDirs::new()
        .map(|dirs| dirs.home_dir().join("détends"))
        .or_else(|| {
            directories::ProjectDirs::from("", "", "detends")
                .map(|dirs| dirs.data_dir().join("vault"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_destinations_are_the_ones_the_specification_names() {
        let names: Vec<_> = Place::ALL.iter().map(|p| p.name()).collect();
        assert_eq!(
            names,
            [
                "Recents",
                "Studio",
                "Downloads",
                "Screenshots",
                "Recently Deleted"
            ]
        );
    }

    #[test]
    fn recents_is_a_view_rather_than_a_folder() {
        assert_eq!(Place::Recents.directory(), None);
        assert!(!Place::Recents.writable());
    }

    #[test]
    fn a_path_resolves_to_the_destination_that_holds_it() {
        let root = Path::new("/vault");
        assert_eq!(
            Place::of(root, Path::new("/vault/Studio/physics.dpg")),
            Some(Place::Studio)
        );
        assert_eq!(
            Place::of(root, Path::new("/vault/Screenshots/shot.png")),
            Some(Place::Screenshots)
        );
        assert_eq!(Place::of(root, Path::new("/vault/loose.txt")), None);
        assert_eq!(Place::of(root, Path::new("/elsewhere/x.txt")), None);
    }

    #[test]
    fn deleted_and_recents_refuse_to_be_written_into() {
        assert!(!Place::Deleted.writable());
        assert!(!Place::Recents.writable());
        assert!(Place::Studio.writable());
    }

    #[test]
    fn an_empty_place_says_what_it_is_for() {
        for place in Place::ALL {
            let message = place.empty_message();
            assert!(!message.is_empty());
            assert_ne!(message, "Empty", "{place:?} should say more than nothing");
        }
    }
}
