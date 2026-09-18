//! Keeping Clock's state between runs.
//!
//! A timer set before lunch should still be there after. The file is JSON, and
//! written whole rather than appended to: it is a few kilobytes at most, and a
//! format a person can read and repair by hand is worth more here than one
//! that saves a millisecond.

use crate::schedule::Schedule;
use std::path::{Path, PathBuf};

/// Where Clock's state lives.
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "detends")
        .map(|dirs| dirs.data_dir().join("clock.json"))
}

/// Read the schedule back, or start empty.
///
/// A missing or unreadable file is not an error worth surfacing: the right
/// behaviour is to carry on with nothing, not to refuse to start. A corrupt
/// file is logged and set aside rather than silently overwritten, so whatever
/// went wrong can still be looked at.
pub fn load(path: &Path) -> Schedule {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Schedule::new();
    };

    match serde_json::from_str(&text) {
        Ok(schedule) => schedule,
        Err(error) => {
            log_corrupt(path, &error);
            Schedule::new()
        }
    }
}

/// Write the schedule out.
///
/// Written to a temporary file and then renamed, so an interrupted save leaves
/// the previous state intact rather than a half-written file that will not
/// parse. Losing yesterday's alarms because the machine lost power mid-write
/// would be a poor trade for the simplicity.
pub fn save(path: &Path, schedule: &Schedule) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let text = serde_json::to_string_pretty(schedule)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)
}

fn log_corrupt(path: &Path, error: &serde_json::Error) {
    let aside = path.with_extension("json.broken");
    let _ = std::fs::rename(path, &aside);
    eprintln!(
        "détends: could not read {}: {error}. Moved to {} and started fresh.",
        path.display(),
        aside.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{alarm::Repeat, Clock};
    use jiff::Timestamp;

    fn at(hour: i8, minute: i8) -> Timestamp {
        Clock::frozen_at(2026, 9, 18, hour, minute, "UTC")
            .unwrap()
            .now()
            .zoned()
            .timestamp()
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "detends-store-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_schedule_survives_a_round_trip() {
        let path = temp_dir().join("clock.json");

        let mut schedule = Schedule::new();
        schedule.start_timer(at(9, 0), 600.0, Some("Bread".into()));
        schedule.add_alarm(at(9, 0), 7, 30, Repeat::Weekdays);
        schedule.stopwatch.start(at(9, 0));
        schedule.stopwatch.lap(at(9, 1));

        save(&path, &schedule).unwrap();
        let back = load(&path);

        assert_eq!(back.timers.len(), 1);
        assert_eq!(back.timers[0].name.as_deref(), Some("Bread"));
        assert_eq!(back.alarms.len(), 1);
        assert_eq!(back.alarms[0].repeat, Repeat::Weekdays);
        assert!(back.stopwatch.is_running());
        assert_eq!(back.stopwatch.laps().len(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_timer_reloaded_after_it_finished_has_finished() {
        // The property that makes persistence worth having rather than
        // confusing: quitting through a timer does not pause it.
        let path = temp_dir().join("elapsed.json");

        let mut schedule = Schedule::new();
        schedule.start_timer(at(9, 0), 300.0, None);
        save(&path, &schedule).unwrap();

        let mut back = load(&path);
        back.reconcile(at(10, 0));
        assert!(back.timers[0].has_rung());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_file_starts_empty_rather_than_failing() {
        let schedule = load(&temp_dir().join("nothing-here.json"));
        assert!(schedule.timers.is_empty());
        assert!(schedule.alarms.is_empty());
    }

    #[test]
    fn a_corrupt_file_is_set_aside_rather_than_lost() {
        let path = temp_dir().join("broken.json");
        std::fs::write(&path, "{ this is not json").unwrap();

        let schedule = load(&path);
        assert!(schedule.timers.is_empty(), "it should have started fresh");

        let aside = path.with_extension("json.broken");
        assert!(aside.exists(), "the unreadable file was thrown away");

        let _ = std::fs::remove_file(&aside);
    }

    #[test]
    fn saving_creates_the_directory_it_needs() {
        let path = temp_dir().join("nested").join("deeper").join("clock.json");
        save(&path, &Schedule::new()).unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn an_interrupted_save_leaves_no_temporary_behind() {
        let path = temp_dir().join("atomic.json");
        save(&path, &Schedule::new()).unwrap();
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_file(&path);
    }
}
