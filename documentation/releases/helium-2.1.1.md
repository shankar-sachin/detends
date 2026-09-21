**The windows actually stack now.**

2.1.0 shipped the dock and the window manager. This is the release that makes
them usable: three windows open drew all three apps on top of one another.

## The layering bug

With Surf, Files and Spotify open, every window drew its app over every other
one — Surf's placeholder text rendered *inside* the Spotify window, Files'
listing over both, the transport buried under a file list.

Within one layer the renderer draws the glass pass **before** text and icons,
so anything written in a layer lands on top of every glass surface in that same
layer, including glass belonging to a window in front of it. No amount of
z-ordering fixes that: the stack has to be expressed in **layers**, not in z.

So it is. Windows behind are flat frosted panels drawn whole in the environment
layer; the focused window has the content layer to itself — its glass, then its
chrome, then its app, in that order. Only the focused window runs its app.

For a system built around one thing owning attention that is the right
behaviour anyway, but it is worth being clear it is a constraint first and a
principle second.

The same problem had a second instance: Spotify's artwork was a pane of glass
*inside* a glass window, and came out as a grey smudge. A record sleeve is a
printed thing, so it is drawn as one.

## Less glass

A window is most of the screen. At the transparency that suits a small control
panel you could read the wallpaper through your own document — and every window
behind it as well. Windows now have their own much heavier tint and a far
higher frost, and an unfocused window is solid enough to be a surface rather
than a stain.

## Windows arrive

They grow from 96% and fade in rather than appearing. Growing from a point
reads as a magic trick; a small movement reads as something settling into
place. Restoring from the dock plays the same arrival, so a window coming back
is as legible as one opening.

## Settings

A new app, seventh in the dock, and a deliberately small one (§18): appearance,
how much glass, how much transparency, how much motion, and one line saying
what this is and what it is drawing with.

Every control takes effect the moment it is touched. There is no Apply, because
a setting you have to confirm is a setting you cannot try.

## Album art

Spotify's cover is fetched on the music worker thread when the track changes —
never on the frame — decoded in the host, and uploaded as a texture. JPEG or
PNG, sniffed by magic number rather than trusting a CDN's path to say what it
serves. The shell only ever sees the identifier that comes back.

Verified against the real service rather than asserted: `cargo test -p
detends-host --release -- --ignored artwork` refreshes the token, asks Spotify
what is playing (or takes a cover from the saved library if nothing is),
fetches the image and decodes it with the same function the shell uses. It
currently comes back with a 640×640 JPEG decoded to RGBA8. The test is ignored
by default because it needs an account and the network.

## Fixes worth naming

- A window's pane was keyed on its **position**, so its id changed on every
  frame of a drag and threw away the renderer's cached work for it exactly
  while it was moving. Keyed on the window now.
- Window titles were drawn from a centred rect while being left-aligned, which
  put the name outside its own window.
- Long filenames wrapped and were clipped mid-word. They are shortened with an
  ellipsis that keeps the extension — what a file *is* lives at the end of its
  name.
- `detends-native` finally has a consumer: Settings reports which kernels are
  in use.

## Still ahead

**Surf has no engine.** It opens and says so. A browser means embedding one.

**Mail and Studio** are windows that say what they will be.

**The bootable image does not exist.** The compositor seam has always been
there — `detends-shell` touches neither wgpu nor winit — but a kernel, a
userspace and an image build are all ahead.

---

529 tests. Rust and WGSL, with C, C++ and aarch64 assembly. Set in Inter.
