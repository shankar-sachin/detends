//! Music for détends.
//!
//! "Music is a unified music environment… détends does NOT operate its own
//! streaming service… The détends interface should abstract the provider
//! wherever provider APIs and terms permit." (§4)
//!
//! The shape of this crate is that paragraph. A [`Provider`] is something that
//! can play music; [`Playback`] is everything the interface draws; [`Command`]
//! is everything a person can ask for. Nothing above the provider seam names a
//! service, which is the only way the rule "never make the UI architecture
//! depend on one provider" can actually hold rather than being a good
//! intention.
//!
//! Two providers exist. [`LocalProvider`] needs no account and no network, and
//! is what proves the abstraction is honest — a trait with one implementation
//! is a trait shaped like that implementation. [`SpotifyProvider`] is the real
//! one, and it is the only place in détends that knows Spotify exists.
//!
//! Providers run on their own thread ([`Music`]): a frame has eight
//! milliseconds and a network round trip does not.

#![forbid(unsafe_code)]

pub mod engine;
pub mod local;
pub mod playback;
pub mod provider;
pub mod spotify;
pub mod track;

pub use engine::Music;
pub use local::LocalProvider;
pub use playback::{Command, Playback, Status};
pub use provider::Provider;
pub use spotify::{SpotifyCredentials, SpotifyProvider};
pub use track::{format_time, MediaId, Repeat, Track};
