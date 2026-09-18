# Provider notes — Milestones 6 & 7

Not in scope yet. Recorded here so the decisions are not re-litigated later.

## Music (Milestone 6)

| Provider | Status | Reason |
|---|---|---|
| **Spotify** | **Primary target** | Premium account available. Premium is required for playback control via the Web API / Web Playback SDK, so this is the one that can actually work. |
| Apple Music | **Dropped** | MusicKit developer tokens require a paid Apple Developer account. Not available, so not planned. |
| YouTube Music | Deferred | No official API. Every integration is unofficial and breaks without warning. |
| Local files | Candidate first provider | Zero credentials, proves the provider abstraction before any network work. |

Spotify still needs an app registration under the user's own account (client ID + redirect URI) before any code can run. That is a five-minute step, but it is theirs to do — ask before starting Milestone 6.

**The architectural rule stands regardless (§6): never let the UI depend on one provider.** Spotify being the only real target makes it *more* important that the abstraction is honest, not less — otherwise Spotify's model silently becomes the interface's model.

## Mail (Milestone 7)

IMAP with an app password avoids OAuth registration entirely and works against both Gmail and Outlook. That is the path of least resistance for a first real inbox; OAuth can come later if it earns its place.
