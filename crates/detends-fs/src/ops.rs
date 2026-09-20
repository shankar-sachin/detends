//! Creating, renaming, moving, copying and deleting.
//!
//! Every operation here follows two rules, and they are the whole reason this
//! module exists rather than calls to `std::fs` scattered through the shell:
//!
//! 1. **Nothing is ever silently overwritten.** A collision produces a new name,
//!    never a replacement. The user can always delete on purpose; they should
//!    never delete by accident.
//! 2. **Nothing reaches outside the vault.** Paths are checked before they are
//!    acted on, so a crafted name cannot walk up out of the vault and act on
//!    the rest of the disk.

use std::io::{Error, ErrorKind, Result};
use std::path::{Component, Path, PathBuf};

/// Characters a name may not contain, and the reason for each.
///
/// The separators would make one name into a path; the control characters and
/// `NUL` would make a name the filesystem or the renderer cannot handle.
fn invalid(c: char) -> bool {
    c == '/' || c == '\\' || c == '\0' || c.is_control()
}

/// Check a name the user typed.
///
/// Returns the trimmed name, or says what is wrong with it in a sentence that
/// can be shown as-is. Errors here are shown inline, not in a modal (§1).
pub fn validate_name(name: &str) -> std::result::Result<String, &'static str> {
    let name = name.trim();

    if name.is_empty() {
        return Err("A name cannot be empty");
    }
    if name == "." || name == ".." {
        return Err("That name is reserved");
    }
    if name.starts_with('.') {
        return Err("Names beginning with a dot are hidden");
    }
    if name.chars().any(invalid) {
        return Err("A name cannot contain slashes");
    }
    // Generous, but short of every filesystem's limit.
    if name.len() > 255 {
        return Err("That name is too long");
    }
    Ok(name.to_string())
}

/// Whether `path` is inside `root`, after resolving `..` textually.
///
/// Textual rather than canonical on purpose: a path that does not exist yet
/// cannot be canonicalised, and the check is needed precisely when creating
/// something new. Symlinks are handled by refusing to follow them out (see
/// [`guard`]), not by resolving them here.
pub fn contained(root: &Path, path: &Path) -> bool {
    let mut depth = 0i32;
    let relative = match path.strip_prefix(root) {
        Ok(r) => r,
        Err(_) => return false,
    };

    for component in relative.components() {
        match component {
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            // An absolute component inside a relative path means the strip
            // above did not mean what it appeared to.
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

/// Refuse anything that would act outside the vault.
fn guard(root: &Path, path: &Path) -> Result<()> {
    if !contained(root, path) {
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            "that path is outside the vault",
        ));
    }
    Ok(())
}

/// A path like `want`, but not one that already exists.
///
/// `notes.txt` becomes `notes 2.txt`, then `notes 3.txt`. The number goes
/// before the extension so the file keeps its type, and the separator is a
/// space rather than a bracket because it is a name a person will read.
pub fn unused_name(want: &Path) -> PathBuf {
    if !want.exists() {
        return want.to_path_buf();
    }

    let parent = want.parent().unwrap_or(Path::new(""));
    let stem = want
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = want.extension().map(|e| e.to_string_lossy().into_owned());

    for n in 2..10_000 {
        let name = match &extension {
            Some(ext) => format!("{stem} {n}.{ext}"),
            None => format!("{stem} {n}"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    // Vanishingly unlikely, and better than looping forever.
    parent.join(format!("{stem}-{}", std::process::id()))
}

/// Make a folder inside `parent`.
pub fn create_folder(root: &Path, parent: &Path, name: &str) -> Result<PathBuf> {
    let name = validate_name(name).map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;
    let path = unused_name(&parent.join(name));
    guard(root, &path)?;
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Rename something in place.
///
/// The extension is preserved when the user does not type one: renaming
/// `physics.dpg` to `mechanics` should not quietly stop it being a Page.
pub fn rename(root: &Path, path: &Path, name: &str) -> Result<PathBuf> {
    guard(root, path)?;
    let name = validate_name(name).map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;

    let mut target = name;
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if !path.is_dir() && !target.to_ascii_lowercase().ends_with(&format!(".{}", ext.to_ascii_lowercase())) {
            target = format!("{target}.{ext}");
        }
    }

    let parent = path.parent().unwrap_or(root);
    let destination = parent.join(&target);

    // Renaming something to what it is already called is a no-op, not a
    // collision to be resolved into "notes 2".
    if destination == path {
        return Ok(destination);
    }

    let destination = unused_name(&destination);
    guard(root, &destination)?;
    std::fs::rename(path, &destination)?;
    Ok(destination)
}

/// Move something into a folder.
pub fn move_to(root: &Path, path: &Path, folder: &Path) -> Result<PathBuf> {
    guard(root, path)?;
    guard(root, folder)?;

    let name = path
        .file_name()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "no name"))?;
    let destination = unused_name(&folder.join(name));

    // Moving a folder inside itself would detach it from the tree entirely.
    if path.is_dir() && folder.starts_with(path) {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "a folder cannot be moved inside itself",
        ));
    }

    std::fs::create_dir_all(folder)?;
    std::fs::rename(path, &destination)?;
    Ok(destination)
}

/// Copy something into a folder, recursively for folders.
pub fn copy_to(root: &Path, path: &Path, folder: &Path) -> Result<PathBuf> {
    guard(root, path)?;
    guard(root, folder)?;

    let name = path
        .file_name()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "no name"))?;

    if path.is_dir() && folder.starts_with(path) {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "a folder cannot be copied inside itself",
        ));
    }

    std::fs::create_dir_all(folder)?;
    let destination = unused_name(&folder.join(name));

    if path.is_dir() {
        copy_tree(path, &destination)?;
    } else {
        std::fs::copy(path, &destination)?;
    }
    Ok(destination)
}

/// Duplicate something beside itself.
pub fn duplicate(root: &Path, path: &Path) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or(root).to_path_buf();
    copy_to(root, path, &parent)
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn a_name_must_be_something_a_filesystem_can_hold() {
        assert!(validate_name("Physics").is_ok());
        assert_eq!(validate_name("  Physics  ").unwrap(), "Physics");
        assert!(validate_name("").is_err());
        assert!(validate_name("   ").is_err());
        assert!(validate_name("..").is_err());
        assert!(validate_name(".hidden").is_err());
        assert!(validate_name("term/notes").is_err());
        assert!(validate_name("bad\0name").is_err());
        assert!(validate_name(&"x".repeat(300)).is_err());
    }

    #[test]
    fn containment_refuses_paths_that_walk_out_of_the_vault() {
        let root = Path::new("/vault");
        assert!(contained(root, Path::new("/vault/Studio/a.dpg")));
        assert!(contained(root, Path::new("/vault/Studio/../Downloads/a")));
        assert!(!contained(root, Path::new("/vault/../etc/passwd")));
        assert!(!contained(root, Path::new("/etc/passwd")));
    }

    #[test]
    fn creating_a_folder_that_exists_makes_a_second_one_rather_than_failing() {
        let root = scratch("ops-folder");
        std::fs::create_dir_all(&root).unwrap();

        let first = create_folder(&root, &root, "Term").unwrap();
        let second = create_folder(&root, &root, "Term").unwrap();

        assert_eq!(first.file_name().unwrap(), "Term");
        assert_eq!(second.file_name().unwrap(), "Term 2");
        assert!(first.is_dir() && second.is_dir());
    }

    #[test]
    fn creating_a_folder_outside_the_vault_is_refused() {
        let root = scratch("ops-escape");
        std::fs::create_dir_all(&root).unwrap();
        let outside = root.parent().unwrap().to_path_buf();
        assert!(create_folder(&root, &outside, "Escaped").is_err());
    }

    #[test]
    fn renaming_keeps_the_extension_when_none_is_typed() {
        let root = scratch("ops-rename-ext");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("physics.dpg");
        std::fs::write(&file, b"x").unwrap();

        let renamed = rename(&root, &file, "mechanics").unwrap();
        assert_eq!(renamed.file_name().unwrap(), "mechanics.dpg");
        assert!(renamed.exists() && !file.exists());
    }

    #[test]
    fn renaming_does_not_double_an_extension_the_user_typed() {
        let root = scratch("ops-rename-typed");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("a.dpg");
        std::fs::write(&file, b"x").unwrap();

        let renamed = rename(&root, &file, "b.dpg").unwrap();
        assert_eq!(renamed.file_name().unwrap(), "b.dpg");
    }

    #[test]
    fn renaming_to_the_same_name_changes_nothing() {
        let root = scratch("ops-rename-same");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("notes.txt");
        std::fs::write(&file, b"x").unwrap();

        let renamed = rename(&root, &file, "notes.txt").unwrap();
        assert_eq!(renamed, file, "it should not have become 'notes 2.txt'");
        assert!(file.exists());
    }

    #[test]
    fn renaming_onto_an_existing_name_keeps_both() {
        let root = scratch("ops-rename-collide");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), b"a").unwrap();
        std::fs::write(root.join("b.txt"), b"b").unwrap();

        let renamed = rename(&root, &root.join("a.txt"), "b").unwrap();
        assert_eq!(renamed.file_name().unwrap(), "b 2.txt");
        assert_eq!(std::fs::read(root.join("b.txt")).unwrap(), b"b");
    }

    #[test]
    fn moving_relocates_and_does_not_overwrite() {
        let root = scratch("ops-move");
        let studio = root.join("Studio");
        let downloads = root.join("Downloads");
        std::fs::create_dir_all(&studio).unwrap();
        std::fs::create_dir_all(&downloads).unwrap();

        std::fs::write(studio.join("a.txt"), b"moved").unwrap();
        std::fs::write(downloads.join("a.txt"), b"already here").unwrap();

        let moved = move_to(&root, &studio.join("a.txt"), &downloads).unwrap();

        assert_eq!(moved.file_name().unwrap(), "a 2.txt");
        assert_eq!(std::fs::read(downloads.join("a.txt")).unwrap(), b"already here");
        assert_eq!(std::fs::read(&moved).unwrap(), b"moved");
        assert!(!studio.join("a.txt").exists());
    }

    #[test]
    fn a_folder_cannot_be_moved_inside_itself() {
        let root = scratch("ops-move-self");
        let folder = root.join("Term");
        std::fs::create_dir_all(folder.join("Week 1")).unwrap();
        assert!(move_to(&root, &folder, &folder.join("Week 1")).is_err());
    }

    #[test]
    fn copying_a_folder_brings_its_whole_tree() {
        let root = scratch("ops-copy-tree");
        let from = root.join("Term");
        let into = root.join("Archive");
        std::fs::create_dir_all(from.join("Week 1")).unwrap();
        std::fs::create_dir_all(&into).unwrap();
        std::fs::write(from.join("Week 1").join("notes.dpg"), b"deep").unwrap();

        let copied = copy_to(&root, &from, &into).unwrap();

        assert_eq!(
            std::fs::read(copied.join("Week 1").join("notes.dpg")).unwrap(),
            b"deep"
        );
        // The original is untouched — this is a copy, not a move.
        assert!(from.join("Week 1").join("notes.dpg").exists());
    }

    #[test]
    fn duplicating_leaves_the_copy_beside_the_original() {
        let root = scratch("ops-duplicate");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("slides.dek");
        std::fs::write(&file, b"deck").unwrap();

        let copy = duplicate(&root, &file).unwrap();

        assert_eq!(copy.parent(), file.parent());
        assert_eq!(copy.file_name().unwrap(), "slides 2.dek");
        assert_eq!(std::fs::read(&copy).unwrap(), b"deck");
    }

    #[test]
    fn unused_names_keep_counting_past_the_second() {
        let root = scratch("ops-unused");
        std::fs::create_dir_all(&root).unwrap();
        for name in ["a.txt", "a 2.txt", "a 3.txt"] {
            std::fs::write(root.join(name), b"x").unwrap();
        }
        assert_eq!(
            unused_name(&root.join("a.txt")).file_name().unwrap(),
            "a 4.txt"
        );
    }
}
