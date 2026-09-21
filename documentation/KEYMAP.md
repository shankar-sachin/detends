# The keyboard

détends has no dock and no taskbar (§16). Navigation is the keyboard and Search.

`Super` is Command on macOS, Meta elsewhere — named by role so bindings are
identical on both.

## Navigation

| | |
|---|---|
| `Super + 1` … `Super + 7` | The dock, left to right: Surf · Spotify · Files · Clock · Mail · Studio · Settings |
| `1` … `7` | The same, when nothing is being typed into |
| `Super + W` | Close the window in front |
| `Super + M` | Put it away — it stays in the dock |
| `Super + \`` | Walk the stack |
| `Super + Space` | Universal Search |
| `Escape` | Back out one step |
| `Super + Q` | Leave détends |

The dock is reachable **without** a modifier: a chord the window system might
claim is not a reliable way to reach the apps a system has. A number names a
*dock position* rather than an app, so 1 is always whatever is leftmost, which
is what your hand learns. The exception is the one that matters — while a name
is being typed, into Search or into a field in Files, `1` is the digit one.
"Week 1" has to be a folder a person can name.

`Escape` backs out one step rather than doing one fixed thing: first a name
being typed, then Search or System Center, then the window in front — which it
*minimises* rather than closes, because Escape has to stay safe to press. It
never leaves the system — which is only a safe rule because `Super + Q` does.
An Escape that quits can never be pressed confidently, and every surface it
might dismiss becomes a trap.

It is also matched **before** any app sees the keyboard. Clock, Files and
Spotify each claim bare keys while focused, so a global gesture has to be taken
first or the window becomes a room with no door.

## Spotify

| | |
|---|---|
| `Space` | Play / pause |
| `←` `→` | Previous / next |
| `s` | Shuffle |
| `r` | Repeat: off → all → one |

## Files

Bare keys, reaching whichever window is focused. Nothing here is destructive without a modifier.

| | |
|---|---|
| `←` `→` | Move between the five destinations |
| `↑` `↓` | Move the selection |
| `Enter` | Open — a folder descends, a `.dpg`/`.dek`/`.dgr` goes to Studio |
| `Backspace` | Up, towards the destination |
| `n` | New folder |
| `r` | Rename — or, in Recently Deleted, **restore** |
| `d` | Duplicate |
| `s` | Change the order: name, modified, size, kind |
| `Super + Backspace` | Delete. In Recently Deleted, delete for good |

Deleting is the one destructive key, so it is the one key that wants a modifier
(§9: deleting moves things aside, and Recently Deleted keeps them for 30 days).

While a name is being typed, every key is a character: `Enter` accepts,
`Escape` abandons, and `n`, `r`, `d` and `s` are letters.

## Search

One field. It executes and disappears; there is no result list to choose from.

| Typed | Does |
|---|---|
| `music`, `cl`, `files` | Goes to that place, by prefix |
| `timer 20 minutes` | Starts a timer. `20`, `25m`, `90s`, `1.5 hours` all work |
| `timer 25 minutes deep work` | …and names it |
| `focus`, `focus 45 minutes` | Begins a Focus session |
| `wi-fi`, `bluetooth`, `volume`, `brightness` | Opens System Center |
| `airplane` | Turns Airplane Mode on |
| `lock`, `sleep`, `restart`, `shut down` | Power (§19) |
| `quit`, `exit` | Leave détends — the way out that no window manager can eat |

A bare number means **minutes** — nobody sets a twenty-second timer by saying
"timer 20". Anything unrecognised matches nothing rather than guessing: a wrong
guess executed instantly is worse than no match.

## Planned

Screenshots and recording are shortcuts rather than System Center buttons,
because they are momentary actions, not persistent states (rule 4). Not yet
implemented — Milestone 8.

| | |
|---|---|
| `Super + Shift + 3` | Capture the display |
| `Super + Shift + 4` | Capture a region |
| `Super + Shift + 5` | Capture overlay / recording |
