# Architecture

```
detends-host  (winit)  ──┐
                         ├──▶ shell.tick(now) ──▶ Frame ──▶ render.draw()
detends-compositor  ─────┘
   (smithay, later)
```

Eight crates, dependencies strictly one way.

| Crate | Depends on | Owns |
|---|---|---|
| `detends-motion` | — | The spring solver. No geometry, no platform. |
| `detends-native` | — | CPU kernels in C, C++ and aarch64 assembly, each with a portable twin. |
| `detends-paint` | motion | What a frame *is*: geometry, colour, tokens, the display list. |
| `detends-time` | jiff | Clock, World Clock, alarms, timers, stopwatch. |
| `detends-fs` | jiff | The vault: destinations, entries, file operations, recently deleted. |
| `detends-render` | paint, wgpu, glyphon | Every pixel. Nothing above it knows a GPU exists. |
| `detends-shell` | paint, time, fs | Boot, five modes, Focus, System Center, Search. Never draws. |
| `detends-host` | shell, render, winit | The platform. The only crate that knows what a window is. |

`detends-time` and `detends-fs` are the two domain crates: no GPU, no window, no
rendering, and testable in full without any of them. Clock is built on the first
and Files on the second, which is why both modes are real rather than
placeholder layouts. A mode that has grown a domain crate has stopped being a
sketch.

## The seam

The shell produces an immutable [`Frame`] each tick — a flat list of SDF
primitives, text runs and images, in **logical** units, plus the environment
state. The renderer consumes it. Neither knows anything about the other.

That is the whole point. Porting to Linux means writing a new host crate that
feeds the same `Event`s and presents to a Wayland surface; `detends-shell` and
`detends-render` are untouched. It is also why `detends-paint` has no
dependencies, why `Frame` is `'static + Send`, and why input has its own
vocabulary in the shell rather than being winit's types passed through.

Three properties the display list carries for the compositor's sake, cheap now
and expensive to retrofit:

- **`layer`** — glass cannot composite correctly in one pass, so surfaces are
  drawn layer by layer with a re-blur between. See `GLASS.md`.
- **stable `id`** — lets the renderer cache shaped text across frames, and lets
  animation bind to an element rather than to a list position.
- **`scale_factor`** on the frame, not baked into coordinates — Wayland
  fractional scaling and Retina backing scales are the renderer's problem.

## What runs each frame

1. `Clock::tick` samples absolute time **once**, and everything in the frame is
   evaluated against that instant.
2. `Shell::tick` advances boot, navigation and Focus, then builds the frame.
3. `Renderer::draw` runs the passes in `GLASS.md`.
4. If nothing is animating, the host stops requesting frames entirely.

## Testing

Everything above the renderer is testable without a GPU, and most of it without
waiting in real time: springs are sampled at an arbitrary instant, the clock can
be frozen, and the shell can be driven a frame at a time. `--capture` renders
frames to PNG offscreen, so the material itself can be regression-checked rather
than argued about.
