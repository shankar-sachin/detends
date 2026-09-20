//! What a thing in the vault *is*.
//!
//! Files is not a Finder clone (§9), and the difference starts here: the system
//! does not care about four hundred MIME types. It cares whether something can
//! be opened in Studio, whether it can be previewed, and what to draw. So the
//! taxonomy is deliberately tiny, and the only part of it with real weight is
//! the détends document family.

use std::path::Path;

/// The three détends-native document types (§8).
///
/// These are the extensions the specification fixes: `.dpg`, `.dek`, `.dgr`.
/// They are not renamed DOCX/PPTX/XLSX — they are DDC containers, and the
/// container is what makes them one family rather than three formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DocKind {
    /// `.dpg` — Studio Page.
    Page,
    /// `.dek` — Studio Deck.
    Deck,
    /// `.dgr` — Studio Grid.
    Grid,
}

impl DocKind {
    pub const ALL: [DocKind; 3] = [DocKind::Page, DocKind::Deck, DocKind::Grid];

    /// The extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            DocKind::Page => "dpg",
            DocKind::Deck => "dek",
            DocKind::Grid => "dgr",
        }
    }

    /// The DDC document type stored in the manifest (§8).
    pub fn ddc_type(self) -> &'static str {
        match self {
            DocKind::Page => "page",
            DocKind::Deck => "deck",
            DocKind::Grid => "grid",
        }
    }

    /// What it is called on screen.
    pub fn name(self) -> &'static str {
        match self {
            DocKind::Page => "Page",
            DocKind::Deck => "Deck",
            DocKind::Grid => "Grid",
        }
    }

    /// Recognise one from an extension, with or without its dot.
    ///
    /// Case-insensitive, because a file that arrived from elsewhere as
    /// `NOTES.DPG` is still a Page and refusing to open it would be pedantry.
    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.trim_start_matches('.');
        DocKind::ALL
            .into_iter()
            .find(|k| ext.eq_ignore_ascii_case(k.extension()))
    }
}

/// The whole taxonomy.
///
/// Everything that is not a folder or a détends document is grouped by what a
/// person would do with it, not by format. `Other` is not a failure — most
/// files are ordinary, and saying so is more honest than inventing a category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Sorts first everywhere, so a place reads as structure then contents.
    Folder,
    /// A détends document, openable in Studio.
    Document(DocKind),
    Image,
    Audio,
    Video,
    /// Plain text, Markdown, and the rest of what previews as words.
    Text,
    Pdf,
    Other,
}

impl Kind {
    /// Classify by extension. The caller decides folders, because only the
    /// filesystem knows that and an extension never will.
    pub fn from_extension(ext: &str) -> Self {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();

        if let Some(doc) = DocKind::from_extension(&ext) {
            return Kind::Document(doc);
        }

        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "tiff" | "bmp" | "svg" => {
                Kind::Image
            }
            "mp3" | "m4a" | "flac" | "wav" | "aac" | "ogg" | "opus" | "aiff" => Kind::Audio,
            "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi" => Kind::Video,
            "txt" | "md" | "markdown" | "rtf" | "csv" | "json" | "toml" | "yaml" | "yml" => {
                Kind::Text
            }
            "pdf" => Kind::Pdf,
            _ => Kind::Other,
        }
    }

    /// Classify a path, given whether the filesystem says it is a directory.
    pub fn of(path: &Path, is_dir: bool) -> Self {
        if is_dir {
            return Kind::Folder;
        }
        path.extension()
            .and_then(|e| e.to_str())
            .map(Kind::from_extension)
            .unwrap_or(Kind::Other)
    }

    /// Whether Studio can open it.
    pub fn document(self) -> Option<DocKind> {
        match self {
            Kind::Document(d) => Some(d),
            _ => None,
        }
    }

    /// Whether Files can show it without opening a mode.
    ///
    /// Preview is a glance, not a viewer: it covers the things that can be
    /// rendered into a small surface truthfully. A `.dgr` cannot, so it is not
    /// here even though it is a first-class détends document.
    pub fn previewable(self) -> bool {
        matches!(self, Kind::Image | Kind::Text | Kind::Pdf)
    }

    /// A short description for the metadata line.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Folder => "Folder",
            Kind::Document(d) => d.name(),
            Kind::Image => "Image",
            Kind::Audio => "Audio",
            Kind::Video => "Video",
            Kind::Text => "Text",
            Kind::Pdf => "PDF",
            Kind::Other => "File",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_three_native_extensions_are_the_ones_the_specification_fixes() {
        // .dpg, .dek, .dgr — not .dpage, .ddeck, .dgrid.
        assert_eq!(DocKind::Page.extension(), "dpg");
        assert_eq!(DocKind::Deck.extension(), "dek");
        assert_eq!(DocKind::Grid.extension(), "dgr");
    }

    #[test]
    fn native_documents_are_recognised_with_or_without_a_dot_and_in_any_case() {
        assert_eq!(DocKind::from_extension("dpg"), Some(DocKind::Page));
        assert_eq!(DocKind::from_extension(".dek"), Some(DocKind::Deck));
        assert_eq!(DocKind::from_extension("DGR"), Some(DocKind::Grid));
        assert_eq!(DocKind::from_extension("docx"), None);
    }

    #[test]
    fn a_document_path_classifies_as_its_studio_type() {
        let page = PathBuf::from("/vault/Studio/physics.dpg");
        assert_eq!(Kind::of(&page, false), Kind::Document(DocKind::Page));
        assert_eq!(
            Kind::of(&page, false).document().map(DocKind::ddc_type),
            Some("page")
        );
    }

    #[test]
    fn a_directory_is_a_folder_whatever_it_is_called() {
        // A folder named "archive.dpg" is still a folder. The filesystem wins.
        let path = PathBuf::from("/vault/archive.dpg");
        assert_eq!(Kind::of(&path, true), Kind::Folder);
    }

    #[test]
    fn folders_sort_before_everything_else() {
        let mut kinds = [Kind::Other, Kind::Folder, Kind::Image];
        kinds.sort();
        assert_eq!(kinds[0], Kind::Folder);
    }

    #[test]
    fn an_extensionless_file_is_ordinary_rather_than_an_error() {
        assert_eq!(Kind::of(&PathBuf::from("/vault/LICENCE"), false), Kind::Other);
    }

    #[test]
    fn preview_covers_what_can_be_shown_truthfully_in_a_small_surface() {
        assert!(Kind::Image.previewable());
        assert!(Kind::Text.previewable());
        assert!(!Kind::Folder.previewable());
        assert!(!Kind::Document(DocKind::Grid).previewable());
    }
}
