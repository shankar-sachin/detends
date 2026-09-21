**Music, a room worth sitting in, and controls that do something.**

Helium 2.0.0 gave détends files. 2.1.0 gives it sound, a background, and a
System Center that is no longer a picture of a System Center.

## Music (Milestone 6)

Music is real, built on a new domain crate — `detends-music` — the way Clock is
built on `detends-time` and Files on `detends-fs`.

The composition is the one §4 draws: artwork, title, artist, transport,
timeline, and the provider named last and faintest. Shuffle and repeat sit
*outside* the transport row, because they are settings that persist and the
three in the middle are things you do.

**Spotify** is behind it when you connect an account — see
[`documentation/MUSIC.md`](../MUSIC.md). It needs Premium (Spotify's rule) and
a client ID of your own, which takes two minutes and cannot be shipped in a
public repository. Sign-in is PKCE, so there is no client secret to leak: the
browser opens once, the tab closes itself, and the refresh token is kept.

Without an account, Music runs a **local player** and says *On this Mac*. That
provider exists mainly to keep the abstraction honest — a trait with one
implementation is a trait shaped like that implementation, and the only way to
know the interface does not depend on Spotify is to run it on something that is
not Spotify, continuously, in the tests. `spotify.rs` is the only file in
détends that knows the service exists.

The provider runs on its own thread. A frame has eight milliseconds and a
network round trip does not fit in one, so the shell asks for a snapshot and
never waits. The interface is briefly *ahead* of the truth — press play and the
icon changes before Spotify agrees — which is the right trade under §21 and
needs two things to not flicker: a generation counter rather than a state
comparison, and never guessing what "next" will play, because that depends on
shuffle, repeat and a queue the provider owns.

## System Center actually works

It had no hit-testing at all. Every control was a drawing of a control.

Wi-Fi, Bluetooth, Airplane and Focus now toggle; volume and brightness drag.
Asking for a radio while in Airplane Mode leaves Airplane Mode rather than
refusing — making someone turn Airplane off first is the system being pedantic
at them. A drag keeps following the pointer outside the panel and ends on
release, not on leaving the track.

Volume reaches the machine on macOS, applied when the drag ends rather than
sixty times a second. Brightness moves its own state but not yet the display:
there is no supported command-line path to it, and shelling out to a private
framework to move a slider is not a trade worth making.

The panel's layout now comes from one function that drawing and hit-testing
share, so what you see and what you can press cannot drift apart.

## A background

The environment was a flat charcoal vignette — chroma near zero, so the glass
had nothing to refract and everything read as grey.

The **détends mark** is now the wallpaper: enormous, tucked into the corner
nothing else uses, at a contrast where you cannot quite say whether it is there.
It sits in the environment layer, so the blur pyramid picks it up and every pane
of glass in the system carries a trace of it. Focus takes it almost entirely
away.

Centred was tried first and is wrong: every mode puts its content in a centre
column, so a centred mark turns the busiest part of the screen into the
noisiest.

## Fourteen more icons

Shuffle, repeat, heart, queue, library, speaker, muted speaker, devices, Wi-Fi,
Bluetooth, brightness, battery, search and check — thirty-eight in all, drawn
as signed-distance fields at one stroke weight so a transport row and a status
cluster read as the same hand. The sheet is seven across now; it had outgrown
five.

## Still ahead

Album artwork is fetched as a URL but not yet decoded or drawn. Library,
playlists and search exist on the provider but have no surface yet. Mail,
the Studio editors, the Wayland compositor and real device state remain.

---

522 tests. Rust and WGSL, with C, C++ and aarch64 assembly. Set in Inter.
