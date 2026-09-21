**Files, a real background, and a way out.**

Releases are named for the elements, in order. Hydrogen was the interaction
language and the glass. Helium is the release where détends starts holding
things: your files, and a room worth sitting in.

## Files (Milestone 3)

Files is real, built on a new domain crate — `detends-fs` — the way Clock is
built on `detends-time`. Not a Finder clone: the model is five **destinations**,
not a folder tree.

- **Recents · Studio · Downloads · Screenshots · Recently Deleted**, over a
  single vault directory that can be moved, backed up or synced as one object.
- Open, descend, rename in place, new folder, duplicate, delete, restore, sort
  by name/modified/size/kind, and scrolling.
- **Ranked search** — exact beats prefix beats word beats contains beats a
  loose subsequence, ties breaking towards what you touched last. "Search over
  navigating trees" is a demand on quality, not just on placement.

Three properties hold in the crate rather than in the interface:

1. **Nothing is silently overwritten.** Every collision becomes `notes 2.txt` —
   including on *restore*, so putting something back can never destroy a newer
   file of the same name.
2. **Nothing escapes the vault.** Paths are checked against the root; symlinked
   directories are not followed when searching.
3. **Deleting is reversible.** A ledger records where everything came from, kept
   30 days, purged when the vault is next opened rather than by a timer.

The Studio document family is fixed at **`.dpg` · `.dek` · `.dgr`**, each with
its own icon: one silhouette, three marks.

## A way out

Escape did nothing in Clock, Files or Home. Each mode claimed bare keys and
swallowed everything it did not name, which made three modes rooms with no
door. Escape is now matched before any mode sees the keyboard.

Escape still never quits — it backs out one step and lands Home. That is only a
safe rule because there is now a deliberate way out: **`Super + Q`**, or
`quit` / `exit` in Search, which no window manager can intercept. Power requests
from Search were previously set and never read by the host; they are carried out
now, and Clock's state is written first.

## A room, not a void

The environment was a flat charcoal vignette — chroma near zero, so the glass
had nothing to refract and everything read as grey.

It is now lit by **two coloured lights**, cool one side and warm the other, over
a deeper ground. Added rather than mixed, because light is additive and mixing
toward a hue only greys the ground out. Neither light is nameable as a colour;
you should only be able to say it is not grey. Focus dims them faster than it
contracts the field — the room goes quiet before it goes dark.

Home's marks are larger and further apart, and there are nine new icons.

## More assembly

Three new aarch64 NEON kernels, each verified against a pure-Rust oracle at
every awkward length:

| kernel | speedup |
|---|---|
| `deinterleave_stereo` (LD2) | **2.6×** |
| `interleave_stereo` (ST2) | 1.3× |
| `i16_to_f32` | 1.0× — no gain |

`deinterleave` is the real prize: LD2 splits the two streams inside the
load-store unit, and no compiler will produce it from a strided loop. The last
row stays in the table because it is true — it was written expecting a win by
symmetry with `f32_to_i16` and measured at parity. Saturation is what makes the
forward direction hard for LLVM, and there is nothing to saturate coming back.
Four of six kernels earn their place.

## Still ahead

Wayland compositor · real Wi-Fi/Bluetooth/battery · Studio editors and the DDC
container · Music and Mail providers · screenshots and recording · audio. Move
and copy exist in `detends-fs` and are tested, but nothing in the interface
calls them yet: both want a destination chosen, and the honest affordance is
drag and drop or Search, neither of which Files has.

---

431 tests. Rust and WGSL, with C, C++ and aarch64 assembly. Set in Inter.
