//! The vault — files for détends.
//!
//! "Files is the system file manager. It must NOT become a Finder/Explorer
//! clone." (§9)
//!
//! The difference is in the shape of this crate rather than in the drawing. A
//! conventional file manager is a view onto a filesystem: its model is the
//! tree, and everything else is decoration on top of it. Here the model is a
//! small set of **destinations** — Recents, Studio, Downloads, Screenshots and
//! Recently Deleted — and the tree is what happens to be underneath one of
//! them. Search is a first-class way to reach things rather than a box in a
//! corner, because the specification says navigation should not be the primary
//! way anyone finds anything.
//!
//! Three properties hold across everything here:
//!
//! - **Nothing is silently overwritten.** Every collision resolves to a new
//!   name. A user can destroy their work on purpose; they should never do it by
//!   accident.
//! - **Nothing escapes the vault.** Operations are checked against the root
//!   before they run, so a crafted name cannot reach the rest of the disk.
//! - **Deleting is reversible.** It moves things aside and writes down where
//!   they came from (§9, "restore recently deleted items").
//!
//! Like [`detends_time`], this is pure domain: no GPU, no window, no rendering.
//! All of it is testable against a temporary directory, and all of it is.

#![forbid(unsafe_code)]

pub mod entry;
pub mod kind;
pub mod ops;
pub mod place;
pub mod search;
pub mod trash;
pub mod vault;

pub use entry::{Entry, Sort};
pub use kind::{DocKind, Kind};
pub use place::Place;
pub use search::{Hit, Quality};
pub use trash::{Deleted, Trash};
pub use vault::Vault;

#[cfg(test)]
mod test_support {
    use jiff::Timestamp;
    use std::path::PathBuf;

    /// A fresh directory for one test.
    ///
    /// Named per test and cleared on entry, so tests neither collide with each
    /// other nor inherit anything from a previous run that failed halfway.
    pub fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("detends-fs-{}", std::process::id()))
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A timestamp, for tests that care about order rather than about dates.
    pub fn stamp(seconds: i64) -> Timestamp {
        Timestamp::from_second(seconds).unwrap()
    }
}
