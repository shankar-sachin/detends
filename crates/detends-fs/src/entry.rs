//! One thing in the vault, and the orders they can be put in.

use crate::kind::Kind;
use jiff::Timestamp;
use std::path::{Path, PathBuf};

/// A single file or folder, as Files needs to show it.
///
/// Read once when a place is listed and then treated as a snapshot. Files does
/// not hold live handles on the filesystem: a listing is a photograph, and the
/// cure for a stale one is to take another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    /// Bytes. Zero for folders — the size of a folder is a question with no
    /// cheap answer and no one waits for it.
    pub size: u64,
    pub modified: Option<Timestamp>,
}

impl Entry {
    /// Read one from a path, if the filesystem will say anything about it.
    ///
    /// A path that cannot be read returns `None` rather than an error: a
    /// listing that skips one unreadable item is far more useful than a
    /// listing that refuses to exist.
    pub fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        let is_dir = if metadata.file_type().is_symlink() {
            // Follow the link to decide what it behaves like, but keep the
            // link's own path: the user put the link there.
            std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
        } else {
            metadata.is_dir()
        };

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| Timestamp::try_from(t).ok());

        Some(Entry {
            name: path.file_name()?.to_string_lossy().into_owned(),
            path: path.to_path_buf(),
            kind: Kind::of(path, is_dir),
            size: if is_dir { 0 } else { metadata.len() },
            modified,
        })
    }

    pub fn is_folder(&self) -> bool {
        self.kind == Kind::Folder
    }

    /// Whether the vault should keep this out of sight.
    ///
    /// Dotfiles and détends' own bookkeeping are not secrets, but they are not
    /// what anyone opened Files to look at (§9: destinations, not a tree).
    pub fn hidden(&self) -> bool {
        self.name.starts_with('.')
    }

    /// The size, written the way a person reads it.
    ///
    /// Decimal units, because that is what storage is sold in and what every
    /// other number the user sees about their disk will agree with.
    pub fn readable_size(&self) -> String {
        if self.is_folder() {
            return String::new();
        }
        const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
        let mut size = self.size as f64;
        let mut unit = 0;
        while size >= 1000.0 && unit < UNITS.len() - 1 {
            size /= 1000.0;
            unit += 1;
        }
        if unit == 0 {
            format!("{} {}", self.size, UNITS[0])
        } else if size < 10.0 {
            format!("{size:.1} {}", UNITS[unit])
        } else {
            format!("{size:.0} {}", UNITS[unit])
        }
    }
}

/// How a listing is ordered (§9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Sort {
    /// Alphabetical. The order that does not move under you.
    #[default]
    Name,
    /// Most recently changed first — the order Recents is always in.
    Modified,
    /// Largest first, for the one question a size column is ever asked.
    Size,
    /// Groups by what things are, then alphabetical inside each group.
    Kind,
}

impl Sort {
    pub const ALL: [Sort; 4] = [Sort::Name, Sort::Modified, Sort::Size, Sort::Kind];

    pub fn name(self) -> &'static str {
        match self {
            Sort::Name => "Name",
            Sort::Modified => "Modified",
            Sort::Size => "Size",
            Sort::Kind => "Kind",
        }
    }

    /// The next order in the cycle, for a control with one affordance.
    pub fn next(self) -> Self {
        let index = Sort::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Sort::ALL[(index + 1) % Sort::ALL.len()]
    }

    /// Apply this order to a listing.
    ///
    /// Folders always come first, whatever the order — a folder is structure
    /// and a file is content, and mixing them by size makes a place unreadable.
    /// Name is the tie-breaker everywhere, so the result is total and a listing
    /// never shuffles between two identical-looking runs.
    pub fn apply(self, entries: &mut [Entry]) {
        entries.sort_by(|a, b| {
            b.is_folder()
                .cmp(&a.is_folder())
                .then_with(|| match self {
                    Sort::Name => std::cmp::Ordering::Equal,
                    Sort::Modified => b.modified.cmp(&a.modified),
                    Sort::Size => b.size.cmp(&a.size),
                    Sort::Kind => a.kind.cmp(&b.kind),
                })
                .then_with(|| natural_cmp(&a.name, &b.name))
        });
    }
}

/// Compare names the way a person would, with runs of digits as numbers.
///
/// Without this, `slide 10` sorts before `slide 2`, which is wrong every time
/// anyone has ever numbered anything.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();

    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut n = String::new();
                    while it.peek().is_some_and(|c| c.is_ascii_digit()) {
                        n.push(it.next().unwrap());
                    }
                    // Trimmed, so `007` and `7` compare as the same number.
                    n.trim_start_matches('0').to_string()
                };
                let x = take(&mut left);
                let y = take(&mut right);
                match x.len().cmp(&y.len()).then_with(|| x.cmp(&y)) {
                    std::cmp::Ordering::Equal => continue,
                    other => return other,
                }
            }
            (Some(x), Some(y)) => {
                let (lx, ly) = (x.to_ascii_lowercase(), y.to_ascii_lowercase());
                if lx != ly {
                    return lx.cmp(&ly);
                }
                left.next();
                right.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::DocKind;

    fn entry(name: &str, kind: Kind, size: u64, modified: i64) -> Entry {
        Entry {
            name: name.to_string(),
            path: PathBuf::from("/vault").join(name),
            kind,
            size,
            modified: Timestamp::from_second(modified).ok(),
        }
    }

    #[test]
    fn sizes_are_written_the_way_a_person_reads_them() {
        assert_eq!(entry("a", Kind::Other, 0, 0).readable_size(), "0 bytes");
        assert_eq!(entry("a", Kind::Other, 999, 0).readable_size(), "999 bytes");
        assert_eq!(entry("a", Kind::Other, 1_500, 0).readable_size(), "1.5 KB");
        assert_eq!(entry("a", Kind::Other, 45_000, 0).readable_size(), "45 KB");
        assert_eq!(
            entry("a", Kind::Other, 2_400_000_000, 0).readable_size(),
            "2.4 GB"
        );
    }

    #[test]
    fn a_folder_reports_no_size_rather_than_a_misleading_zero() {
        assert_eq!(entry("f", Kind::Folder, 0, 0).readable_size(), "");
    }

    #[test]
    fn folders_come_first_in_every_order() {
        for sort in Sort::ALL {
            let mut items = vec![
                entry("zeta.txt", Kind::Text, 9_000, 300),
                entry("Archive", Kind::Folder, 0, 100),
                entry("alpha.png", Kind::Image, 1, 200),
            ];
            sort.apply(&mut items);
            assert!(items[0].is_folder(), "{sort:?} put a file above a folder");
        }
    }

    #[test]
    fn names_sort_naturally_so_ten_follows_two() {
        let mut items = vec![
            entry("slide 10.dek", Kind::Document(DocKind::Deck), 0, 0),
            entry("slide 2.dek", Kind::Document(DocKind::Deck), 0, 0),
            entry("slide 1.dek", Kind::Document(DocKind::Deck), 0, 0),
        ];
        Sort::Name.apply(&mut items);
        let names: Vec<_> = items.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["slide 1.dek", "slide 2.dek", "slide 10.dek"]);
    }

    #[test]
    fn name_order_ignores_case() {
        let mut items = vec![entry("banana", Kind::Text, 0, 0), entry("Apple", Kind::Text, 0, 0)];
        Sort::Name.apply(&mut items);
        assert_eq!(items[0].name, "Apple");
    }

    #[test]
    fn modified_order_is_newest_first() {
        let mut items = vec![
            entry("old.txt", Kind::Text, 0, 100),
            entry("new.txt", Kind::Text, 0, 900),
        ];
        Sort::Modified.apply(&mut items);
        assert_eq!(items[0].name, "new.txt");
    }

    #[test]
    fn size_order_is_largest_first() {
        let mut items = vec![
            entry("small.txt", Kind::Text, 10, 0),
            entry("big.txt", Kind::Text, 10_000, 0),
        ];
        Sort::Size.apply(&mut items);
        assert_eq!(items[0].name, "big.txt");
    }

    #[test]
    fn kind_order_groups_documents_together_then_sorts_by_name() {
        let mut items = vec![
            entry("b.png", Kind::Image, 0, 0),
            entry("z.dpg", Kind::Document(DocKind::Page), 0, 0),
            entry("a.dpg", Kind::Document(DocKind::Page), 0, 0),
        ];
        Sort::Kind.apply(&mut items);
        let names: Vec<_> = items.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a.dpg", "z.dpg", "b.png"]);
    }

    #[test]
    fn sorting_is_stable_across_identical_runs() {
        // Two entries that differ only in name must still land in a fixed
        // order, or a listing shuffles under a refresh.
        let build = || {
            vec![
                entry("one.txt", Kind::Text, 5, 10),
                entry("two.txt", Kind::Text, 5, 10),
            ]
        };
        let (mut a, mut b) = (build(), build());
        Sort::Size.apply(&mut a);
        Sort::Size.apply(&mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn the_sort_cycle_returns_to_where_it_started() {
        let mut sort = Sort::Name;
        for _ in 0..Sort::ALL.len() {
            sort = sort.next();
        }
        assert_eq!(sort, Sort::Name);
    }

    #[test]
    fn dotfiles_are_hidden() {
        assert!(entry(".detends-trash", Kind::Folder, 0, 0).hidden());
        assert!(!entry("Notes.dpg", Kind::Document(DocKind::Page), 0, 0).hidden());
    }
}
