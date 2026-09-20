# Files, and the vault

> "Files is the system file manager. It must NOT become a Finder/Explorer
> clone… Search should be emphasized over deeply navigating folder trees." (§9)

Two sentences, and the second is the one that shapes the code.

## Destinations, not a tree

A conventional file manager is a *view onto a filesystem*: its model is the
tree, and everything else — sidebars, favourites, tags — is decoration laid on
top. Open one and you are somewhere in a hierarchy, and the way to get anywhere
else is to walk.

détends inverts that. The model is a small fixed set of **destinations**:

| | |
|---|---|
| **Recents** | Everything in the vault, newest first. Not a folder — a question. |
| **Studio** | Where `.dpg`, `.dek` and `.dgr` live |
| **Downloads** | |
| **Screenshots** | Captures land here on their own (§14) |
| **Recently Deleted** | Deleted things, for 30 days |

Folders still exist underneath, and can be opened — pretending otherwise would
be a lie about the disk. But going deeper is something you do occasionally, not
the way you are expected to find anything. There is no sidebar, no column view,
no toolbar and no status bar: one row of destinations, and a listing.

## The vault

All five live in **one directory**, rather than scattered across a Unix home.
That is what lets rule 13 hold — "Linux implementation details remain invisible"
— and it means the vault can be moved, backed up or synced as a single object.

`detends-fs` is a domain crate like `detends-time`: no GPU, no window, no
rendering, and testable in full against a temporary directory. `Vault` is the
only thing that turns a destination into files on a disk; the shell never
touches `std::fs` itself.

A listing is a **snapshot**, read when something changes rather than every
frame. Reading a directory sixty times a second to draw the same rows is exactly
the background work §21 rules out, and a cache would need invalidating by
something that cannot know when the disk changed.

## Three rules that never bend

These are properties of `detends-fs`, not conventions the UI remembers to
follow, because a rule enforced at one call site out of six is not a rule.

**Nothing is silently overwritten.** Every collision resolves to a new name —
`notes.txt` becomes `notes 2.txt`. This holds for creating, renaming, moving,
copying *and* restoring. A user can destroy their work on purpose; they should
never do it by accident. Restoring in particular must never be the operation
that loses data, so a file restored over a newer file of the same name lands
beside it.

**Nothing escapes the vault.** Paths are checked against the root before they
are acted on, so a crafted name cannot walk up out of the vault and act on the
rest of the disk. Symlinked directories are not followed when searching, so a
link pointing back up the tree cannot make a search loop.

**Deleting is reversible.** Delete moves things aside and writes down where they
came from; Recently Deleted keeps them for 30 days and the ledger makes restore
exact rather than approximate. The stored name carries the deletion time and a
counter, so deleting two files called `notes.txt` a second apart does not have
the second quietly destroy the first — which is the accident the whole mechanism
exists to prevent. Expired items are purged when the vault is next opened,
rather than by a timer: no background activity at all.

## Search is ranked, not filtered

"Search should be emphasized over deeply navigating folder trees" is a
requirement on *quality*. A search box only replaces navigation if it reliably
puts the thing you meant at the top; otherwise people go back to clicking
through folders and the tree quietly becomes the real interface after all.

So matches are ranked rather than merely collected:

| | |
|---|---|
| **Exact** | the name, ignoring extension and case |
| **Prefix** | the name starts with the query |
| **Word** | a word inside the name starts with it |
| **Contains** | it appears somewhere |
| **Scattered** | the letters appear in order — `phw` finds `Physics Homework` |

Ties break towards the recently changed, because the thing you are looking for
is usually the thing you were just working on. An empty query returns nothing
rather than everything: a surface that dumps the whole vault the moment it opens
is noise (§15).

Both walks are bounded — eight levels deep, twenty thousand entries — so a vault
with a git checkout in it degrades into a slightly incomplete search rather than
a frozen interface.

## Icons

The three Studio types share one silhouette, a sheet with a turned corner, and
differ only in the mark inside: lines for Page, a slide for Deck, crossed rules
for Grid. The family reads as a family at a glance and the member is legible on
a second look. This is the one place in Files where an icon carries information
rather than decoration.

## Not yet

Move and copy exist in `detends-fs` and are tested, but nothing in the interface
calls them yet: both want a destination to be chosen, and the honest way to do
that is drag and drop or Search, neither of which Files has. Preview is
classified but not drawn. Opening a `.dpg` says what it is and which Studio mode
owns it — Studio cannot edit anything until Milestone 4, and pretending to open
a document would be worse than saying so.

Screenshots have a home and a naming scheme (`Vault::screenshot_path`) before
anything can take one, so that Milestone 8 has somewhere to put them.
