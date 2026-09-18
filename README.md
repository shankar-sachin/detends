# détends

An operating system shell built on **Five places. One System Center. One Search.
One thing at a time.**

Milestones 1 and 2 of the [specification](docs/): the complete interaction
language, a custom optical-glass renderer, and the beginnings of Clock.

## Running it

```sh
cargo run --release -p detends-host              # fullscreen; Escape exits
cargo run --release -p detends-host -- --windowed
```

Capture frames offscreen without disturbing the display:

```sh
cargo run -p detends-host -- --capture out.png --capture-at 0.4,1.2,2.6
cargo run -p detends-host -- --capture music.png --mode music --open center
```

## What is here

| | |
|---|---|
| **Boot** | The mark flies in, holds, unfolds into the wordmark as the workspace rises behind it |
| **Five modes** | Music · Clock · Mail · Studio · Files, switched with `Super+1…5` |
| **Status cluster** | Focus · Network · Battery · Time. Simplifies itself under Airplane Mode |
| **System Center** | Grows out of the cluster. States only — no screenshot button |
| **Search** | `Super+Space`. Executes and disappears |
| **Focus** | A global state, not a mode. Makes détends quieter rather than announcing itself |
| **Glass** | A real optical slab: height field, refraction, dispersion, Fresnel |

Clock shows the real time. Everything else is placeholder content in its real
layout — the point of Milestone 1 is the interaction language, not the data.

## Reading order

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — the crates and the compositor seam
- [`docs/GLASS.md`](docs/GLASS.md) — the material, and why it is not a blur
- [`docs/MOTION.md`](docs/MOTION.md) — the four springs
- [`docs/KEYMAP.md`](docs/KEYMAP.md) — the keyboard
- [`docs/PROVIDERS.md`](docs/PROVIDERS.md) — Music and Mail account decisions

## Not yet

Wayland compositor · real Wi-Fi/Bluetooth/battery · Studio editors and the DDC
container (`.dpg` `.ddk` `.dgr`) · Music and Mail providers · screenshots ·
alarms, timers and stopwatch. Each has a milestone; the seam each attaches to
exists.

## Licence

MIT or Apache-2.0. Inter is licensed under the SIL Open Font License; see
`assets/fonts/LICENSE.txt`.
