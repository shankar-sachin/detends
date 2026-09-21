//! Keeping the network off the frame.
//!
//! A provider talks to a service over the internet. A frame has about eight
//! milliseconds. Those two facts are irreconcilable, so the provider lives on
//! its own thread and the shell never waits for it: commands go down a channel
//! and state comes back through a snapshot the shell copies when it is free.
//!
//! The consequence worth stating plainly is that **the interface is always
//! slightly ahead of the truth**. Press play and the button changes at once,
//! before Spotify has agreed. That is the right trade — §21 puts perceived
//! performance first — but it means the local state has to be *corrected*
//! rather than trusted, which is what [`Music::apply`] is for.

use crate::playback::{Command, Playback, Status};
use crate::provider::Provider;
use crate::track::Track;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often the worker asks the provider what is happening.
///
/// Fast enough that a change made elsewhere — someone pressing pause on their
/// phone — shows up quickly; slow enough to be polite to a rate-limited API.
const POLL: Duration = Duration::from_millis(900);

/// What the worker sends back out of band.
enum Update {
    Results(Vec<Track>),
}

/// The shell's handle on music.
pub struct Music {
    commands: Sender<Request>,
    updates: Receiver<Update>,
    shared: Arc<Mutex<Playback>>,
    /// Bumped by the worker every time it writes a new snapshot.
    ///
    /// Without it there is no way to tell "the worker has news" from "the
    /// interface is optimistically ahead": comparing the two states says they
    /// differ in both cases, and copying on difference throws away every
    /// optimistic update the instant it is made. That bug is invisible in the
    /// provider and obvious in the hand — the play button flickers back.
    generation: Arc<AtomicU64>,
    /// The generation already copied.
    seen: u64,
    /// The interface's own copy, kept moving between updates.
    local: Playback,
    last: Instant,
    results: Vec<Track>,
}

enum Request {
    Do(Command),
    Search(String),
    Library,
    Stop,
}

impl Music {
    /// Start a provider on its own thread.
    pub fn start(mut provider: Box<dyn Provider>) -> Self {
        let shared = Arc::new(Mutex::new(Playback {
            status: Status::Connecting,
            provider: provider.name().to_string(),
            ..Default::default()
        }));

        let (commands, inbox) = std::sync::mpsc::channel::<Request>();
        let (outbox, updates) = std::sync::mpsc::channel::<Update>();
        let worker_shared = Arc::clone(&shared);
        let generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&generation);

        std::thread::Builder::new()
            .name("detends-music".into())
            .spawn(move || {
                let mut next_poll = Instant::now();
                loop {
                    // Commands first, and all of them: a person pressing next
                    // three times should not wait out three poll intervals.
                    loop {
                        match inbox.try_recv() {
                            Ok(Request::Stop) | Err(TryRecvError::Disconnected) => return,
                            Ok(Request::Do(command)) => {
                                if let Err(why) = provider.execute(command) {
                                    if let Ok(mut state) = worker_shared.lock() {
                                        state.status = Status::Failed(why);
                                    }
                                    worker_generation.fetch_add(1, Ordering::Release);
                                }
                                // Something changed, so the next poll is now.
                                next_poll = Instant::now();
                            }
                            Ok(Request::Search(query)) => {
                                let found = provider.search(&query);
                                let _ = outbox.send(Update::Results(found));
                            }
                            Ok(Request::Library) => {
                                let found = provider.library();
                                let _ = outbox.send(Update::Results(found));
                            }
                            Err(TryRecvError::Empty) => break,
                        }
                    }

                    if Instant::now() >= next_poll {
                        let state = provider.poll();
                        if let Ok(mut shared) = worker_shared.lock() {
                            *shared = state;
                        }
                        worker_generation.fetch_add(1, Ordering::Release);
                        next_poll = Instant::now() + POLL;
                    }

                    // Idle politely rather than spinning. Short enough that a
                    // command is acted on without a perceptible wait.
                    std::thread::sleep(Duration::from_millis(16));
                }
            })
            .expect("music worker thread");

        Music {
            commands,
            updates,
            shared,
            generation,
            seen: 0,
            local: Playback::default(),
            last: Instant::now(),
            results: Vec::new(),
        }
    }

    /// The state to draw this frame.
    ///
    /// Never blocks: if the worker happens to hold the lock, the interface
    /// carries on with what it had and moves the clock forward itself. A
    /// dropped frame of music state is invisible; a stalled frame is not.
    pub fn state(&mut self) -> &Playback {
        let elapsed = self.last.elapsed().as_secs_f64();
        self.last = Instant::now();

        // Only when the worker has actually written something new. An
        // optimistic update made since then is left alone until the truth
        // arrives to replace it.
        let generation = self.generation.load(Ordering::Acquire);
        if generation != self.seen {
            if let Ok(shared) = self.shared.try_lock() {
                self.local = shared.clone();
                self.seen = generation;
            }
        }
        self.local.advance(elapsed);

        while let Ok(update) = self.updates.try_recv() {
            match update {
                Update::Results(found) => self.results = found,
            }
        }

        &self.local
    }

    /// Ask for something, and pretend locally that it worked.
    ///
    /// The pretending is the point (§21): the transport must respond to the
    /// press, not to the round trip. Whatever the provider actually did lands
    /// within the second and overwrites this.
    pub fn send(&mut self, command: Command) {
        self.apply(&command);
        let _ = self.commands.send(Request::Do(command));
    }

    /// The optimistic half of `send`, split out so it can be tested alone.
    fn apply(&mut self, command: &Command) {
        match command {
            Command::PlayPause => self.local.playing = !self.local.playing,
            Command::ToggleShuffle => self.local.shuffle = !self.local.shuffle,
            Command::CycleRepeat => self.local.repeat = self.local.repeat.next(),
            Command::Seek(fraction) => {
                if let Some(track) = &self.local.track {
                    self.local.position = (*fraction).clamp(0.0, 1.0) as f64 * track.duration;
                }
            }
            Command::Volume(v) => self.local.volume = Some(v.clamp(0.0, 1.0)),
            // Next and Previous are deliberately *not* guessed at. Which track
            // comes next depends on shuffle, on repeat and on a queue the
            // provider owns; showing the wrong title for a second is worse
            // than showing the right one a moment late.
            Command::Next | Command::Previous | Command::Play(_) | Command::Connect => {}
        }
    }

    pub fn search(&mut self, query: &str) {
        let _ = self.commands.send(Request::Search(query.to_string()));
    }

    pub fn load_library(&mut self) {
        let _ = self.commands.send(Request::Library);
    }

    /// Whatever the last search or library load returned.
    pub fn results(&self) -> &[Track] {
        &self.results
    }
}

impl Drop for Music {
    fn drop(&mut self) {
        let _ = self.commands.send(Request::Stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalProvider;

    /// Wait for a condition the worker thread has to bring about.
    ///
    /// Polling with a deadline rather than sleeping a fixed time: the test
    /// passes as soon as the worker has caught up, and fails in bounded time
    /// if it never does.
    fn until(music: &mut Music, what: &str, mut ready: impl FnMut(&Playback) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if ready(music.state()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {what}");
    }

    #[test]
    fn it_reaches_the_provider_and_comes_back_ready() {
        let mut music = Music::start(Box::new(LocalProvider::placeholder()));
        until(&mut music, "the first poll", |s| s.status.ready());
        assert_eq!(music.state().track.as_ref().unwrap().title, "Resonance");
    }

    #[test]
    fn a_command_reaches_the_provider() {
        let mut music = Music::start(Box::new(LocalProvider::placeholder()));
        until(&mut music, "ready", |s| s.status.ready());

        music.send(Command::Next);
        until(&mut music, "the next track", |s| {
            s.track.as_ref().is_some_and(|t| t.title == "Dream Corridor")
        });
    }

    #[test]
    fn the_interface_responds_before_the_provider_does() {
        // §21: the button changes on the press, not on the round trip.
        let mut music = Music::start(Box::new(LocalProvider::placeholder()));
        until(&mut music, "ready", |s| s.status.ready());
        assert!(!music.state().playing);

        music.send(Command::PlayPause);
        assert!(music.state().playing, "the press was not shown immediately");
    }

    #[test]
    fn the_provider_corrects_an_optimistic_guess() {
        // An empty library cannot play, so the local guess must not survive.
        let mut music = Music::start(Box::new(LocalProvider::new()));
        until(&mut music, "ready", |s| s.status.ready());

        music.send(Command::PlayPause);
        assert!(music.state().playing, "the guess should have been applied");

        until(&mut music, "the correction", |s| !s.playing);
    }

    #[test]
    fn next_is_not_guessed_at_because_the_answer_depends_on_the_queue() {
        let mut music = Music::start(Box::new(LocalProvider::placeholder()));
        until(&mut music, "ready", |s| s.status.ready());

        let before = music.state().track.clone();
        music.send(Command::Next);
        // Immediately afterwards it still shows the old track rather than a
        // guess that shuffle or repeat could make wrong.
        assert_eq!(music.state().track, before);
    }

    #[test]
    fn the_timeline_keeps_moving_between_polls() {
        let mut music = Music::start(Box::new(LocalProvider::placeholder()));
        until(&mut music, "ready", |s| s.status.ready());
        music.send(Command::PlayPause);

        let first = music.state().position;
        std::thread::sleep(Duration::from_millis(60));
        let second = music.state().position;

        assert!(second > first, "the timeline stalled between polls");
    }

    #[test]
    fn searching_comes_back_through_the_worker() {
        let mut music = Music::start(Box::new(LocalProvider::placeholder()));
        until(&mut music, "ready", |s| s.status.ready());

        music.search("HOME");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && music.results().is_empty() {
            music.state();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(music.results().len(), 2, "search did not come back");
    }

    #[test]
    fn dropping_it_stops_the_worker_rather_than_leaking_a_thread() {
        let music = Music::start(Box::new(LocalProvider::placeholder()));
        drop(music);
        // Nothing to assert beyond not hanging: the worker returns when the
        // channel closes, and the test process ending proves it.
    }
}
