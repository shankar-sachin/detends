//! What the interface draws, and what it can ask for.
//!
//! [`Playback`] is a **snapshot**: everything Music needs to draw one frame,
//! with no provider, no network and no locking behind it. The shell reads one
//! of these per tick and never waits for anybody.
//!
//! [`Command`] is the other direction, and is deliberately tiny. It is the list
//! of things §4 says a person can do to music, and nothing else — no provider
//! reaches through it to offer something only it can do, because the moment one
//! does, the interface has a branch in it for that provider and rule "never let
//! the UI architecture depend on one provider" is gone.

use crate::track::{format_time, Repeat, Track};
use serde::{Deserialize, Serialize};

/// Whether anything is connected, and how it is going.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Status {
    /// No provider has been set up yet.
    #[default]
    Disconnected,
    /// Configured, but the user has not approved access yet.
    ///
    /// Distinct from `Disconnected` because the cure is different: there is an
    /// account waiting, and signing in is one press away rather than a trip to
    /// a developer dashboard.
    NeedsSignIn,
    /// Talking to the provider for the first time.
    Connecting,
    /// Waiting for the user to approve access in a browser.
    Authorising,
    Ready,
    /// Something went wrong, in words that can be shown to a person.
    Failed(String),
}

impl Status {
    pub fn ready(&self) -> bool {
        matches!(self, Status::Ready)
    }

    /// What the interface says when there is nothing to show.
    ///
    /// Never a bare error code: this line is the whole interface when Music has
    /// nothing playing, so it has to tell a person what to do next.
    pub fn message(&self) -> String {
        match self {
            Status::Disconnected => "No music account connected".into(),
            Status::NeedsSignIn => "Press play to connect".into(),
            Status::Connecting => "Connecting…".into(),
            Status::Authorising => "Waiting for approval in your browser…".into(),
            Status::Ready => "Nothing playing".into(),
            Status::Failed(why) => why.clone(),
        }
    }
}

/// Everything Music draws, as of one instant.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Playback {
    pub status: Status,
    pub track: Option<Track>,
    pub playing: bool,
    /// Seconds into the track.
    pub position: f64,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// 0 to 1, or `None` where the provider does not offer volume.
    pub volume: Option<f32>,
    /// What is playing it — "Spotify", "On this Mac". Shown, but secondary (§4).
    pub provider: String,
    /// What is queued after this, nearest first.
    pub queue: Vec<Track>,
    /// Where the sound is coming out, when the provider knows.
    pub device: Option<String>,
}

impl Playback {
    /// How far through the track, 0 to 1.
    ///
    /// Zero-length tracks report zero rather than dividing by zero — a live
    /// stream has no length and its timeline should sit at the start rather
    /// than be full or be `NaN`.
    pub fn progress(&self) -> f32 {
        let Some(track) = &self.track else { return 0.0 };
        if track.duration <= 0.0 {
            return 0.0;
        }
        (self.position / track.duration).clamp(0.0, 1.0) as f32
    }

    /// The elapsed side of the timeline.
    pub fn elapsed(&self) -> String {
        format_time(self.position)
    }

    /// The remaining side, written as a countdown.
    pub fn remaining(&self) -> String {
        let Some(track) = &self.track else {
            return format_time(0.0);
        };
        if track.duration <= 0.0 {
            return String::new();
        }
        format!("-{}", format_time(track.duration - self.position))
    }

    /// Advance the clock locally between provider updates.
    ///
    /// A remote provider is polled every second or so, and a timeline that only
    /// moved when a poll landed would visibly stutter. So the position is
    /// carried forward locally and corrected whenever the truth arrives —
    /// which is also why `position` is never trusted past the track's end.
    pub fn advance(&mut self, seconds: f64) {
        if !self.playing {
            return;
        }
        let limit = self.track.as_ref().map(|t| t.duration).unwrap_or(0.0);
        self.position += seconds;
        if limit > 0.0 {
            self.position = self.position.min(limit);
        }
    }
}

/// Everything a person can ask of music (§4).
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    PlayPause,
    Next,
    Previous,
    /// Jump to a fraction of the current track, 0 to 1.
    Seek(f32),
    /// Set the provider's volume, 0 to 1.
    Volume(f32),
    ToggleShuffle,
    CycleRepeat,
    /// Play something the user chose from a library or a search.
    Play(crate::track::MediaId),
    /// Begin whatever the provider needs to become usable.
    Connect,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::MediaId;

    fn playing(duration: f64, position: f64) -> Playback {
        Playback {
            status: Status::Ready,
            track: Some(Track {
                id: MediaId::new("x"),
                title: "Resonance".into(),
                artist: "HOME".into(),
                album: "Odyssey".into(),
                duration,
                artwork: None,
            }),
            playing: true,
            position,
            ..Default::default()
        }
    }

    #[test]
    fn progress_runs_from_nothing_to_everything() {
        assert_eq!(playing(100.0, 0.0).progress(), 0.0);
        assert_eq!(playing(100.0, 50.0).progress(), 0.5);
        assert_eq!(playing(100.0, 100.0).progress(), 1.0);
    }

    #[test]
    fn a_track_with_no_length_does_not_divide_by_zero() {
        // A live stream. The timeline sits at the start rather than being full
        // or being NaN.
        let live = playing(0.0, 340.0);
        assert_eq!(live.progress(), 0.0);
        assert_eq!(live.remaining(), "");
    }

    #[test]
    fn nothing_playing_is_not_a_special_case() {
        let idle = Playback::default();
        assert_eq!(idle.progress(), 0.0);
        assert_eq!(idle.elapsed(), "0:00");
    }

    #[test]
    fn the_timeline_reads_from_both_ends() {
        let p = playing(215.0, 35.0);
        assert_eq!(p.elapsed(), "0:35");
        assert_eq!(p.remaining(), "-3:00");
    }

    #[test]
    fn the_position_advances_locally_between_updates() {
        // Otherwise the timeline only moves when a poll lands, once a second,
        // and visibly steps.
        let mut p = playing(215.0, 10.0);
        p.advance(0.5);
        assert!((p.position - 10.5).abs() < 1e-9);
    }

    #[test]
    fn a_paused_track_does_not_advance() {
        let mut p = playing(215.0, 10.0);
        p.playing = false;
        p.advance(5.0);
        assert_eq!(p.position, 10.0);
    }

    #[test]
    fn the_position_never_runs_past_the_end_of_the_track() {
        // The provider will say the next track started; until it does, the
        // timeline must sit at the end rather than keep counting.
        let mut p = playing(100.0, 99.0);
        p.advance(30.0);
        assert_eq!(p.position, 100.0);
        assert_eq!(p.progress(), 1.0);
    }

    #[test]
    fn every_status_says_something_a_person_can_act_on() {
        for status in [
            Status::Disconnected,
            Status::NeedsSignIn,
            Status::Connecting,
            Status::Authorising,
            Status::Ready,
            Status::Failed("Spotify said no".into()),
        ] {
            assert!(!status.message().is_empty(), "{status:?} said nothing");
        }
        assert!(Status::Ready.ready());
        assert!(!Status::Connecting.ready());
    }
}
