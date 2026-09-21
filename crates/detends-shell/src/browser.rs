//! Files.
//!
//! "Files is the system file manager. It must NOT become a Finder/Explorer
//! clone… Search should be emphasized over deeply navigating folder trees."
//! (§9)
//!
//! So this is one surface, built like Clock: a row of destinations across the
//! top, and whatever is in the current one written underneath. There is no
//! sidebar, no column view, no toolbar and no status bar. A folder can be
//! opened — folders exist, and pretending otherwise would be a lie — but going
//! deeper is something you do occasionally, not the way you are expected to
//! find things.
//!
//! The listing is a snapshot, taken when something changes rather than every
//! frame. Reading a directory sixty times a second to draw the same rows would
//! be exactly the kind of background work §21 rules out.

use crate::notify::Notice;
use detends_fs::{DocKind, Entry, Kind, Place, Sort, Vault};
use detends_paint::{
    space, text, Align, Color, Fill, Frame, Icon, IconShape, Id, Item, Layer, Palette, Primitive,
    Rect, Seconds, Spring, Text, TextStyle, Vec2, ICON_STROKE,
};
use jiff::Timestamp;
use std::path::{Path, PathBuf};

/// What a keystroke in Files asked the rest of the shell to do.
///
/// Files does not open Studio itself — it says that a document was opened and
/// lets the shell decide, which is what keeps the modes independent of each
/// other.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    /// A détends document was opened (§9: `.dpg` → Page, and so on).
    OpenDocument(PathBuf, DocKind),
    /// Something worth a brief word, shown the way any other notice is (§15).
    Say(Notice),
}

/// What the user is typing into, if anything.
#[derive(Clone, Debug, PartialEq)]
enum Editing {
    Nothing,
    /// Renaming the selected entry.
    Rename(String),
    /// Naming a new folder.
    NewFolder(String),
}

/// Icon size in the destination row, and the spacing between rows.
const TAB_ICON: f32 = 29.0;
const TAB_PITCH: f32 = 128.0;
const ROW: f32 = 40.0;
const ROW_ICON: f32 = 19.0;

/// How many rows are drawn at once.
///
/// A listing is not paginated — it scrolls — but only a window of it is ever
/// on screen, and drawing the rest would cost text shaping for nothing.
const VISIBLE: usize = 9;

/// Where the destinations sit, for drawing and for hit-testing alike.
pub fn tabs(area: Rect) -> [(Place, Rect); 5] {
    let y = area.min().y + area.height() * 0.10;
    let pitch = TAB_PITCH.min(area.width() / 6.0);
    let span = pitch * 4.0;
    let start = area.center.x - span * 0.5;

    let mut out = [(Place::Recents, Rect::ZERO); 5];
    for (index, place) in Place::ALL.iter().enumerate() {
        out[index] = (
            *place,
            Rect::from_center_size(
                Vec2 { x: start + index as f32 * pitch, y },
                Vec2::splat(TAB_ICON.min(pitch * 0.30)),
            ),
        );
    }
    out
}

/// Which destination a point lands on.
pub fn hit(area: Rect, at: Vec2) -> Option<Place> {
    tabs(area)
        .iter()
        .find(|(_, rect)| {
            Rect::from_center_size(
                rect.center,
                Vec2 { x: rect.width() + space::WIDE, y: rect.height() + space::OPEN },
            )
            .contains(at)
        })
        .map(|(place, _)| *place)
}

/// The icon a destination is drawn with.
fn place_icon(place: Place) -> IconShape {
    match place {
        Place::Recents => IconShape::Recent,
        Place::Studio => IconShape::Studio,
        Place::Downloads => IconShape::Download,
        Place::Screenshots => IconShape::Screenshot,
        Place::Deleted => IconShape::Trash,
    }
}

/// The icon one entry is drawn with.
///
/// The three Studio types get their own marks (§9), which is the one place in
/// Files where the icon carries information rather than decoration.
fn entry_icon(kind: Kind) -> IconShape {
    match kind {
        Kind::Folder => IconShape::Folder,
        Kind::Document(DocKind::Page) => IconShape::Page,
        Kind::Document(DocKind::Deck) => IconShape::Deck,
        Kind::Document(DocKind::Grid) => IconShape::Grid,
        Kind::Image | Kind::Video => IconShape::Screenshot,
        Kind::Audio => IconShape::Music,
        _ => IconShape::Document,
    }
}


/// Shorten a name that will not fit, with an ellipsis.
///
/// Measured by an average character width rather than by shaping the text,
/// because the shell has no font and cannot ask. Inter's lowercase averages a
/// little over half its size; the estimate is deliberately conservative, so a
/// name is occasionally shortened a character earlier than it had to be and
/// never overruns its column.
///
/// The alternative is what this replaced: the renderer wraps, the second line
/// has nowhere to go, and the name is cut off mid-word with no sign that
/// anything is missing.
fn fit(name: &str, width: f32, size: f32) -> String {
    let per_char = size * 0.55;
    let room = (width / per_char).floor().max(4.0) as usize;

    if name.chars().count() <= room {
        return name.to_string();
    }

    // Keep the extension: "Screenshot 2026-…png" says more than a name cut at
    // a fixed length, because what a file *is* lives at the end of its name.
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, ext)) if ext.len() <= 5 && !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };

    let keep = room.saturating_sub(extension.chars().count() + 1);
    let head: String = stem.chars().take(keep).collect();
    format!("{}…{}", head.trim_end(), extension)
}

/// Files.
pub struct Browser {
    place: usize,
    /// Set when the user has opened a folder inside a destination.
    folder: Option<PathBuf>,
    selected: usize,
    /// First visible row, so a long listing scrolls rather than overflowing.
    top: usize,
    cursor: Spring<f32>,
    editing: Editing,
    /// The snapshot being shown.
    entries: Vec<Entry>,
}

impl Default for Browser {
    fn default() -> Self {
        Self::new()
    }
}

impl Browser {
    pub fn new() -> Self {
        Self {
            place: 0,
            folder: None,
            selected: 0,
            top: 0,
            cursor: Spring::new(detends_paint::springs::SNAP, 0.0),
            editing: Editing::Nothing,
            entries: Vec::new(),
        }
    }

    pub fn place(&self) -> Place {
        Place::ALL[self.place.min(Place::ALL.len() - 1)]
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn selection(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    /// Whether a text field is taking keystrokes.
    ///
    /// The shell asks before treating a bare letter as a shortcut: while a name
    /// is being typed, `d` is a letter and not "delete".
    pub fn is_editing(&self) -> bool {
        self.editing != Editing::Nothing
    }

    /// Where the current listing comes from.
    pub fn directory(&self, vault: &Vault) -> Option<PathBuf> {
        self.folder
            .clone()
            .or_else(|| vault.directory(self.place()))
    }

    /// Re-read the listing.
    ///
    /// Called after anything that could change it, and on entering the mode.
    pub fn refresh(&mut self, vault: &Vault) {
        self.entries = match &self.folder {
            Some(folder) => vault.list_directory(folder),
            None => vault.list(self.place()),
        };
        self.clamp();
    }

    fn clamp(&mut self) {
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        if self.entries.is_empty() {
            self.selected = 0;
            self.top = 0;
            return;
        }
        // Keep the selection inside the window, moving the window as little as
        // it takes — a list that jumps to re-centre is disorienting.
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + VISIBLE {
            self.top = self.selected + 1 - VISIBLE;
        }
        self.top = self.top.min(self.entries.len().saturating_sub(1));
    }

    pub fn select_place(&mut self, now: Seconds, place: Place, vault: &Vault) {
        if let Some(index) = Place::ALL.iter().position(|p| *p == place) {
            self.place = index;
            self.folder = None;
            self.selected = 0;
            self.top = 0;
            self.editing = Editing::Nothing;
            self.cursor.target(now, index as f32);
            self.refresh(vault);
        }
    }

    pub fn step_place(&mut self, now: Seconds, delta: i32, vault: &Vault) {
        let last = Place::ALL.len() as i32 - 1;
        let next = (self.place as i32 + delta).clamp(0, last) as usize;
        if next != self.place {
            self.select_place(now, Place::ALL[next], vault);
        }
    }

    /// Move the selection.
    pub fn step(&mut self, delta: i32) {
        if self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() as i32 - 1;
        self.selected = (self.selected as i32 + delta).clamp(0, last) as usize;
        self.clamp();
    }

    /// Open whatever is selected.
    ///
    /// A folder is descended into; a détends document is handed to the shell;
    /// anything else says what it is rather than pretending to open it, because
    /// Files has nothing honest to do with a `.zip` yet.
    pub fn open(&mut self, vault: &Vault) -> Action {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Action::None;
        };

        if entry.is_folder() {
            // Recently Deleted is a holding area: descending into it would let
            // the user wander inside a deleted folder, which means nothing.
            if self.place() == Place::Deleted {
                return Action::None;
            }
            self.folder = Some(entry.path.clone());
            self.selected = 0;
            self.top = 0;
            self.refresh(vault);
            return Action::None;
        }

        match entry.kind.document() {
            Some(kind) => Action::OpenDocument(entry.path, kind),
            None => {
                let detail = format!("{} · {}", entry.kind.name(), entry.readable_size());
                Action::Say(Notice {
                    title: entry.name,
                    detail,
                    icon: entry_icon(entry.kind),
                })
            }
        }
    }

    /// Leave a folder, back towards the destination.
    pub fn up(&mut self, vault: &Vault) -> bool {
        let Some(folder) = self.folder.clone() else {
            return false;
        };

        let root = vault.directory(self.place());
        let parent = folder.parent().map(Path::to_path_buf);

        self.folder = match (parent, root) {
            // Back at the destination itself: stop holding a folder at all.
            (Some(p), Some(r)) if p == r => None,
            (Some(p), _) if detends_fs::ops::contained(vault.root(), &p) => Some(p),
            _ => None,
        };

        // Land on the folder just left, rather than at the top of the list.
        self.refresh(vault);
        self.selected = self
            .entries
            .iter()
            .position(|e| e.path == folder)
            .unwrap_or(0);
        self.clamp();
        true
    }

    /// Change the order the listing is in (§9).
    pub fn cycle_sort(&mut self, vault: &mut Vault) -> Sort {
        let sort = vault.cycle_sort();
        // Keep the same entry selected as the ground moves under it.
        let keep = self.selection().map(|e| e.path.clone());
        self.refresh(vault);
        if let Some(path) = keep {
            if let Some(index) = self.entries.iter().position(|e| e.path == path) {
                self.selected = index;
            }
        }
        self.clamp();
        sort
    }

    /// Delete the selection — which is to say, move it aside (§9).
    pub fn delete(&mut self, vault: &mut Vault, now: Timestamp) -> Action {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Action::None;
        };

        // In Recently Deleted, the same key means "for good".
        if self.place() == Place::Deleted {
            let stored = vault
                .deleted()
                .into_iter()
                .find(|(_, e)| e.name == entry.name)
                .map(|(d, _)| d.stored);
            if let Some(stored) = stored {
                let _ = vault.purge(&stored);
            }
            self.refresh(vault);
            return Action::Say(Notice {
                title: entry.name,
                detail: "Deleted for good".into(),
                icon: IconShape::Trash,
            });
        }

        match vault.delete(&entry.path, now) {
            Ok(()) => {
                self.refresh(vault);
                Action::Say(Notice {
                    title: entry.name,
                    detail: "Moved to Recently Deleted".into(),
                    icon: IconShape::Trash,
                })
            }
            Err(error) => Action::Say(Notice {
                title: "Could not delete".into(),
                detail: error.to_string(),
                icon: IconShape::Trash,
            }),
        }
    }

    /// Put the selection back where it came from.
    pub fn restore(&mut self, vault: &mut Vault) -> Action {
        if self.place() != Place::Deleted {
            return Action::None;
        }
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Action::None;
        };

        let stored = vault
            .deleted()
            .into_iter()
            .find(|(_, e)| e.name == entry.name)
            .map(|(d, _)| d.stored);

        let Some(stored) = stored else {
            return Action::None;
        };

        let action = match vault.restore(&stored) {
            Ok(path) => Action::Say(Notice {
                title: entry.name,
                detail: format!(
                    "Restored to {}",
                    vault.trail(&path).first().cloned().unwrap_or_default()
                ),
                icon: entry_icon(entry.kind),
            }),
            Err(error) => Action::Say(Notice {
                title: "Could not restore".into(),
                detail: error.to_string(),
                icon: IconShape::Trash,
            }),
        };
        self.refresh(vault);
        action
    }

    /// Duplicate the selection beside itself.
    pub fn duplicate(&mut self, vault: &Vault) -> Action {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Action::None;
        };
        if !self.place().writable() && self.folder.is_none() {
            return Action::None;
        }

        match vault.duplicate(&entry.path) {
            Ok(path) => {
                self.refresh(vault);
                if let Some(index) = self.entries.iter().position(|e| e.path == path) {
                    self.selected = index;
                    self.clamp();
                }
                Action::None
            }
            Err(error) => Action::Say(Notice {
                title: "Could not duplicate".into(),
                detail: error.to_string(),
                icon: entry_icon(entry.kind),
            }),
        }
    }

    // ---- typing ----------------------------------------------------------

    /// Start renaming the selection.
    pub fn begin_rename(&mut self) {
        if let Some(entry) = self.selection() {
            if self.place() == Place::Deleted || self.place() == Place::Recents {
                // Neither is a real folder, so there is nothing to rename in.
                return;
            }
            self.editing = Editing::Rename(entry.name.clone());
        }
    }

    /// Start naming a new folder.
    pub fn begin_new_folder(&mut self) {
        if self.folder.is_some() || self.place().writable() {
            self.editing = Editing::NewFolder(String::new());
        }
    }

    pub fn type_char(&mut self, c: char) {
        match &mut self.editing {
            Editing::Rename(text) | Editing::NewFolder(text) => {
                if !c.is_control() {
                    text.push(c);
                }
            }
            Editing::Nothing => {}
        }
    }

    pub fn backspace(&mut self) {
        match &mut self.editing {
            Editing::Rename(text) | Editing::NewFolder(text) => {
                text.pop();
            }
            Editing::Nothing => {}
        }
    }

    pub fn cancel(&mut self) -> bool {
        if self.editing == Editing::Nothing {
            return false;
        }
        self.editing = Editing::Nothing;
        true
    }

    /// Finish whatever is being typed.
    pub fn commit(&mut self, vault: &Vault) -> Action {
        let editing = std::mem::replace(&mut self.editing, Editing::Nothing);

        let result = match editing {
            Editing::Nothing => return Action::None,
            Editing::Rename(name) => {
                let Some(entry) = self.entries.get(self.selected).cloned() else {
                    return Action::None;
                };
                vault.rename(&entry.path, &name)
            }
            Editing::NewFolder(name) => {
                let Some(parent) = self.directory(vault) else {
                    return Action::None;
                };
                vault.create_folder(&parent, &name)
            }
        };

        match result {
            Ok(path) => {
                self.refresh(vault);
                if let Some(index) = self.entries.iter().position(|e| e.path == path) {
                    self.selected = index;
                    self.clamp();
                }
                Action::None
            }
            // The message from `ops` is already a sentence; show it as one.
            Err(error) => Action::Say(Notice {
                title: "That name will not work".into(),
                detail: error.to_string(),
                icon: IconShape::Folder,
            }),
        }
    }

    pub fn settled(&self, now: Seconds) -> bool {
        self.cursor.at_rest(now)
    }

    /// The line under the destinations saying where you are.
    fn trail_line(&self, vault: &Vault) -> String {
        match &self.folder {
            Some(folder) => vault.trail(folder).join("  ›  "),
            None => match self.place() {
                Place::Recents => "Across the vault".to_string(),
                place => place.name().to_string(),
            },
        }
    }

    /// The right-hand column for one row.
    fn detail(&self, entry: &Entry, vault: &Vault) -> String {
        if self.place() == Place::Deleted {
            // In Recently Deleted the useful fact is how long it has left.
            return vault
                .deleted()
                .into_iter()
                .find(|(_, e)| e.name == entry.name)
                .map(|(d, _)| {
                    let now = Timestamp::from_second(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0),
                    )
                    .unwrap_or(d.deleted_at);
                    d.readout(now)
                })
                .unwrap_or_default();
        }

        if entry.is_folder() {
            return "Folder".to_string();
        }

        // In Recents, where a thing lives matters more than how big it is.
        if self.place() == Place::Recents {
            if let Some(place) = vault.place_of(&entry.path) {
                return place.name().to_string();
            }
        }
        entry.readable_size()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        frame: &mut Frame,
        palette: &Palette,
        area: Rect,
        vault: &Vault,
        now: Seconds,
        opacity: f32,
        scale: f32,
    ) {
        let center = area.center;
        let about = |p: Vec2| Vec2 {
            x: center.x + (p.x - center.x) * scale,
            y: center.y + (p.y - center.y) * scale,
        };

        let write = |frame: &mut Frame,
                     id: Id,
                     label: std::borrow::Cow<'static, str>,
                     at: Vec2,
                     width: f32,
                     style: TextStyle,
                     color: Color,
                     align: Align| {
            frame.push(
                Item::new(
                    id,
                    Layer::Content,
                    Primitive::Text(Text {
                        text: label,
                        rect: Rect::from_center_size(
                            about(at),
                            Vec2 { x: width, y: style.size * 2.0 },
                        ),
                        size: style.size * scale,
                        weight: style.weight,
                        tracking: style.tracking,
                        line_height: style.line_height,
                        color,
                        align,
                    }),
                )
                .opacity(opacity)
                .z(2),
            );
        };

        // The five destinations, as one quiet row — the same shape as Clock's
        // utilities, because they are the same kind of choice.
        let cursor = self.cursor.value(now);
        for (index, (place, rect)) in tabs(area).iter().enumerate() {
            let near = (1.0 - (cursor - index as f32).abs()).clamp(0.0, 1.0);

            frame.push(
                Item::new(
                    Id::of("files-tab").nth(index as u64),
                    Layer::Content,
                    Primitive::Icon(Icon {
                        rect: Rect::from_center_size(
                            about(rect.center),
                            rect.size() * (scale * (1.0 + near * 0.06)),
                        ),
                        shape: place_icon(*place),
                        stroke: ICON_STROKE,
                        color: palette.text_faint.lerp(palette.text, near),
                        rim: 0.25 + near * 0.5,
                    }),
                )
                .opacity(opacity)
                .z(3),
            );

            write(
                frame,
                Id::of("files-tab-label").nth(index as u64),
                place.name().into(),
                Vec2 { x: rect.center.x, y: rect.center.y + rect.half.y + space::ROOM },
                TAB_PITCH,
                text::CAPTION,
                palette
                    .text_faint
                    .lerp(palette.text_soft, near)
                    .fade(0.4 + near * 0.6),
                Align::Center,
            );
        }

        // Where you are — one line, not a bar of buttons to click through.
        //
        // Everything in the listing hangs off these two edges, so the trail,
        // the rows and the detail column line up as one block rather than as
        // three things that happen to be near each other.
        let listing_width = (area.width() * 0.46).clamp(360.0, 620.0);
        let left = center.x - listing_width * 0.5;
        let right = center.x + listing_width * 0.5;
        let head_y = area.min().y + area.height() * 0.24;

        let trail_width = listing_width * 0.6;
        write(
            frame,
            Id::of("files-trail"),
            self.trail_line(vault).into(),
            Vec2 { x: left + trail_width * 0.5, y: head_y },
            trail_width,
            text::CAPTION,
            palette.text_faint,
            Align::Left,
        );

        let sort_width = listing_width * 0.4;
        write(
            frame,
            Id::of("files-sort"),
            format!("By {}", vault.sort().name().to_lowercase()).into(),
            Vec2 { x: right - sort_width * 0.5, y: head_y },
            sort_width,
            text::CAPTION,
            palette.text_faint.fade(0.7),
            Align::Right,
        );

        let top_y = head_y + space::OPEN;

        // A new folder is named on a line of its own above the listing, so the
        // listing does not reflow while it is being typed.
        let mut first_row = top_y;
        if let Editing::NewFolder(typed) = &self.editing {
            self.draw_field(
                frame,
                palette,
                &about,
                Id::of("files-new-folder"),
                "New folder",
                typed,
                Vec2 { x: center.x, y: top_y },
                listing_width,
                opacity,
                scale,
            );
            first_row += ROW;
        }

        if self.entries.is_empty() {
            write(
                frame,
                Id::of("files-empty"),
                self.place().empty_message().into(),
                Vec2 { x: center.x, y: first_row + space::OPEN },
                area.width() * 0.7,
                text::BODY,
                palette.text_faint,
                Align::Center,
            );
            return;
        }

        let visible = self.entries.iter().enumerate().skip(self.top).take(VISIBLE);

        for (index, entry) in visible {
            let row = index - self.top;
            let y = first_row + row as f32 * ROW;
            let chosen = index == self.selected;

            // The selection is a wash of the one accent the system has (§17),
            // not a filled bar — the row should look marked, not boxed in.
            if chosen {
                frame.push(
                    Item::new(
                        Id::of("files-selection"),
                        Layer::Content,
                        Primitive::Fill(Fill {
                            rect: Rect::from_center_size(
                                about(Vec2 { x: center.x, y }),
                                Vec2 { x: listing_width + space::ROOM * 2.0, y: ROW } * scale,
                            ),
                            radius: 11.0 * scale,
                            squircle: 4.0,
                            // A wash, not a filled bar: the row should read as
                            // marked rather than boxed in (§17).
                            color: palette.accent.fade(0.09),
                        }),
                    )
                    .opacity(opacity)
                    .z(1),
                );
            }

            frame.push(
                Item::new(
                    Id::of("files-row-icon").nth(index as u64),
                    Layer::Content,
                    Primitive::Icon(Icon {
                        rect: Rect::from_center_size(
                            about(Vec2 { x: left + ROW_ICON * 0.5, y }),
                            Vec2::splat(ROW_ICON) * scale,
                        ),
                        shape: entry_icon(entry.kind),
                        stroke: ICON_STROKE,
                        color: if chosen { palette.text } else { palette.text_faint },
                        rim: if chosen { 0.6 } else { 0.25 },
                    }),
                )
                .opacity(opacity)
                .z(3),
            );

            let name_x = left + ROW_ICON + space::ROOM;
            let name_width = listing_width * 0.62 - ROW_ICON;

            // The row being renamed becomes the field, in place.
            if chosen {
                if let Editing::Rename(typed) = &self.editing {
                    self.draw_field(
                        frame,
                        palette,
                        &about,
                        Id::of("files-rename"),
                        "Name",
                        typed,
                        Vec2 { x: name_x + name_width * 0.5, y },
                        name_width,
                        opacity,
                        scale,
                    );
                    continue;
                }
            }

            write(
                frame,
                Id::of("files-row-name").nth(index as u64),
                fit(&entry.name, name_width, text::BODY.size).into(),
                Vec2 { x: name_x + name_width * 0.5, y },
                name_width,
                text::BODY,
                if chosen { palette.text } else { palette.text_soft },
                Align::Left,
            );

            let detail_width = listing_width * 0.34;
            write(
                frame,
                Id::of("files-row-detail").nth(index as u64),
                self.detail(entry, vault).into(),
                Vec2 { x: right - detail_width * 0.5, y },
                detail_width,
                text::CAPTION,
                palette.text_faint.fade(if chosen { 1.0 } else { 0.7 }),
                Align::Right,
            );
        }

        // Say that there is more, rather than cutting off silently.
        let shown = self.top + VISIBLE;
        if shown < self.entries.len() {
            write(
                frame,
                Id::of("files-more"),
                format!("{} more", self.entries.len() - shown).into(),
                Vec2 { x: center.x, y: first_row + VISIBLE as f32 * ROW },
                listing_width,
                text::CAPTION,
                palette.text_faint.fade(0.6),
                Align::Center,
            );
        }
    }

    /// One text field, drawn the same way wherever it appears.
    #[allow(clippy::too_many_arguments)]
    fn draw_field(
        &self,
        frame: &mut Frame,
        palette: &Palette,
        about: &impl Fn(Vec2) -> Vec2,
        id: Id,
        placeholder: &'static str,
        typed: &str,
        at: Vec2,
        width: f32,
        opacity: f32,
        scale: f32,
    ) {
        // A caret, so an empty field still looks like somewhere to type.
        let label = if typed.is_empty() {
            placeholder.to_string()
        } else {
            format!("{typed}|")
        };
        let color = if typed.is_empty() {
            palette.text_faint
        } else {
            palette.text
        };

        frame.push(
            Item::new(
                id.nth(1),
                Layer::Content,
                Primitive::Fill(Fill {
                    rect: Rect::from_center_size(
                        about(at),
                        Vec2 { x: width, y: ROW - 6.0 } * scale,
                    ),
                    radius: 9.0 * scale,
                    squircle: 4.0,
                    color: palette.accent.fade(0.10),
                }),
            )
            .opacity(opacity)
            .z(1),
        );

        frame.push(
            Item::new(
                id,
                Layer::Content,
                Primitive::Text(Text {
                    text: label.into(),
                    rect: Rect::from_center_size(
                        about(at),
                        Vec2 { x: width - space::ROOM, y: text::BODY.size * 2.0 },
                    ),
                    size: text::BODY.size * scale,
                    weight: text::BODY.weight,
                    tracking: text::BODY.tracking,
                    line_height: text::BODY.line_height,
                    color,
                    align: Align::Left,
                }),
            )
            .opacity(opacity)
            .z(2),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::vec2;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("detends-browser-{}", std::process::id()))
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A vault with a few things in it, and a browser looking at Studio.
    fn fixture(name: &str, files: &[&str]) -> (Vault, Browser) {
        let vault = Vault::open(scratch(name));
        for file in files {
            let path = vault.root().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            if file.ends_with('/') {
                std::fs::create_dir_all(&path).unwrap();
            } else {
                std::fs::write(&path, b"x").unwrap();
            }
        }
        let mut browser = Browser::new();
        browser.select_place(0.0, Place::Studio, &vault);
        (vault, browser)
    }

    fn stamp(seconds: i64) -> Timestamp {
        Timestamp::from_second(seconds).unwrap()
    }

    #[test]
    fn it_opens_on_recents() {
        assert_eq!(Browser::new().place(), Place::Recents);
    }

    #[test]
    fn the_destinations_are_the_five_from_the_specification() {
        let area = Rect::from_min_size(vec2(0.0, 0.0), vec2(1512.0, 982.0));
        let names: Vec<_> = tabs(area).iter().map(|(p, _)| p.name()).collect();
        assert_eq!(
            names,
            ["Recents", "Studio", "Downloads", "Screenshots", "Recently Deleted"]
        );
    }

    #[test]
    fn clicking_a_destination_selects_it() {
        let area = Rect::from_min_size(vec2(0.0, 0.0), vec2(1512.0, 982.0));
        let (_, rect) = tabs(area)[2];
        assert_eq!(hit(area, rect.center), Some(Place::Downloads));
        assert_eq!(hit(area, vec2(10.0, 900.0)), None);
    }

    #[test]
    fn the_selection_moves_and_stops_at_the_ends() {
        let (_, mut b) = fixture("browser-step", &["Studio/a.dpg", "Studio/b.dpg"]);
        assert_eq!(b.selection().unwrap().name, "a.dpg");

        b.step(1);
        assert_eq!(b.selection().unwrap().name, "b.dpg");
        b.step(1);
        assert_eq!(b.selection().unwrap().name, "b.dpg", "it should not wrap");
        b.step(-5);
        assert_eq!(b.selection().unwrap().name, "a.dpg");
    }

    #[test]
    fn opening_a_folder_descends_and_up_comes_back_to_it() {
        let (vault, mut b) = fixture(
            "browser-descend",
            &["Studio/Term/notes.dpg", "Studio/zzz.dpg"],
        );

        // "Term" is a folder, so it sorts first.
        assert!(b.selection().unwrap().is_folder());
        b.open(&vault);
        assert_eq!(b.entries().len(), 1);
        assert_eq!(b.selection().unwrap().name, "notes.dpg");

        assert!(b.up(&vault));
        assert_eq!(
            b.selection().unwrap().name,
            "Term",
            "it should land on the folder it left"
        );
        assert!(!b.up(&vault), "there is nothing above a destination");
    }

    #[test]
    fn opening_a_document_hands_it_to_the_shell_with_its_studio_type() {
        let (vault, mut b) = fixture("browser-open-doc", &["Studio/physics.dpg"]);

        match b.open(&vault) {
            Action::OpenDocument(path, kind) => {
                assert_eq!(kind, DocKind::Page);
                assert!(path.ends_with("physics.dpg"));
            }
            other => panic!("expected a document to open, got {other:?}"),
        }
    }

    #[test]
    fn opening_an_ordinary_file_says_what_it_is_rather_than_pretending() {
        let (vault, mut b) = fixture("browser-open-other", &["Studio/archive.zip"]);
        assert!(matches!(b.open(&vault), Action::Say(_)));
    }

    #[test]
    fn deleting_moves_to_recently_deleted_and_restoring_brings_it_back() {
        let (mut vault, mut b) = fixture("browser-delete", &["Studio/physics.dpg"]);

        assert!(matches!(b.delete(&mut vault, stamp(0)), Action::Say(_)));
        assert!(b.entries().is_empty(), "Studio should be empty now");

        b.select_place(0.0, Place::Deleted, &vault);
        assert_eq!(b.entries().len(), 1);
        assert_eq!(b.selection().unwrap().name, "physics.dpg");

        b.restore(&mut vault);
        assert!(b.entries().is_empty(), "the trash should be empty now");

        b.select_place(0.0, Place::Studio, &vault);
        assert_eq!(b.entries().len(), 1);
    }

    #[test]
    fn deleting_inside_recently_deleted_removes_it_for_good() {
        let (mut vault, mut b) = fixture("browser-purge", &["Studio/a.dpg"]);
        b.delete(&mut vault, stamp(0));

        b.select_place(0.0, Place::Deleted, &vault);
        b.delete(&mut vault, stamp(0));

        assert!(b.entries().is_empty());
        b.select_place(0.0, Place::Studio, &vault);
        assert!(b.entries().is_empty(), "it must not have come back");
    }

    #[test]
    fn renaming_in_place_keeps_the_selection_on_the_renamed_entry() {
        let (vault, mut b) = fixture("browser-rename", &["Studio/physics.dpg"]);

        b.begin_rename();
        assert!(b.is_editing());
        // The field starts holding the current name, extension and all.
        for _ in 0.."physics.dpg".len() {
            b.backspace();
        }
        for c in "mechanics".chars() {
            b.type_char(c);
        }
        assert_eq!(b.commit(&vault), Action::None);

        assert!(!b.is_editing());
        assert_eq!(b.selection().unwrap().name, "mechanics.dpg");
    }

    #[test]
    fn a_rename_can_be_abandoned() {
        let (vault, mut b) = fixture("browser-rename-cancel", &["Studio/a.dpg"]);

        b.begin_rename();
        b.type_char('z');
        assert!(b.cancel());
        assert!(!b.is_editing());

        b.refresh(&vault);
        assert_eq!(b.selection().unwrap().name, "a.dpg");
        assert!(!b.cancel(), "cancelling nothing changes nothing");
    }

    #[test]
    fn an_impossible_name_is_reported_rather_than_applied() {
        let (vault, mut b) = fixture("browser-bad-name", &["Studio/a.dpg"]);

        b.begin_rename();
        for _ in 0..20 {
            b.backspace();
        }
        assert!(matches!(b.commit(&vault), Action::Say(_)));
        assert!(!b.is_editing(), "a refused name still closes the field");
    }

    #[test]
    fn a_new_folder_is_created_and_selected() {
        let (vault, mut b) = fixture("browser-new-folder", &[]);

        b.begin_new_folder();
        for c in "Term".chars() {
            b.type_char(c);
        }
        b.commit(&vault);

        assert_eq!(b.selection().unwrap().name, "Term");
        assert!(b.selection().unwrap().is_folder());
    }

    #[test]
    fn folders_cannot_be_made_in_recents_or_recently_deleted() {
        let (vault, mut b) = fixture("browser-new-folder-refused", &[]);

        for place in [Place::Recents, Place::Deleted] {
            b.select_place(0.0, place, &vault);
            b.begin_new_folder();
            assert!(!b.is_editing(), "{place:?} should refuse a new folder");
        }
    }

    #[test]
    fn duplicating_leaves_the_copy_selected() {
        let (vault, mut b) = fixture("browser-duplicate", &["Studio/slides.dek"]);

        b.duplicate(&vault);
        assert_eq!(b.entries().len(), 2);
        assert_eq!(b.selection().unwrap().name, "slides 2.dek");
    }

    #[test]
    fn changing_the_order_keeps_the_same_entry_selected() {
        let (mut vault, mut b) = fixture("browser-sort", &["Studio/a.txt", "Studio/b.txt"]);
        std::fs::write(vault.root().join("Studio").join("b.txt"), b"much longer").unwrap();
        b.refresh(&vault);

        b.step(1);
        let before = b.selection().unwrap().name.clone();

        // Cycle right round; the selection must survive every order.
        for _ in 0..Sort::ALL.len() {
            b.cycle_sort(&mut vault);
            assert_eq!(b.selection().unwrap().name, before);
        }
    }

    #[test]
    fn recents_gathers_across_destinations() {
        let (vault, mut b) = fixture(
            "browser-recents",
            &["Studio/a.dpg", "Downloads/b.pdf", "Screenshots/c.png"],
        );

        b.select_place(0.0, Place::Recents, &vault);
        assert_eq!(b.entries().len(), 3);
    }

    #[test]
    fn a_long_listing_scrolls_to_keep_the_selection_in_view() {
        let files: Vec<String> = (0..30).map(|i| format!("Studio/note {i:02}.dpg")).collect();
        let refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
        let (_, mut b) = fixture("browser-scroll", &refs);

        for _ in 0..25 {
            b.step(1);
        }
        assert!(b.top > 0, "the window should have moved");
        assert!(
            b.selected >= b.top && b.selected < b.top + VISIBLE,
            "the selection must stay visible"
        );
    }

    #[test]
    fn it_draws_the_destinations_and_the_listing() {
        let (vault, b) = fixture("browser-draw", &["Studio/physics.dpg"]);
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        let area = Rect::from_min_size(vec2(0.0, 0.0), vec2(1512.0, 982.0)).inset(60.0);

        b.draw(&mut frame, &Palette::dark(), area, &vault, 0.0, 1.0, 1.0);

        let strings: Vec<String> = frame
            .items
            .iter()
            .filter_map(|i| match &i.primitive {
                Primitive::Text(t) => Some(t.text.to_string()),
                _ => None,
            })
            .collect();

        assert!(strings.iter().any(|s| s == "Studio"));
        assert!(strings.iter().any(|s| s == "Recently Deleted"));
        assert!(strings.iter().any(|s| s == "physics.dpg"));
    }

    #[test]
    fn an_empty_destination_says_what_it_is_for() {
        let (vault, b) = fixture("browser-draw-empty", &[]);
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        let area = Rect::from_min_size(vec2(0.0, 0.0), vec2(1512.0, 982.0)).inset(60.0);

        b.draw(&mut frame, &Palette::dark(), area, &vault, 0.0, 1.0, 1.0);

        let strings: Vec<String> = frame
            .items
            .iter()
            .filter_map(|i| match &i.primitive {
                Primitive::Text(t) => Some(t.text.to_string()),
                _ => None,
            })
            .collect();
        assert!(strings.iter().any(|s| s == "No documents yet"), "{strings:?}");
    }

    #[test]
    fn the_three_studio_types_get_three_different_icons() {
        let page = entry_icon(Kind::Document(DocKind::Page));
        let deck = entry_icon(Kind::Document(DocKind::Deck));
        let grid = entry_icon(Kind::Document(DocKind::Grid));

        assert_ne!(page, deck);
        assert_ne!(deck, grid);
        assert_ne!(page, grid);
        assert_ne!(page, entry_icon(Kind::Other));
    }

    #[test]
    fn a_name_too_long_for_its_column_is_shortened_rather_than_wrapped() {
        let long = "Screenshot 2026-09-17 at 17.14.02.png";
        let short = fit(long, 160.0, 15.0);

        assert!(short.chars().count() < long.chars().count());
        assert!(short.contains('…'), "no sign anything was cut: {short}");
        assert!(short.ends_with(".png"), "the extension was lost: {short}");
    }

    #[test]
    fn a_name_that_fits_is_left_alone() {
        assert_eq!(fit("notes.dpg", 400.0, 15.0), "notes.dpg");
    }

    #[test]
    fn shortening_never_panics_on_awkward_names() {
        for name in ["", ".", "a", "…", "no-extension", "x.verylongextension"] {
            let _ = fit(name, 40.0, 15.0);
        }
    }
}
