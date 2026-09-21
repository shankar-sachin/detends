//! Persistent system state.
//!
//! "Persistent system states belong in System Center" (rule 3), so this is
//! exactly the set of things the status cluster shows and System Center
//! controls — and nothing else. Momentary actions live in shortcuts and Search
//! (rule 4) and have no place here.
//!
//! Every value is shell-level for now; the real wiring to NetworkManager,
//! BlueZ, UPower and the rest is Milestone 8. The seam is that the shell asks
//! for state through this struct and never talks to a system service directly.

use detends_paint::Seconds;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Network {
    Wifi { bars: u8 },
    Wired,
    Offline,
}

impl Network {
    pub fn label(self) -> &'static str {
        match self {
            Network::Wifi { .. } => "Wi-Fi",
            Network::Wired => "Wired",
            Network::Offline => "Offline",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Battery {
    /// 0 to 1.
    pub level: f32,
    pub charging: bool,
}

impl Battery {
    pub fn percent(&self) -> u8 {
        (self.level.clamp(0.0, 1.0) * 100.0).round() as u8
    }

    /// Whether the level is worth drawing attention to.
    ///
    /// Used sparingly: a battery indicator that changes colour at 40% is
    /// training the user to ignore it.
    pub fn low(&self) -> bool {
        !self.charging && self.level < 0.15
    }
}

#[derive(Clone, Copy, Debug)]
pub struct System {
    pub network: Network,
    pub airplane: bool,
    pub bluetooth: Option<&'static str>,
    pub volume: f32,
    pub brightness: f32,
    pub battery: Battery,
}

impl Default for System {
    fn default() -> Self {
        Self {
            network: Network::Wifi { bars: 3 },
            airplane: false,
            bluetooth: None,
            volume: 0.62,
            brightness: 0.74,
            battery: Battery {
                level: 0.87,
                charging: false,
            },
        }
    }
}

impl System {
    /// Turn every radio off, or restore them.
    ///
    /// Airplane Mode is a first-class control (§11), and the point of it is
    /// that one switch settles every radio — not that the user turns three
    /// things off in sequence.
    pub fn set_airplane(&mut self, on: bool) {
        self.airplane = on;
        if on {
            self.network = Network::Offline;
            self.bluetooth = None;
        } else {
            self.network = Network::Wifi { bars: 3 };
        }
    }

    /// What the status cluster should show.
    ///
    /// Under Airplane Mode the cluster *simplifies itself* to a single glyph
    /// rather than displaying a row of disconnected-state icons (§11) — the
    /// user turned the radios off deliberately and does not need to be told
    /// three times that they are off.
    /// Turn Wi-Fi on or off.
    ///
    /// Turning a radio on leaves Airplane Mode, because asking for Wi-Fi while
    /// in Airplane Mode can only mean one thing, and making the user turn
    /// Airplane off first would be the system being pedantic at them (§11).
    pub fn set_wifi(&mut self, on: bool) {
        if on {
            self.airplane = false;
            self.network = Network::Wifi { bars: 3 };
        } else {
            self.network = Network::Offline;
        }
    }

    pub fn wifi_on(&self) -> bool {
        !self.airplane && !matches!(self.network, Network::Offline)
    }

    /// Turn Bluetooth on or off. Same rule about Airplane Mode.
    pub fn set_bluetooth(&mut self, on: bool) {
        if on {
            self.airplane = false;
            // A name rather than a bare "On": the useful fact about Bluetooth
            // is what it is connected to (§10).
            self.bluetooth = Some("AirPods");
        } else {
            self.bluetooth = None;
        }
    }

    pub fn indicators(&self) -> Vec<&'static str> {
        if self.airplane {
            return vec!["✈︎"];
        }

        let mut out = Vec::with_capacity(2);
        match self.network {
            Network::Wifi { .. } => out.push("Wi-Fi"),
            Network::Wired => out.push("Wired"),
            // Worth saying, because it is unexpected — unlike in Airplane Mode.
            Network::Offline => out.push("Offline"),
        }
        if self.bluetooth.is_some() {
            out.push("Bluetooth");
        }
        out
    }
}

/// A Focus session: a global system state, never a mode (rule 8).
#[derive(Clone, Debug)]
pub struct Focus {
    pub name: Option<String>,
    started: Seconds,
    /// `None` is an indefinite session.
    duration: Option<Seconds>,
}

impl Focus {
    pub fn begin(now: Seconds, name: Option<String>, duration: Option<Seconds>) -> Self {
        Self {
            name,
            started: now,
            duration,
        }
    }

    /// Seconds left, or `None` if the session is indefinite.
    pub fn remaining(&self, now: Seconds) -> Option<Seconds> {
        self.duration.map(|d| (d - (now - self.started)).max(0.0))
    }

    pub fn elapsed(&self, now: Seconds) -> Seconds {
        now - self.started
    }

    pub fn finished(&self, now: Seconds) -> bool {
        self.remaining(now).is_some_and(|r| r <= 0.0)
    }

    /// `42:18`, or `1:02:45` once past an hour.
    pub fn readout(&self, now: Seconds) -> String {
        let seconds = match self.remaining(now) {
            Some(r) => r,
            // An indefinite session counts up instead — there is nothing to
            // count down to.
            None => self.elapsed(now),
        }
        .max(0.0) as u64;

        let (h, m, s) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
        if h > 0 {
            format!("{h}:{m:02}:{s:02}")
        } else {
            format!("{m}:{s:02}")
        }
    }
}

/// The durations Focus offers (§12).
pub const FOCUS_DURATIONS: [(&str, Option<Seconds>); 4] = [
    ("25m", Some(25.0 * 60.0)),
    ("45m", Some(45.0 * 60.0)),
    ("60m", Some(60.0 * 60.0)),
    ("∞", None),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airplane_mode_settles_every_radio_at_once() {
        let mut s = System {
            bluetooth: Some("AirPods"),
            ..Default::default()
        };
        s.set_airplane(true);

        assert_eq!(s.network, Network::Offline);
        assert!(s.bluetooth.is_none());
    }

    #[test]
    fn the_cluster_simplifies_under_airplane_mode() {
        // §11: one glyph, not a row of disconnected states.
        let mut s = System {
            bluetooth: Some("AirPods"),
            ..Default::default()
        };
        assert!(s.indicators().len() >= 2);

        s.set_airplane(true);
        assert_eq!(s.indicators(), vec!["✈︎"]);
    }

    #[test]
    fn being_offline_unexpectedly_is_still_worth_saying() {
        // The distinction that makes the simplification honest: Airplane Mode
        // is deliberate and needs no explanation; losing Wi-Fi is not.
        let s = System {
            network: Network::Offline,
            ..Default::default()
        };
        assert_eq!(s.indicators(), vec!["Offline"]);
    }

    #[test]
    fn leaving_airplane_mode_restores_the_network() {
        let mut s = System::default();
        s.set_airplane(true);
        s.set_airplane(false);
        assert!(matches!(s.network, Network::Wifi { .. }));
        assert!(!s.airplane);
    }

    #[test]
    fn the_battery_only_calls_for_attention_when_it_should() {
        assert!(!Battery {
            level: 0.87,
            charging: false
        }
        .low());
        assert!(!Battery {
            level: 0.40,
            charging: false
        }
        .low());
        assert!(Battery {
            level: 0.09,
            charging: false
        }
        .low());
        // Charging at 9% is not a problem worth a colour change.
        assert!(!Battery {
            level: 0.09,
            charging: true
        }
        .low());
    }

    #[test]
    fn a_timed_focus_session_counts_down_and_finishes() {
        let f = Focus::begin(100.0, Some("Physics homework".into()), Some(25.0 * 60.0));
        assert_eq!(f.readout(100.0), "25:00");
        assert_eq!(f.readout(100.0 + 42.0), "24:18");
        assert!(!f.finished(100.0 + 60.0));

        assert!(f.finished(100.0 + 1500.0));
        assert_eq!(f.readout(100.0 + 9999.0), "0:00", "must not go negative");
    }

    #[test]
    fn an_indefinite_session_counts_up_and_never_finishes() {
        let f = Focus::begin(0.0, None, None);
        assert_eq!(f.readout(0.0), "0:00");
        assert_eq!(f.readout(75.0), "1:15");
        assert!(!f.finished(99999.0));
        assert!(f.remaining(50.0).is_none());
    }

    #[test]
    fn the_readout_grows_an_hours_field_only_when_needed() {
        let f = Focus::begin(0.0, None, Some(2.0 * 3600.0));
        assert_eq!(f.readout(0.0), "2:00:00");
        // Below an hour it drops back to minutes, so the common case stays
        // short and the cluster stays narrow.
        assert_eq!(f.readout(3600.0 + 1.0), "59:59");
    }

    #[test]
    fn focus_offers_the_documented_durations() {
        let labels: Vec<&str> = FOCUS_DURATIONS.iter().map(|(l, _)| *l).collect();
        assert_eq!(labels, vec!["25m", "45m", "60m", "∞"]);
        assert!(
            FOCUS_DURATIONS.last().unwrap().1.is_none(),
            "∞ must be indefinite"
        );
    }
}
