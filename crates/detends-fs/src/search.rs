//! Finding things.
//!
//! "Search should be emphasized over deeply navigating folder trees." (§9)
//!
//! That sentence is a requirement on quality, not just on placement. A search
//! box only replaces navigation if it reliably puts the thing you meant at the
//! top — otherwise people go back to clicking through folders, and the folder
//! tree quietly becomes the real interface after all.
//!
//! So matches are ranked rather than merely collected: an exact name beats a
//! prefix, a prefix beats a word inside the name, and that beats a loose
//! subsequence. Ties break towards the recently changed, because the thing you
//! are looking for is usually the thing you were just working on.

use crate::entry::Entry;
use crate::kind::Kind;
use std::path::Path;

/// How well a name matched, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Quality {
    /// The name, ignoring extension and case.
    Exact,
    /// The name starts with the query.
    Prefix,
    /// A word inside the name starts with the query.
    Word,
    /// The query appears somewhere in the name.
    Contains,
    /// The query's letters appear in order, but not together.
    Scattered,
}

/// One result.
#[derive(Clone, Debug)]
pub struct Hit {
    pub entry: Entry,
    pub quality: Quality,
}

/// How deep a search will walk.
///
/// A vault is destinations and shallow folders inside them (§9). Anything
/// deeper than this is almost certainly a checkout or a bundle, and walking
/// into it would cost far more than it would find.
const MAX_DEPTH: usize = 8;

/// The most entries a single search will visit.
///
/// Search is typed into, so it runs on every keystroke. A bound means a vault
/// with a huge folder in it degrades into a slightly incomplete search rather
/// than a frozen interface (§21).
const MAX_VISITED: usize = 20_000;

/// Score one name against a query, both already lowercased.
fn quality(name: &str, stem: &str, query: &str) -> Option<Quality> {
    if stem == query {
        return Some(Quality::Exact);
    }
    if name.starts_with(query) {
        return Some(Quality::Prefix);
    }
    // A word boundary is a space, dash, underscore or dot — the separators
    // people actually use in filenames.
    if name
        .split([' ', '-', '_', '.'])
        .any(|word| !word.is_empty() && word.starts_with(query))
    {
        return Some(Quality::Word);
    }
    if name.contains(query) {
        return Some(Quality::Contains);
    }

    // Subsequence: "phys" finds "Physics Homework", and "phw" finds it too.
    let mut chars = name.chars();
    for wanted in query.chars() {
        chars.find(|c| *c == wanted)?;
    }
    Some(Quality::Scattered)
}

/// Search a tree for a query.
///
/// An empty query returns nothing rather than everything: a search surface
/// that dumps the whole vault the moment it opens is noise (§15).
pub fn search(root: &Path, query: &str, limit: usize) -> Vec<Hit> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    let mut visited = 0usize;
    walk(root, 0, &mut visited, &mut |entry: Entry| {
        let name = entry.name.to_ascii_lowercase();
        let stem = Path::new(&name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.clone());

        if let Some(quality) = quality(&name, &stem, &query) {
            hits.push(Hit { entry, quality });
        }
    });

    // Best match first; then newest, because the thing you want is usually the
    // thing you touched last; then by name so the order is total.
    hits.sort_by(|a, b| {
        a.quality
            .cmp(&b.quality)
            .then_with(|| b.entry.modified.cmp(&a.entry.modified))
            .then_with(|| a.entry.name.to_lowercase().cmp(&b.entry.name.to_lowercase()))
    });
    hits.truncate(limit);
    hits
}

/// Everything in the vault, newest first — what Recents shows.
///
/// Folders are left out: Recents answers "what was I just working on", and a
/// folder is where work lives rather than the work itself.
pub fn recents(root: &Path, limit: usize) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut visited = 0usize;
    walk(root, 0, &mut visited, &mut |entry: Entry| {
        if !entry.is_folder() {
            out.push(entry);
        }
    });

    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    out.truncate(limit);
    out
}

/// Everything of a given kind, newest first.
pub fn of_kind(root: &Path, kind: Kind, limit: usize) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut visited = 0usize;
    walk(root, 0, &mut visited, &mut |entry: Entry| {
        if entry.kind == kind {
            out.push(entry);
        }
    });

    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    out.truncate(limit);
    out
}

/// Walk the vault, skipping what should not be seen.
fn walk(directory: &Path, depth: usize, visited: &mut usize, found: &mut impl FnMut(Entry)) {
    if depth >= MAX_DEPTH || *visited >= MAX_VISITED {
        return;
    }

    let Ok(listing) = std::fs::read_dir(directory) else {
        return;
    };

    for item in listing.flatten() {
        if *visited >= MAX_VISITED {
            return;
        }
        *visited += 1;

        let path = item.path();
        let Some(entry) = Entry::read(&path) else {
            continue;
        };

        // Hidden things, and so the trash: deleted items must not surface in
        // Recents or in a search, or deleting something would not feel like it
        // did anything at all.
        if entry.hidden() {
            continue;
        }

        let is_folder = entry.is_folder();
        found(entry);

        if is_folder {
            // Do not follow symlinked directories: they can point back up the
            // tree, and a search that loops is worse than one that misses.
            let linked = std::fs::symlink_metadata(&path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);
            if !linked {
                walk(&path, depth + 1, visited, found);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::DocKind;
    use crate::test_support::scratch;

    fn vault(name: &str, files: &[&str]) -> std::path::PathBuf {
        let root = scratch(name);
        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"x").unwrap();
        }
        root
    }

    #[test]
    fn an_exact_name_outranks_a_file_that_merely_contains_it() {
        let root = vault(
            "search-rank",
            &["Studio/physics.dpg", "Studio/physics homework.dpg"],
        );
        let hits = search(&root, "physics", 10);

        assert_eq!(hits[0].entry.name, "physics.dpg");
        assert_eq!(hits[0].quality, Quality::Exact);
    }

    #[test]
    fn the_ranking_runs_exact_then_prefix_then_word_then_contains() {
        let root = vault(
            "search-order",
            &[
                "Studio/zzz contains term inside.txt",
                "Studio/term.txt",
                "Studio/termite colony.txt",
                "Studio/spring term.txt",
            ],
        );
        let hits = search(&root, "term", 10);
        let names: Vec<_> = hits.iter().map(|h| h.entry.name.as_str()).collect();

        assert_eq!(names[0], "term.txt", "exact should lead");
        assert_eq!(names[1], "termite colony.txt", "then prefix");
        assert_eq!(names[2], "spring term.txt", "then a word inside");
        assert_eq!(names[3], "zzz contains term inside.txt");
    }

    #[test]
    fn search_finds_things_in_nested_folders() {
        let root = vault("search-deep", &["Studio/Term/Week 1/lab notes.dpg"]);
        let hits = search(&root, "lab", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entry.name, "lab notes.dpg");
    }

    #[test]
    fn search_ignores_case() {
        let root = vault("search-case", &["Studio/Physics.dpg"]);
        assert_eq!(search(&root, "PHYSICS", 10).len(), 1);
        assert_eq!(search(&root, "physics", 10).len(), 1);
    }

    #[test]
    fn scattered_letters_still_find_a_document() {
        let root = vault("search-scatter", &["Studio/Physics Homework.dpg"]);
        let hits = search(&root, "phw", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].quality, Quality::Scattered);
    }

    #[test]
    fn an_empty_query_returns_nothing_rather_than_the_whole_vault() {
        let root = vault("search-empty", &["Studio/a.dpg", "Studio/b.dpg"]);
        assert!(search(&root, "", 10).is_empty());
        assert!(search(&root, "   ", 10).is_empty());
    }

    #[test]
    fn deleted_things_do_not_surface_in_search_or_recents() {
        let root = vault(
            "search-hidden",
            &[".detends-trash/123-secret.dpg", "Studio/visible.dpg"],
        );

        assert!(search(&root, "secret", 10).is_empty());
        let names: Vec<_> = recents(&root, 10).iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, ["visible.dpg"]);
    }

    #[test]
    fn folders_are_searchable_but_recents_shows_only_work() {
        let root = vault("search-folders", &["Studio/Physics/notes.dpg"]);

        let hits = search(&root, "physics", 10);
        assert!(hits.iter().any(|h| h.entry.is_folder()), "folders match too");

        assert!(
            recents(&root, 10).iter().all(|e| !e.is_folder()),
            "Recents is about work, not containers"
        );
    }

    #[test]
    fn a_limit_is_respected() {
        let files: Vec<String> = (0..30).map(|i| format!("Studio/note {i}.dpg")).collect();
        let refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
        let root = vault("search-limit", &refs);

        assert_eq!(search(&root, "note", 5).len(), 5);
        assert_eq!(recents(&root, 7).len(), 7);
    }

    #[test]
    fn documents_can_be_gathered_by_their_studio_type() {
        let root = vault(
            "search-kind",
            &["Studio/a.dpg", "Studio/b.dek", "Studio/c.dgr", "Studio/d.txt"],
        );

        let pages = of_kind(&root, Kind::Document(DocKind::Page), 10);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].name, "a.dpg");

        let decks = of_kind(&root, Kind::Document(DocKind::Deck), 10);
        assert_eq!(decks[0].name, "b.dek");
    }

    #[test]
    fn a_missing_directory_searches_to_nothing_rather_than_failing() {
        let root = scratch("search-missing").join("not-here");
        assert!(search(&root, "anything", 10).is_empty());
        assert!(recents(&root, 10).is_empty());
    }
}
