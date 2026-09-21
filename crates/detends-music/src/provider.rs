//! The provider seam.
//!
//! "The détends interface should abstract the provider wherever provider APIs
//! and terms permit… Never make the UI architecture depend on one provider."
//! (§4, §22)
//!
//! That rule is kept by this trait being small and by nothing above it ever
//! naming a provider. The shell holds a `Box<dyn Provider>`; it asks for a
//! [`Playback`] and it sends [`Command`]s. Which service is behind that — or
//! whether there is a service at all — is not a question the interface can
//! ask, which means it cannot come to depend on the answer.
//!
//! Providers run on their own thread (see [`crate::engine`]), so every method
//! here is allowed to block. None of them may be called from a frame.

use crate::playback::{Command, Playback};
use crate::track::{MediaId, Track};

/// Something that can play music.
pub trait Provider: Send {
    /// What to show as the source, secondary to the music itself (§4).
    fn name(&self) -> &str;

    /// The current state of play.
    ///
    /// Called about once a second on the worker thread. May block; may do I/O.
    fn poll(&mut self) -> Playback;

    /// Do something, or explain why not.
    ///
    /// The error is shown to the user as written, so it is a sentence rather
    /// than a code — "Spotify needs Premium to control playback" is useful and
    /// `403` is not.
    fn execute(&mut self, command: Command) -> Result<(), String>;

    /// Anything matching a query, for Search and the library.
    ///
    /// The default is nothing, so a provider that cannot search is still a
    /// perfectly good provider.
    fn search(&mut self, _query: &str) -> Vec<Track> {
        Vec::new()
    }

    /// What the user has saved, for the library.
    fn library(&mut self) -> Vec<Track> {
        Vec::new()
    }

    /// Whether a given thing can be played by this provider.
    ///
    /// Used when a queue outlives a provider change: a Spotify URI means
    /// nothing to a local player, and playing the wrong thing is worse than
    /// playing nothing.
    fn owns(&self, id: &MediaId) -> bool {
        let _ = id;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::Status;

    /// A provider that does nothing, to prove the trait is usable as one.
    struct Silent;

    impl Provider for Silent {
        fn name(&self) -> &str {
            "Nothing"
        }
        fn poll(&mut self) -> Playback {
            Playback {
                status: Status::Ready,
                provider: "Nothing".into(),
                ..Default::default()
            }
        }
        fn execute(&mut self, _command: Command) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn a_provider_can_be_held_as_a_trait_object() {
        // The property that matters: the shell holds `dyn Provider` and never
        // knows which one it has.
        let mut provider: Box<dyn Provider> = Box::new(Silent);
        assert_eq!(provider.name(), "Nothing");
        assert!(provider.poll().status.ready());
        assert!(provider.execute(Command::PlayPause).is_ok());
    }

    #[test]
    fn search_and_library_are_optional() {
        let mut provider: Box<dyn Provider> = Box::new(Silent);
        assert!(provider.search("anything").is_empty());
        assert!(provider.library().is_empty());
    }
}
