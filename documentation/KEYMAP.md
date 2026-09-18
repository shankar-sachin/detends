# The keyboard

détends has no dock and no taskbar (§16). Navigation is the keyboard and Search.

`Super` is Command on macOS, Meta elsewhere — named by role so bindings are
identical on both.

## Navigation

| | |
|---|---|
| `Super + 1` | Music |
| `Super + 2` | Clock |
| `Super + 3` | Mail |
| `Super + 4` | Studio |
| `Super + 5` | Files |
| `Super + Space` | Universal Search |
| `Escape` | Dismiss Search or System Center |

A **bare** number never navigates — otherwise typing into Search or a document
would teleport the user.

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
