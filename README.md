# détends — Helium

An operating system shell built on **A few apps. One System Center. One Search.
One thing at a time.**

Helium 2.1 replaced the five modes with apps, windows and a dock. The modes were
environments you switched between; apps are things you open, move and close. What
survived the change is the restraint: seven apps, fixed, no installing, and one
window at a time owning attention.

Releases are named for the elements, in order. This is **Helium** (v2.x);
Lithium is next.

Milestones 1 to 3 and 6 of the [specification](documentation/): the complete
interaction language, a custom optical-glass renderer, Clock and Focus, Files,
and Music with Spotify behind it.

## Running it

```sh
cargo run --release -p detends-host              # fullscreen; Super+Q leaves
cargo run --release -p detends-host -- --windowed
```

Capture frames offscreen without disturbing the display:

```sh
cargo run -p detends-host -- --capture out.png --capture-at 0.4,1.2,2.6
cargo run -p detends-host -- --capture music.png --mode music --open center
cargo run -p detends-host -- --capture files.png --mode files --place studio
```

A capture never touches the real vault or the real schedule: it gets its own
seeded vault under the temporary directory, so a screenshot can be shared
without putting anyone's filenames in it.

## What is here

| | |
|---|---|
| **Boot** | The mark flies in, holds, unfolds into the wordmark as the workspace rises behind it |
| **Dock** | Surf · Spotify · Files · Clock · Mail · Studio · Settings, opened with `Super+1…7` |
| **Windows** | Draggable, with minimise and close. No maximise, no tiling, no resize |
| **Status cluster** | Focus · Network · Battery · Time. Simplifies itself under Airplane Mode |
| **System Center** | Grows out of the cluster. States only — no screenshot button |
| **Search** | `Super+Space`. Executes and disappears |
| **Focus** | A global state, not a mode. Makes détends quieter rather than announcing itself |
| **Glass** | A real optical slab: height field, refraction, dispersion, Fresnel |
| **Clock** | Real time, World Clock, alarms, timers and stopwatch, kept between runs |
| **Files** | Five destinations over a real vault: open, rename, duplicate, delete, restore |
| **Music** | One interface over any provider. Spotify when connected, a local player otherwise |
| **System Center** | Wi-Fi, Bluetooth, Airplane, Focus, volume and brightness — all live |

Clock, Files and Music are real: each is built on a domain crate of its own
(`detends-time`, `detends-fs`, `detends-music`) and acts on real state. Mail and
Studio are still placeholder content in their real layout — the point of
Milestone 1 was the interaction language, not the data.

Music needs a Spotify client ID of your own; see
[`documentation/MUSIC.md`](documentation/MUSIC.md). Without one it runs a local
player, which is a working mode rather than an error message.

## Reading order

- [`documentation/ARCHITECTURE.md`](documentation/ARCHITECTURE.md) — the crates and the compositor seam
- [`documentation/GLASS.md`](documentation/GLASS.md) — the material, and why it is not a blur
- [`documentation/MOTION.md`](documentation/MOTION.md) — the four springs
- [`documentation/KEYMAP.md`](documentation/KEYMAP.md) — the keyboard
- [`documentation/FILES.md`](documentation/FILES.md) — destinations, the vault, and the three rules
- [`documentation/MUSIC.md`](documentation/MUSIC.md) — connecting Spotify, and the provider seam
- [`documentation/PROVIDERS.md`](documentation/PROVIDERS.md) — Music and Mail account decisions

## Not yet

Wayland compositor · real Wi-Fi/Bluetooth/battery · Studio editors and the DDC
container (`.dpg` `.dek` `.dgr`) · Mail providers · screenshots and recording ·
album artwork. Each has a milestone; the seam each attaches to exists —
screenshots already have a home and a naming scheme in the vault, and the
`detends-native` audio kernels are written and measured ahead of anything that
plays them.

System Center's controls all work; volume reaches the machine on macOS.
Brightness moves its own state but not yet the display, which needs the device
layer in Milestone 8.

## Licence

MIT or Apache-2.0. Inter is licensed under the SIL Open Font License; see
`assets/fonts/LICENSE.txt`.
