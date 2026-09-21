//! A provider that needs no account, no network and no permission.
//!
//! It exists for three reasons, and only the third is about playing music.
//!
//! It is the **proof that the abstraction is honest**. A provider trait with
//! exactly one implementation is a trait shaped like that implementation; the
//! only way to know the interface does not depend on Spotify is to run it on
//! something that is not Spotify.
//!
//! It is what Music shows when **no account is connected**, so a fresh install
//! has a working mode rather than an error message.
//!
//! And it is the seam local files will attach to, once there is a decoder
//! behind it. Today it keeps time against a queue without making a sound,
//! which is enough to drive and test every part of the interface.

use crate::playback::{Command, Playback, Status};
use crate::provider::Provider;
use crate::track::{MediaId, Repeat, Track};

/// Plays a fixed queue, silently, in real time.
pub struct LocalProvider {
    tracks: Vec<Track>,
    index: usize,
    playing: bool,
    position: f64,
    shuffle: bool,
    repeat: Repeat,
    volume: f32,
    /// When the position was last advanced.
    last: std::time::Instant,
}

impl Default for LocalProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalProvider {
    pub fn new() -> Self {
        LocalProvider {
            tracks: Vec::new(),
            index: 0,
            playing: false,
            position: 0.0,
            shuffle: false,
            repeat: Repeat::default(),
            volume: 0.7,
            last: std::time::Instant::now(),
        }
    }

    /// The placeholder library, in the real layout (§22).
    ///
    /// The track from the specification's own example, so that what is drawn
    /// matches what the specification describes.
    pub fn placeholder() -> Self {
        let mut provider = Self::new();
        provider.tracks = vec![
            Track::simple("Resonance", "HOME", 215.0),
            Track::simple("Dream Corridor", "HOME", 188.0),
            Track::simple("Half Moon", "Sungazer", 241.0),
            Track::simple("Nocturne", "Chad Lawson", 167.0),
        ];
        provider
    }

    pub fn with_tracks(tracks: Vec<Track>) -> Self {
        let mut provider = Self::new();
        provider.tracks = tracks;
        provider
    }

    fn current(&self) -> Option<&Track> {
        self.tracks.get(self.index)
    }

    /// Move the position forward by however long it has been since last time.
    ///
    /// Wall-clock rather than a fixed step, because `poll` is called whenever
    /// the worker gets round to it and assuming a rate would drift.
    fn tick(&mut self) {
        let elapsed = self.last.elapsed().as_secs_f64();
        self.last = std::time::Instant::now();

        if !self.playing {
            return;
        }

        self.position += elapsed;
        let Some(duration) = self.current().map(|t| t.duration) else {
            return;
        };

        if self.position >= duration && duration > 0.0 {
            match self.repeat {
                Repeat::One => self.position = 0.0,
                _ => self.advance_track(),
            }
        }
    }

    fn advance_track(&mut self) {
        self.position = 0.0;
        if self.tracks.is_empty() {
            return;
        }

        if self.shuffle {
            // Deterministic enough to be testable, varied enough not to feel
            // like a rotation: step by a number coprime with the queue length.
            let step = (self.tracks.len() / 2).max(1);
            self.index = (self.index + step + 1) % self.tracks.len();
            return;
        }

        if self.index + 1 < self.tracks.len() {
            self.index += 1;
        } else if self.repeat == Repeat::All {
            self.index = 0;
        } else {
            // Ran out, and nothing said to go round again.
            self.playing = false;
            self.index = self.tracks.len() - 1;
            self.position = self.current().map(|t| t.duration).unwrap_or(0.0);
        }
    }
}

impl Provider for LocalProvider {
    fn name(&self) -> &str {
        "On this Mac"
    }

    fn poll(&mut self) -> Playback {
        self.tick();

        Playback {
            status: Status::Ready,
            track: self.current().cloned(),
            playing: self.playing,
            position: self.position,
            shuffle: self.shuffle,
            repeat: self.repeat,
            volume: Some(self.volume),
            provider: self.name().to_string(),
            queue: self.tracks.iter().skip(self.index + 1).cloned().collect(),
            device: None,
        }
    }

    fn execute(&mut self, command: Command) -> Result<(), String> {
        self.tick();

        match command {
            Command::PlayPause => {
                if self.tracks.is_empty() {
                    return Err("Nothing to play".into());
                }
                self.playing = !self.playing;
            }
            Command::Next => self.advance_track(),
            Command::Previous => {
                // The convention every player shares: back goes to the start of
                // this track unless you are already near it.
                if self.position > 3.0 {
                    self.position = 0.0;
                } else if self.index > 0 {
                    self.index -= 1;
                    self.position = 0.0;
                } else {
                    self.position = 0.0;
                }
            }
            Command::Seek(fraction) => {
                let duration = self.current().map(|t| t.duration).unwrap_or(0.0);
                self.position = (fraction.clamp(0.0, 1.0) as f64) * duration;
            }
            Command::Volume(v) => self.volume = v.clamp(0.0, 1.0),
            Command::ToggleShuffle => self.shuffle = !self.shuffle,
            Command::CycleRepeat => self.repeat = self.repeat.next(),
            Command::Play(id) => {
                let Some(index) = self.tracks.iter().position(|t| t.id == id) else {
                    return Err("That is not in this library".into());
                };
                self.index = index;
                self.position = 0.0;
                self.playing = true;
            }
            // Nothing to connect to, which is the point of this provider.
            Command::Connect => {}
        }
        Ok(())
    }

    fn search(&mut self, query: &str) -> Vec<Track> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        self.tracks
            .iter()
            .filter(|t| {
                t.title.to_lowercase().contains(&query) || t.artist.to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    fn library(&mut self) -> Vec<Track> {
        self.tracks.clone()
    }

    fn owns(&self, id: &MediaId) -> bool {
        self.tracks.iter().any(|t| t.id == *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> LocalProvider {
        LocalProvider::placeholder()
    }

    #[test]
    fn it_starts_paused_at_the_first_track() {
        let mut p = provider();
        let state = p.poll();
        assert!(!state.playing);
        assert_eq!(state.track.unwrap().title, "Resonance");
        assert_eq!(state.provider, "On this Mac");
    }

    #[test]
    fn play_pause_is_a_toggle() {
        let mut p = provider();
        p.execute(Command::PlayPause).unwrap();
        assert!(p.poll().playing);
        p.execute(Command::PlayPause).unwrap();
        assert!(!p.poll().playing);
    }

    #[test]
    fn next_and_previous_move_through_the_queue() {
        let mut p = provider();
        p.execute(Command::Next).unwrap();
        assert_eq!(p.poll().track.unwrap().title, "Dream Corridor");
        p.execute(Command::Previous).unwrap();
        assert_eq!(p.poll().track.unwrap().title, "Resonance");
    }

    #[test]
    fn previous_restarts_the_track_when_you_are_already_into_it() {
        // The convention every player shares, and the one people rely on.
        let mut p = provider();
        p.execute(Command::Next).unwrap();
        p.execute(Command::Seek(0.5)).unwrap();

        p.execute(Command::Previous).unwrap();
        let state = p.poll();
        assert_eq!(state.track.unwrap().title, "Dream Corridor", "it skipped back too far");
        assert!(state.position < 1.0);
    }

    #[test]
    fn the_queue_is_what_comes_after_this() {
        let mut p = provider();
        let state = p.poll();
        assert_eq!(state.queue.len(), 3);
        assert_eq!(state.queue[0].title, "Dream Corridor");

        p.execute(Command::Next).unwrap();
        assert_eq!(p.poll().queue.len(), 2);
    }

    #[test]
    fn seeking_lands_where_it_was_asked_to() {
        let mut p = provider();
        p.execute(Command::Seek(0.5)).unwrap();
        let state = p.poll();
        assert!((state.position - 107.5).abs() < 1.0, "got {}", state.position);
        assert!((state.progress() - 0.5).abs() < 0.01);
    }

    #[test]
    fn seeking_past_either_end_clamps() {
        let mut p = provider();
        p.execute(Command::Seek(5.0)).unwrap();
        assert!(p.poll().progress() <= 1.0);
        p.execute(Command::Seek(-5.0)).unwrap();
        assert_eq!(p.poll().position, 0.0);
    }

    #[test]
    fn running_off_the_end_stops_rather_than_wrapping() {
        let mut p = LocalProvider::with_tracks(vec![Track::simple("Only", "One", 10.0)]);
        p.execute(Command::PlayPause).unwrap();
        p.execute(Command::Next).unwrap();

        let state = p.poll();
        assert!(!state.playing, "it should have stopped at the end");
    }

    #[test]
    fn repeat_all_goes_round_again() {
        let mut p = LocalProvider::with_tracks(vec![
            Track::simple("A", "x", 10.0),
            Track::simple("B", "x", 10.0),
        ]);
        p.execute(Command::CycleRepeat).unwrap(); // Off -> All
        p.execute(Command::Next).unwrap();
        p.execute(Command::Next).unwrap();

        assert_eq!(p.poll().track.unwrap().title, "A", "it did not wrap");
    }

    #[test]
    fn repeat_one_and_shuffle_are_reported_back() {
        let mut p = provider();
        p.execute(Command::ToggleShuffle).unwrap();
        p.execute(Command::CycleRepeat).unwrap();

        let state = p.poll();
        assert!(state.shuffle);
        assert_eq!(state.repeat, Repeat::All);
    }

    #[test]
    fn playing_something_from_the_library_jumps_to_it() {
        let mut p = provider();
        let id = p.library()[2].id.clone();

        p.execute(Command::Play(id.clone())).unwrap();
        let state = p.poll();
        assert_eq!(state.track.unwrap().id, id);
        assert!(state.playing);
    }

    #[test]
    fn playing_something_it_does_not_have_says_so() {
        let mut p = provider();
        let result = p.execute(Command::Play(MediaId::new("spotify:track:xyz")));
        assert!(result.is_err());
    }

    #[test]
    fn it_only_claims_what_it_actually_has() {
        // A Spotify URI must not be treated as playable here.
        let p = provider();
        assert!(p.owns(&MediaId::new("local:Resonance")));
        assert!(!p.owns(&MediaId::new("spotify:track:xyz")));
    }

    #[test]
    fn search_matches_title_or_artist_and_ignores_case() {
        let mut p = provider();
        assert_eq!(p.search("resonance").len(), 1);
        assert_eq!(p.search("HOME").len(), 2);
        assert!(p.search("").is_empty());
        assert!(p.search("nothing here").is_empty());
    }

    #[test]
    fn an_empty_library_refuses_to_play_rather_than_panicking() {
        let mut p = LocalProvider::new();
        assert!(p.execute(Command::PlayPause).is_err());
        assert!(p.poll().track.is_none());
    }

    #[test]
    fn it_is_usable_as_a_trait_object_like_any_other_provider() {
        // The whole reason this provider exists.
        let mut provider: Box<dyn Provider> = Box::new(LocalProvider::placeholder());
        assert!(provider.execute(Command::PlayPause).is_ok());
        assert!(provider.poll().playing);
    }
}
