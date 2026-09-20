//! The vault: one directory holding the five destinations.
//!
//! This is the object the shell talks to. It owns the root, the trash ledger
//! and the current sort order, and it is the only thing that turns a [`Place`]
//! into files on a disk.
//!
//! It deliberately holds no listing of its own. Files asks for what it needs
//! when it needs it, and the answer is always read fresh — a cache here would
//! have to be invalidated by something, and nothing in the system is in a
//! position to know when the disk changed.

use crate::entry::{Entry, Sort};
use crate::kind::{DocKind, Kind};
use crate::ops;
use crate::place::{self, Place};
use crate::search::{self, Hit};
use crate::trash::{Deleted, Trash};
use jiff::Timestamp;
use std::io::Result;
use std::path::{Path, PathBuf};

/// How many things Recents and search will show.
const RECENTS: usize = 60;
const RESULTS: usize = 40;

/// The file store behind Files.
pub struct Vault {
    root: PathBuf,
    trash: Trash,
    sort: Sort,
}

impl Vault {
    /// Open the vault at a given root, creating the destinations if they are
    /// not there yet.
    ///
    /// Creating them is the right default: a first run should land in a system
    /// that looks finished, not one that asks the user to build their own
    /// Downloads folder before it will work.
    pub fn open(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let mut vault = Vault {
            root,
            trash: Trash::new(),
            sort: Sort::default(),
        };
        vault.prepare();
        vault
    }

    /// Open the vault at the default location.
    pub fn open_default() -> Option<Self> {
        place::default_root().map(Vault::open)
    }

    fn prepare(&mut self) {
        for place in Place::ALL {
            if let Some(directory) = place.directory() {
                let _ = std::fs::create_dir_all(self.root.join(directory));
            }
        }
        self.trash = Trash::load(&self.root);

        // Tidy on open rather than on a timer — no background work (§21).
        if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            if let Ok(now) = Timestamp::from_second(now.as_secs() as i64) {
                let _ = self.trash.purge_expired(&self.root, now);
            }
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sort(&self) -> Sort {
        self.sort
    }

    pub fn set_sort(&mut self, sort: Sort) {
        self.sort = sort;
    }

    /// Move to the next sort order, for a control with one affordance.
    pub fn cycle_sort(&mut self) -> Sort {
        self.sort = self.sort.next();
        self.sort
    }

    /// The directory a destination lives in, if it has one.
    pub fn directory(&self, place: Place) -> Option<PathBuf> {
        place.directory().map(|d| self.root.join(d))
    }

    /// List a destination.
    ///
    /// Recents is a question asked of the whole vault; the rest are directory
    /// listings. Either way the result is sorted by the vault's current order,
    /// except Recents, which is by definition in time order already.
    pub fn list(&self, place: Place) -> Vec<Entry> {
        match place {
            Place::Recents => search::recents(&self.root, RECENTS),
            Place::Deleted => self
                .trash
                .listing(&self.root)
                .into_iter()
                .map(|(_, entry)| entry)
                .collect(),
            _ => match self.directory(place) {
                Some(directory) => self.list_directory(&directory),
                None => Vec::new(),
            },
        }
    }

    /// List any folder inside the vault.
    pub fn list_directory(&self, directory: &Path) -> Vec<Entry> {
        if !ops::contained(&self.root, directory) {
            return Vec::new();
        }

        let Ok(listing) = std::fs::read_dir(directory) else {
            return Vec::new();
        };

        let mut entries: Vec<Entry> = listing
            .flatten()
            .filter_map(|item| Entry::read(&item.path()))
            .filter(|entry| !entry.hidden())
            .collect();

        self.sort.apply(&mut entries);
        entries
    }

    /// Recently deleted, with the metadata the destination shows.
    pub fn deleted(&self) -> Vec<(Deleted, Entry)> {
        self.trash.listing(&self.root)
    }

    /// Search the whole vault.
    pub fn search(&self, query: &str) -> Vec<Hit> {
        search::search(&self.root, query, RESULTS)
    }

    /// Every détends document of one Studio type, newest first.
    pub fn documents(&self, kind: DocKind) -> Vec<Entry> {
        search::of_kind(&self.root, Kind::Document(kind), RECENTS)
    }

    // ---- operations -------------------------------------------------------
    //
    // Each of these is a thin pass-through to `ops`, plus whatever bookkeeping
    // the vault owns. They exist so the shell never touches `std::fs` itself:
    // containment and collision handling are not decisions a UI should be
    // making one call site at a time.

    pub fn create_folder(&self, parent: &Path, name: &str) -> Result<PathBuf> {
        ops::create_folder(&self.root, parent, name)
    }

    pub fn rename(&self, path: &Path, name: &str) -> Result<PathBuf> {
        ops::rename(&self.root, path, name)
    }

    pub fn move_to(&self, path: &Path, folder: &Path) -> Result<PathBuf> {
        ops::move_to(&self.root, path, folder)
    }

    pub fn copy_to(&self, path: &Path, folder: &Path) -> Result<PathBuf> {
        ops::copy_to(&self.root, path, folder)
    }

    pub fn duplicate(&self, path: &Path) -> Result<PathBuf> {
        ops::duplicate(&self.root, path)
    }

    /// Delete something — which is to say, move it aside so it can come back.
    pub fn delete(&mut self, path: &Path, now: Timestamp) -> Result<()> {
        if !ops::contained(&self.root, path) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "that path is outside the vault",
            ));
        }
        self.trash.accept(&self.root, path, now)
    }

    /// Put something back where it came from.
    pub fn restore(&mut self, stored: &str) -> Result<PathBuf> {
        self.trash.restore(&self.root, stored)
    }

    /// Remove something permanently.
    pub fn purge(&mut self, stored: &str) -> Result<()> {
        self.trash.purge(&self.root, stored)
    }

    /// Empty Recently Deleted.
    pub fn empty_trash(&mut self) -> Result<()> {
        for stored in self.trash.items.iter().map(|d| d.stored.clone()).collect::<Vec<_>>() {
            self.trash.purge(&self.root, &stored)?;
        }
        Ok(())
    }

    /// Where a screenshot should be written (§14).
    ///
    /// Files → Screenshots is not a convention the capture code should have to
    /// know: it asks the vault, and the vault decides. The name carries the
    /// date, so the folder sorts chronologically under any order.
    pub fn screenshot_path(&self, now: Timestamp, zone: &jiff::tz::TimeZone) -> PathBuf {
        let local = now.to_zoned(zone.clone());
        let name = format!(
            "Screenshot {:04}-{:02}-{:02} at {:02}.{:02}.{:02}.png",
            local.year(),
            local.month(),
            local.day(),
            local.hour(),
            local.minute(),
            local.second()
        );
        let directory = self
            .directory(Place::Screenshots)
            .unwrap_or_else(|| self.root.clone());
        ops::unused_name(&directory.join(name))
    }

    /// Which destination a path belongs to.
    pub fn place_of(&self, path: &Path) -> Option<Place> {
        Place::of(&self.root, path)
    }

    /// The trail from a destination down to a folder, for the header.
    ///
    /// Not a breadcrumb bar to click through — §9 is explicit that navigating a
    /// tree is not the point. It is one line saying where you are, so that
    /// going two folders deep does not lose you.
    pub fn trail(&self, path: &Path) -> Vec<String> {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        for (index, component) in relative.components().enumerate() {
            let name = component.as_os_str().to_string_lossy().into_owned();
            if index == 0 {
                // Show the destination's proper name, not its directory name.
                let place = Place::ALL
                    .into_iter()
                    .find(|p| p.directory() == Some(name.as_str()));
                out.push(place.map(|p| p.name().to_string()).unwrap_or(name));
            } else {
                out.push(name);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{scratch, stamp};

    fn vault(name: &str) -> Vault {
        Vault::open(scratch(name))
    }

    #[test]
    fn opening_creates_the_destinations() {
        let v = vault("vault-prepare");
        for place in Place::ALL {
            if let Some(directory) = place.directory() {
                assert!(
                    v.root().join(directory).is_dir(),
                    "{} was not created",
                    place.name()
                );
            }
        }
    }

    #[test]
    fn opening_twice_is_harmless() {
        let root = scratch("vault-reopen");
        let first = Vault::open(&root);
        std::fs::write(first.root().join("Studio").join("a.dpg"), b"x").unwrap();

        let second = Vault::open(&root);
        assert_eq!(second.list(Place::Studio).len(), 1, "it should not have reset");
    }

    #[test]
    fn a_destination_lists_what_is_in_it_and_hides_the_trash() {
        let v = vault("vault-list");
        std::fs::write(v.root().join("Studio").join("physics.dpg"), b"x").unwrap();
        std::fs::write(v.root().join("Studio").join("slides.dek"), b"x").unwrap();

        let names: Vec<_> = v.list(Place::Studio).iter().map(|e| e.name.clone()).collect();
        assert_eq!(names.len(), 2);

        // The trash lives inside the vault root but is never listed there.
        assert!(v.list_directory(v.root()).iter().all(|e| !e.hidden()));
    }

    #[test]
    fn recents_gathers_across_destinations_newest_first() {
        let v = vault("vault-recents");
        std::fs::write(v.root().join("Studio").join("older.dpg"), b"x").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(v.root().join("Downloads").join("newer.pdf"), b"x").unwrap();

        let names: Vec<_> = v.list(Place::Recents).iter().map(|e| e.name.clone()).collect();
        assert_eq!(names.first().map(String::as_str), Some("newer.pdf"));
        assert!(names.contains(&"older.dpg".to_string()));
    }

    #[test]
    fn the_sort_order_changes_the_listing() {
        let mut v = vault("vault-sort");
        std::fs::write(v.root().join("Studio").join("b.txt"), b"xxxxxxxxxx").unwrap();
        std::fs::write(v.root().join("Studio").join("a.txt"), b"x").unwrap();

        v.set_sort(Sort::Name);
        assert_eq!(v.list(Place::Studio)[0].name, "a.txt");

        v.set_sort(Sort::Size);
        assert_eq!(v.list(Place::Studio)[0].name, "b.txt");
    }

    #[test]
    fn deleting_then_restoring_returns_the_file_to_its_destination() {
        let mut v = vault("vault-delete-restore");
        let file = v.root().join("Studio").join("physics.dpg");
        std::fs::write(&file, b"work").unwrap();

        v.delete(&file, stamp(0)).unwrap();
        assert!(v.list(Place::Studio).is_empty());
        assert_eq!(v.list(Place::Deleted).len(), 1);

        let stored = v.deleted()[0].0.stored.clone();
        v.restore(&stored).unwrap();

        assert_eq!(v.list(Place::Studio).len(), 1);
        assert!(v.list(Place::Deleted).is_empty());
        assert_eq!(std::fs::read(&file).unwrap(), b"work");
    }

    #[test]
    fn deleting_something_outside_the_vault_is_refused() {
        let mut v = vault("vault-delete-escape");
        let outside = v.root().parent().unwrap().join("important.txt");
        std::fs::write(&outside, b"do not touch").unwrap();

        assert!(v.delete(&outside, stamp(0)).is_err());
        assert!(outside.exists(), "it must still be there");
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn emptying_the_trash_removes_everything_permanently() {
        let mut v = vault("vault-empty");
        for name in ["a.txt", "b.txt"] {
            let file = v.root().join("Downloads").join(name);
            std::fs::write(&file, b"x").unwrap();
            v.delete(&file, stamp(0)).unwrap();
        }
        assert_eq!(v.list(Place::Deleted).len(), 2);

        v.empty_trash().unwrap();
        assert!(v.list(Place::Deleted).is_empty());
    }

    #[test]
    fn a_screenshot_lands_in_screenshots_with_a_dated_name() {
        let v = vault("vault-screenshot");
        let path = v.screenshot_path(stamp(1_758_000_000), &jiff::tz::TimeZone::UTC);

        assert_eq!(path.parent().unwrap(), v.root().join("Screenshots"));
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("Screenshot 2025-09-16 at "), "got {name}");
        assert!(name.ends_with(".png"));
    }

    #[test]
    fn two_screenshots_in_the_same_second_do_not_collide() {
        let v = vault("vault-screenshot-collide");
        let when = stamp(1_758_000_000);

        let first = v.screenshot_path(when, &jiff::tz::TimeZone::UTC);
        std::fs::write(&first, b"png").unwrap();
        let second = v.screenshot_path(when, &jiff::tz::TimeZone::UTC);

        assert_ne!(first, second);
    }

    #[test]
    fn the_trail_names_the_destination_rather_than_its_directory() {
        let v = vault("vault-trail");
        let deep = v.root().join("Studio").join("Term").join("Week 1");
        std::fs::create_dir_all(&deep).unwrap();

        assert_eq!(v.trail(&deep), ["Studio", "Term", "Week 1"]);
        assert_eq!(
            v.trail(&v.root().join(".detends-trash")),
            ["Recently Deleted"]
        );
    }

    #[test]
    fn documents_can_be_gathered_by_studio_type() {
        let v = vault("vault-documents");
        std::fs::write(v.root().join("Studio").join("a.dpg"), b"x").unwrap();
        std::fs::write(v.root().join("Studio").join("b.dek"), b"x").unwrap();

        assert_eq!(v.documents(DocKind::Page).len(), 1);
        assert_eq!(v.documents(DocKind::Deck).len(), 1);
        assert_eq!(v.documents(DocKind::Grid).len(), 0);
    }

    #[test]
    fn searching_the_vault_finds_documents_anywhere_in_it() {
        let v = vault("vault-search");
        let deep = v.root().join("Studio").join("Term");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("physics.dpg"), b"x").unwrap();

        let hits = v.search("physics");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entry.name, "physics.dpg");
    }

    #[test]
    fn listing_a_folder_outside_the_vault_returns_nothing() {
        let v = vault("vault-list-escape");
        assert!(v.list_directory(Path::new("/etc")).is_empty());
    }
}
