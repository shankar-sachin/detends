//! Recently deleted, and getting things back.
//!
//! "restore recently deleted items" (§9). Deleting in détends never removes
//! anything: it moves it aside and writes down where it came from. That is the
//! difference between a system a person can relax in and one where every
//! keystroke near a delete key is a small risk.
//!
//! The ledger is JSON next to the items, for the same reason Clock's is — a few
//! kilobytes, and worth more repairable by hand than fast.

use crate::entry::Entry;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The directory inside the vault. Hidden, so it never appears in a listing of
/// the vault itself — it is reached through its destination or not at all.
pub const DIRECTORY: &str = ".detends-trash";

/// How long something waits before it may be purged.
pub const KEEP_DAYS: i64 = 30;

/// One deleted thing, and where it came from.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Deleted {
    /// The unique name it is stored under, which is not its original name.
    pub stored: String,
    /// What it was called, for showing in the listing.
    pub name: String,
    /// Where it was, so restoring is exact rather than approximate.
    pub origin: PathBuf,
    pub deleted_at: Timestamp,
}

impl Deleted {
    /// Days since it was deleted, against a given instant.
    pub fn age_days(&self, now: Timestamp) -> i64 {
        (now.as_second() - self.deleted_at.as_second()) / 86_400
    }

    /// Whether it has waited out its keep period.
    pub fn expired(&self, now: Timestamp) -> bool {
        self.age_days(now) >= KEEP_DAYS
    }

    /// What the listing says under the name.
    pub fn readout(&self, now: Timestamp) -> String {
        match self.age_days(now) {
            d if d <= 0 => "Deleted today".to_string(),
            1 => "Deleted yesterday".to_string(),
            d => format!("Deleted {d} days ago"),
        }
    }
}

/// The ledger of everything waiting.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Trash {
    pub items: Vec<Deleted>,
}

impl Trash {
    pub fn new() -> Self {
        Self::default()
    }

    /// The trash directory for a vault.
    pub fn directory(root: &Path) -> PathBuf {
        root.join(DIRECTORY)
    }

    fn ledger_path(root: &Path) -> PathBuf {
        Self::directory(root).join("ledger.json")
    }

    /// Read the ledger back, or start empty.
    ///
    /// An unreadable ledger is not fatal: the files are still there, and a
    /// vault that refuses to open because its wastebasket is confused would be
    /// a poor trade.
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(Self::ledger_path(root))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Write the ledger out, atomically.
    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        let directory = Self::directory(root);
        std::fs::create_dir_all(&directory)?;

        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let path = Self::ledger_path(root);
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, &path)
    }

    /// Move something into the trash and record where it came from.
    ///
    /// The stored name carries the deletion time and a counter, so deleting two
    /// files called `notes.txt` a second apart does not have the second quietly
    /// destroy the first — which is precisely the accident the trash exists to
    /// prevent.
    pub fn accept(&mut self, root: &Path, path: &Path, now: Timestamp) -> std::io::Result<()> {
        let directory = Self::directory(root);
        std::fs::create_dir_all(&directory)?;

        let name = path
            .file_name()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no name"))?
            .to_string_lossy()
            .into_owned();

        let mut counter = 0;
        let stored = loop {
            let candidate = if counter == 0 {
                format!("{}-{}", now.as_second(), name)
            } else {
                format!("{}-{counter}-{}", now.as_second(), name)
            };
            if !directory.join(&candidate).exists() {
                break candidate;
            }
            counter += 1;
        };

        std::fs::rename(path, directory.join(&stored))?;
        self.items.push(Deleted {
            stored,
            name,
            origin: path.to_path_buf(),
            deleted_at: now,
        });
        self.save(root)
    }

    /// Put something back where it came from.
    ///
    /// If the original name is taken again, the restored copy is renamed rather
    /// than overwriting whatever is there now — restoring must never be the
    /// operation that loses data.
    pub fn restore(&mut self, root: &Path, stored: &str) -> std::io::Result<PathBuf> {
        let index = self
            .items
            .iter()
            .position(|d| d.stored == stored)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "not in the trash"))?;

        let item = self.items[index].clone();
        let from = Self::directory(root).join(&item.stored);

        if let Some(parent) = item.origin.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let destination = crate::ops::unused_name(&item.origin);
        std::fs::rename(&from, &destination)?;

        self.items.remove(index);
        self.save(root)?;
        Ok(destination)
    }

    /// Remove something permanently.
    pub fn purge(&mut self, root: &Path, stored: &str) -> std::io::Result<()> {
        let Some(index) = self.items.iter().position(|d| d.stored == stored) else {
            return Ok(());
        };
        let path = Self::directory(root).join(stored);

        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else if path.exists() {
            std::fs::remove_file(&path)?;
        }

        self.items.remove(index);
        self.save(root)
    }

    /// Drop everything that has waited out its keep period.
    ///
    /// Called on load rather than on a timer: the vault tidies itself when it
    /// is next opened, which needs no background activity at all (§21).
    pub fn purge_expired(&mut self, root: &Path, now: Timestamp) -> std::io::Result<usize> {
        let expired: Vec<String> = self
            .items
            .iter()
            .filter(|d| d.expired(now))
            .map(|d| d.stored.clone())
            .collect();

        for stored in &expired {
            self.purge(root, stored)?;
        }
        Ok(expired.len())
    }

    /// What the Recently Deleted destination shows, newest first.
    pub fn listing(&self, root: &Path) -> Vec<(Deleted, Entry)> {
        let directory = Self::directory(root);
        let mut out: Vec<(Deleted, Entry)> = self
            .items
            .iter()
            .filter_map(|d| {
                let mut entry = Entry::read(&directory.join(&d.stored))?;
                // Show what it was called, not what it is filed under.
                entry.name = d.name.clone();
                Some((d.clone(), entry))
            })
            .collect();
        out.sort_by_key(|(deleted, _)| std::cmp::Reverse(deleted.deleted_at));
        out
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{scratch, stamp};

    #[test]
    fn deleting_moves_a_file_aside_rather_than_destroying_it() {
        let root = scratch("trash-accept");
        let file = root.join("Studio").join("notes.dpg");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"content").unwrap();

        let mut trash = Trash::new();
        trash.accept(&root, &file, stamp(0)).unwrap();

        assert!(!file.exists(), "the original should be gone from its place");
        assert_eq!(trash.len(), 1);

        let stored = Trash::directory(&root).join(&trash.items[0].stored);
        assert_eq!(std::fs::read(stored).unwrap(), b"content");
    }

    #[test]
    fn restoring_puts_it_back_exactly_where_it_was() {
        let root = scratch("trash-restore");
        let file = root.join("Studio").join("physics.dpg");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"work").unwrap();

        let mut trash = Trash::new();
        trash.accept(&root, &file, stamp(0)).unwrap();
        let back = trash.restore(&root, &trash.items[0].stored.clone()).unwrap();

        assert_eq!(back, file);
        assert_eq!(std::fs::read(&file).unwrap(), b"work");
        assert!(trash.is_empty());
    }

    #[test]
    fn two_files_with_the_same_name_do_not_overwrite_each_other() {
        // The exact accident the trash exists to prevent.
        let root = scratch("trash-collide");
        std::fs::create_dir_all(root.join("Studio")).unwrap();

        let mut trash = Trash::new();
        for body in [b"first".as_slice(), b"second".as_slice()] {
            let file = root.join("Studio").join("notes.txt");
            std::fs::write(&file, body).unwrap();
            trash.accept(&root, &file, stamp(0)).unwrap();
        }

        assert_eq!(trash.len(), 2);
        assert_ne!(trash.items[0].stored, trash.items[1].stored);

        let bodies: Vec<Vec<u8>> = trash
            .items
            .iter()
            .map(|d| std::fs::read(Trash::directory(&root).join(&d.stored)).unwrap())
            .collect();
        assert!(bodies.contains(&b"first".to_vec()));
        assert!(bodies.contains(&b"second".to_vec()));
    }

    #[test]
    fn restoring_over_a_new_file_of_the_same_name_keeps_both() {
        // Restoring must never be the operation that loses data.
        let root = scratch("trash-restore-collide");
        let file = root.join("Studio").join("notes.txt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"old").unwrap();

        let mut trash = Trash::new();
        trash.accept(&root, &file, stamp(0)).unwrap();

        std::fs::write(&file, b"new").unwrap();
        let back = trash.restore(&root, &trash.items[0].stored.clone()).unwrap();

        assert_ne!(back, file, "it should not have landed on the new file");
        assert_eq!(std::fs::read(&file).unwrap(), b"new");
        assert_eq!(std::fs::read(&back).unwrap(), b"old");
    }

    #[test]
    fn a_folder_goes_in_and_comes_back_with_its_contents() {
        let root = scratch("trash-folder");
        let folder = root.join("Studio").join("Term");
        std::fs::create_dir_all(folder.join("deep")).unwrap();
        std::fs::write(folder.join("deep").join("a.txt"), b"x").unwrap();

        let mut trash = Trash::new();
        trash.accept(&root, &folder, stamp(0)).unwrap();
        assert!(!folder.exists());

        trash.restore(&root, &trash.items[0].stored.clone()).unwrap();
        assert_eq!(std::fs::read(folder.join("deep").join("a.txt")).unwrap(), b"x");
    }

    #[test]
    fn the_ledger_survives_a_round_trip() {
        let root = scratch("trash-ledger");
        let file = root.join("Downloads").join("report.pdf");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"pdf").unwrap();

        let mut trash = Trash::new();
        trash.accept(&root, &file, stamp(0)).unwrap();

        let back = Trash::load(&root);
        assert_eq!(back.items, trash.items);
        assert_eq!(back.items[0].origin, file);
    }

    #[test]
    fn purging_removes_it_permanently() {
        let root = scratch("trash-purge");
        let file = root.join("Downloads").join("junk.bin");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"junk").unwrap();

        let mut trash = Trash::new();
        trash.accept(&root, &file, stamp(0)).unwrap();
        let stored = trash.items[0].stored.clone();
        trash.purge(&root, &stored).unwrap();

        assert!(trash.is_empty());
        assert!(!Trash::directory(&root).join(&stored).exists());
    }

    #[test]
    fn things_older_than_the_keep_period_are_purged_and_newer_ones_are_kept() {
        let root = scratch("trash-expire");
        std::fs::create_dir_all(root.join("Downloads")).unwrap();

        let mut trash = Trash::new();
        for (name, when) in [("ancient.txt", 0), ("fresh.txt", 29 * 86_400)] {
            let file = root.join("Downloads").join(name);
            std::fs::write(&file, b"x").unwrap();
            trash.accept(&root, &file, stamp(when)).unwrap();
        }

        let purged = trash.purge_expired(&root, stamp(30 * 86_400)).unwrap();
        assert_eq!(purged, 1);
        assert_eq!(trash.len(), 1);
        assert_eq!(trash.items[0].name, "fresh.txt");
    }

    #[test]
    fn the_listing_shows_original_names_newest_first() {
        let root = scratch("trash-listing");
        std::fs::create_dir_all(root.join("Downloads")).unwrap();

        let mut trash = Trash::new();
        for (name, when) in [("older.txt", 10), ("newer.txt", 500)] {
            let file = root.join("Downloads").join(name);
            std::fs::write(&file, b"x").unwrap();
            trash.accept(&root, &file, stamp(when)).unwrap();
        }

        let listing = trash.listing(&root);
        assert_eq!(listing[0].1.name, "newer.txt");
        assert_eq!(listing[1].1.name, "older.txt");
    }

    #[test]
    fn the_readout_says_how_long_ago_in_words() {
        let root = scratch("trash-readout");
        let file = root.join("Downloads").join("a.txt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"x").unwrap();

        let mut trash = Trash::new();
        trash.accept(&root, &file, stamp(0)).unwrap();
        let item = &trash.items[0];

        assert_eq!(item.readout(stamp(0)), "Deleted today");
        assert_eq!(item.readout(stamp(86_400)), "Deleted yesterday");
        assert_eq!(item.readout(stamp(5 * 86_400)), "Deleted 5 days ago");
    }

    #[test]
    fn restoring_something_unknown_fails_rather_than_panicking() {
        let root = scratch("trash-unknown");
        let mut trash = Trash::new();
        assert!(trash.restore(&root, "nothing-here").is_err());
    }
}
