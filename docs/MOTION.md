# Motion

Animation communicates relationships rather than showing off (rule 10). This
document is the whole vocabulary: four springs, and what each is for.

## The solver

Springs are solved in **closed form** and sampled at **absolute time**, never
integrated and never advanced by a delta. That buys three things:

1. **Frame-rate independence.** 60Hz and 120Hz produce identical results, and a
   dropped frame costs nothing.
2. **No accumulated error.** Position is a function of `t`, so jitter in frame
   delivery causes sub-pixel error that never compounds.
3. **Exact interruption.** Velocity is available in closed form, so retargeting
   mid-flight preserves momentum precisely. Mashing `Super+2`, `Super+4`,
   `Super+1` reads as one continuous movement, never three.

All three damping regimes are solved exactly, including overdamped — the
critical-damping branch is numerically unstable as ζ→1, so a narrow band around
it snaps to the critical solution.

## The four springs

| Name | Stiffness | ζ | Looks arrived | Used by |
|---|---|---|---|---|
| `SNAP` | 420 | 1.00 | 0.32s | toggles, taps, selection |
| `GLIDE` | 210 | 0.92 | 0.38s | mode switches |
| `SETTLE` | 120 | 1.00 | 0.61s | System Center, Search, boot |
| `HUSH` | 70 | 1.00 | 0.79s | wallpaper drift, Focus dimming |

"Looks arrived" is within 1% of the target, and is **scale-invariant** — a panel
sliding 40px and one crossing the screen take the same time to read as *there*.
That is what keeps the system feeling like one object.

`GLIDE` is the only spring permitted overshoot, and only about 3% — enough that
the workspace feels like it has mass, never enough to read as a bounce.

## Rest, and going idle

A spring is at rest when it is within `max(0.0005, 0.00025 × travel)` of its
target *and* has stopped moving. The threshold scales with the travel because
one spring type animates both pixel geometry (which travels a thousand units)
and opacity (which travels one); a threshold tuned for one snaps the other
visibly or never lets it settle.

When every spring has settled and no second has ticked, the shell reports
`animating: false` and the host **stops drawing entirely**. A calm system that
pins a core to redraw a static screen is a contradiction.

## The transitions, named

- **Mode switch** — outgoing recedes to 0.96 and fades; incoming arrives from
  1.04. They overlap, so it reads as one workspace changing state.
- **System Center** — the status cluster's own geometry is interpolated into the
  panel's. The panel *is* the cluster expanding; contents appear only once there
  is room for them.
- **Search** — arrives from 0.94 and slightly high; the workspace behind it
  recedes and dims, so one thing owns attention (rule 2).
- **Boot** — see `§20` and `boot.rs`: the short mark flies in, holds a beat,
  opens outward as the wordmark unfolds, and the workspace rises behind it
  before the mark has finished leaving.
- **Focus** — everything quietens on `HUSH`. Chrome recedes; content does not.

## Reduced motion

An accessibility requirement, not a preference, so it lives in the solver rather
than at call sites: `MotionPreference::Reduced` retunes every spring to a stiff
critically-damped one that arrives in ~120ms with no perceptible travel. Every
state change stays legible; nothing moves.
