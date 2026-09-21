# Music, and connecting Spotify

> "Users connect their existing accounts. détends does NOT operate its own
> streaming service. The détends interface should abstract the provider wherever
> provider APIs and terms permit." (§4)

## Connecting Spotify

You need **Spotify Premium**. Controlling playback from another application is
a Premium feature — that is Spotify's rule, and when it bites, détends says so
in those words rather than showing a `403`.

You also need a **client ID of your own**. détends cannot ship one: a client ID
identifies an application to *your* account, and embedding one in a public
repository would be both against Spotify's terms and a secret that is not
secret. It takes about two minutes.

1. Go to <https://developer.spotify.com/dashboard> and **Create app**.
2. Name it anything — "détends" will do.
3. Add exactly this redirect URI:

   ```
   http://127.0.0.1:8888/callback
   ```

   It must be `127.0.0.1`, not `localhost`. Spotify compares the string.
4. Tick **Web API**, save, and copy the **Client ID**. There is no need to
   touch the client secret.
5. Give it to détends, either in a file:

   ```sh
   mkdir -p ~/Library/Application\ Support/detends
   cat > ~/Library/Application\ Support/detends/spotify.json <<'JSON'
   { "client_id": "paste-it-here" }
   JSON
   ```

   or, to try it without editing anything:

   ```sh
   DETENDS_SPOTIFY_CLIENT_ID=paste-it-here cargo run --release -p detends-host
   ```

The first time Music needs the account it opens your browser, you approve it
once, and the tab closes itself. The refresh token is written next to the client
ID so it does not ask again.

**Start playback on some Spotify device first** — your phone, the desktop app,
a speaker. The Web API asks Spotify's own player to do things; it does not
decode audio itself, and with no active device there is nothing to talk to.
détends says "No active Spotify device" rather than failing silently.

Without any of this, Music runs on the local player and says *On this Mac*. It
is a working mode with a placeholder library rather than an error message.

## Why the provider seam looks like it does

§4 says the interface must abstract the provider, and §22 says never to let the
UI architecture depend on one. Those are easy sentences to agree with and easy
to break, so the structure enforces them:

- `Provider` is six methods, and `spotify.rs` is the **only file in détends that
  knows Spotify exists**. Nothing above it names a service.
- `Playback` is what the interface draws; `Command` is everything a person can
  ask for. Both are provider-agnostic, and neither has a hole in it through
  which one provider can offer something the others cannot.
- `LocalProvider` exists mainly to keep the first two honest. A trait with one
  implementation is a trait shaped like that implementation — the only way to
  know Music does not depend on Spotify is to run it on something that is not
  Spotify, continuously, in the tests.

Apple Music is dropped rather than deferred: MusicKit needs an Apple Developer
membership. YouTube Music has no official API. Spotify is the one real
streaming target, which is exactly why the seam matters.

## Threading, and why the interface lies slightly

A frame has about eight milliseconds. A network round trip does not fit in one,
so the provider runs on its own thread: commands go down a channel, state comes
back through a snapshot the shell copies when it is free, and **the shell never
blocks on music**.

The consequence is that the interface is briefly ahead of the truth. Press play
and the icon changes immediately, before Spotify has agreed — §21 puts perceived
performance first. Whatever actually happened lands within the second and
overwrites the guess.

Two details make that work rather than flicker:

- **A generation counter**, not a state comparison. Comparing local and shared
  state cannot distinguish "the worker has news" from "the interface is
  optimistically ahead", so copying on difference throws every optimistic
  update away the instant it is made. The counter says which of the two it is.
- **Next and previous are not guessed at.** What comes next depends on shuffle,
  on repeat and on a queue the provider owns. Showing the wrong title for a
  second is worse than showing the right one a moment late.

## The keyboard

| | |
|---|---|
| `Space` | Play / pause |
| `←` `→` | Previous / next |
| `s` | Shuffle |
| `r` | Repeat: off → all → one |

The transport and the timeline can also be pressed. Shuffle and repeat sit
*outside* the transport row rather than in it: they are settings that persist,
and the three in the middle are things you do.

## Not yet

Album artwork is fetched as a URL by the provider but not yet decoded or drawn —
the artwork panel is a pane of glass with the music mark in it, which is honest
about having no picture where a grey box would just look broken. Library,
playlists and search within Music are implemented on the provider
(`search`, `library`) but have no surface yet; universal Search reaching music
is §13's job. Queue is carried in `Playback` and shown by the local provider
only, because Spotify's queue endpoint is a separate call détends does not make
yet.
