//! What is playing.
//!
//! Deliberately the smallest description of a piece of music that the interface
//! in §4 actually draws: artwork, track, artist, album, and how long it is.
//! Everything a provider knows beyond that — popularity, ISRC, audio features,
//! market availability — is the provider's business and stops at this boundary.

use serde::{Deserialize, Serialize};

/// A provider's own identifier for something.
///
/// Opaque on purpose. détends never parses one, never builds one, and never
/// assumes a shape: a Spotify URI and a path on disk are both just strings
/// that mean something to whoever issued them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MediaId(pub String);

impl MediaId {
    pub fn new(id: impl Into<String>) -> Self {
        MediaId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One piece of music.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: MediaId,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Seconds. Zero when the provider will not say.
    pub duration: f64,
    /// Where the artwork can be fetched from, if anywhere.
    ///
    /// A URL rather than pixels: fetching and decoding is the host's problem,
    /// and a domain crate that reached for the network to describe a song
    /// would be impossible to test.
    pub artwork: Option<String>,
}

impl Track {
    /// A track with nothing but a title, for tests and placeholders.
    pub fn simple(title: &str, artist: &str, seconds: f64) -> Self {
        Track {
            id: MediaId::new(format!("local:{title}")),
            title: title.to_string(),
            artist: artist.to_string(),
            album: String::new(),
            duration: seconds,
            artwork: None,
        }
    }

    /// `m:ss`, or `h:mm:ss` for anything over an hour.
    pub fn readable_duration(&self) -> String {
        format_time(self.duration)
    }
}

/// Seconds as a clock reading.
///
/// Truncating rather than rounding: a track that has one second left should
/// read `0:01`, not `0:00`, and a position of 59.7s into a minute is still
/// the fifty-ninth second.
pub fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// What to do when the queue runs out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    /// The whole queue, round again.
    All,
    /// This track, until told otherwise.
    One,
}

impl Repeat {
    /// The next state, for a control with one affordance.
    pub fn next(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Repeat::Off => "Repeat off",
            Repeat::All => "Repeat all",
            Repeat::One => "Repeat one",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_as_clock_times() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(9.0), "0:09");
        assert_eq!(format_time(65.0), "1:05");
        assert_eq!(format_time(600.0), "10:00");
        assert_eq!(format_time(3661.0), "1:01:01");
    }

    #[test]
    fn a_partial_second_has_not_elapsed_yet() {
        // 59.7 seconds in is still the fifty-ninth second, not the next minute.
        assert_eq!(format_time(59.7), "0:59");
        assert_eq!(format_time(0.9), "0:00");
    }

    #[test]
    fn a_negative_position_reads_as_the_start_rather_than_as_nonsense() {
        assert_eq!(format_time(-5.0), "0:00");
    }

    #[test]
    fn the_repeat_cycle_returns_to_where_it_started() {
        let mut r = Repeat::Off;
        for _ in 0..3 {
            r = r.next();
        }
        assert_eq!(r, Repeat::Off);
        assert_eq!(Repeat::Off.next(), Repeat::All);
    }

    #[test]
    fn a_track_reports_its_own_length() {
        assert_eq!(Track::simple("Resonance", "HOME", 215.0).readable_duration(), "3:35");
    }
}
