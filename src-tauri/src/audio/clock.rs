//! Where the clicks go, counted in samples.
//!
//! Nothing here knows about devices, threads or time on a wall. It is handed a
//! block length and the current parameters, and it says which samples of that
//! block start a click. That makes it the one part of the audio path that can
//! be tested exhaustively, and the tests below are the definition of "in time".
//!
//! Positions are never accumulated by repeated addition. The next click is
//! `anchor + count * period`, where the anchor is re-set only when the period
//! changes, so the error after a thousand beats is the error of one
//! multiplication rather than a thousand additions.

/// What the audio thread reads on every click. Plain values, copied out of
/// atomics by the caller, so the clock itself never touches shared state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    pub bpm: u32,
    pub beats_per_bar: u8,
    pub subdivision: u8,
    pub accent_first: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            bpm: 120,
            beats_per_bar: 4,
            subdivision: 1,
            accent_first: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The first beat of a bar, when accenting is on.
    Accent,
    Beat,
    /// A click between beats.
    Sub,
}

/// One click, relative to the block it falls in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// Frames into the block.
    pub offset: usize,
    /// Absolute frame since the clock started.
    pub at: u64,
    pub bar: u32,
    /// Zero based beat within the bar.
    pub beat: u8,
    /// Zero based click within the beat.
    pub sub: u8,
    pub kind: Kind,
    /// The meter and grid in force for this click. The interface draws from
    /// these rather than from its own idea of the settings, because a meter
    /// change only lands at the next bar and the dots must follow the sound.
    pub beats_per_bar: u8,
    pub subdivision: u8,
    pub bpm: u32,
}

pub struct Clock {
    rate: f64,
    /// Frames already handed out.
    position: u64,
    /// Exact position the current period is counted from.
    anchor: f64,
    /// Clicks since the anchor.
    count: u64,
    /// Tempo and grid the current period was computed from. Kept as integers
    /// so a change is detected exactly rather than by comparing floats.
    period_bpm: u32,
    period_sub: u8,
    /// Meter and grid in force for the bar and beat under way.
    beats_per_bar: u8,
    subdivision: u8,
    /// Counters for the next click.
    bar: u32,
    beat: u8,
    sub: u8,
}

impl Clock {
    pub fn new(rate: u32) -> Self {
        let p = Params::default();
        Self {
            rate: rate.max(1) as f64,
            position: 0,
            anchor: 0.0,
            count: 0,
            period_bpm: p.bpm,
            period_sub: p.subdivision,
            beats_per_bar: p.beats_per_bar,
            subdivision: p.subdivision,
            bar: 0,
            beat: 0,
            sub: 0,
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate as u32
    }

    /// Restarts the pattern so the next block opens with the downbeat.
    pub fn start(&mut self, p: Params) {
        self.anchor = self.position as f64;
        self.count = 0;
        self.period_bpm = p.bpm.max(1);
        self.period_sub = p.subdivision.max(1);
        self.subdivision = self.period_sub;
        self.beats_per_bar = p.beats_per_bar.max(1);
        self.bar = 0;
        self.beat = 0;
        self.sub = 0;
    }

    fn period(&self) -> f64 {
        self.rate * 60.0 / (self.period_bpm as f64 * self.period_sub as f64)
    }

    fn next_exact(&self) -> f64 {
        self.anchor + self.count as f64 * self.period()
    }

    /// Frames from the start of the next block to the next click.
    pub fn frames_to_next(&self) -> u64 {
        (self.next_exact().round() as u64).saturating_sub(self.position)
    }

    /// Moves the clock forward by one block and reports every click that
    /// starts inside it. Allocates nothing and takes no lock; `emit` is where
    /// the caller starts a voice.
    pub fn advance(&mut self, frames: usize, p: Params, mut emit: impl FnMut(Tick)) {
        let end = self.position + frames as u64;
        loop {
            let exact = self.next_exact();
            let at = exact.round() as u64;
            if at >= end {
                break;
            }
            self.adopt(exact, p);
            let kind = if self.sub > 0 {
                Kind::Sub
            } else if self.beat == 0 && p.accent_first {
                Kind::Accent
            } else {
                Kind::Beat
            };
            emit(Tick {
                offset: at.saturating_sub(self.position) as usize,
                at,
                bar: self.bar,
                beat: self.beat,
                sub: self.sub,
                kind,
                beats_per_bar: self.beats_per_bar,
                subdivision: self.subdivision,
                bpm: self.period_bpm,
            });
            self.count_past();
        }
        self.position = end;
    }

    /// Takes whatever changed, each at the boundary it belongs to, as the
    /// click at `exact` is about to sound.
    ///
    /// A new meter waits for a downbeat, because a bar that changes length in
    /// the middle is not a bar. A new grid waits for a beat, otherwise a
    /// triplet would be counted against sixteenths halfway through. A new
    /// tempo takes the interval that starts here, so the click that is
    /// already sounding is never cut short and nothing waits longer than one
    /// interval for a new tempo.
    fn adopt(&mut self, exact: f64, p: Params) {
        if self.sub == 0 {
            if self.beat == 0 {
                self.beats_per_bar = p.beats_per_bar.max(1);
            }
            self.subdivision = p.subdivision.max(1);
        }

        let bpm = p.bpm.max(1);
        if bpm != self.period_bpm || self.subdivision != self.period_sub {
            self.period_bpm = bpm;
            self.period_sub = self.subdivision;
            self.anchor = exact;
            self.count = 1;
        } else {
            self.count += 1;
        }
    }

    fn count_past(&mut self) {
        self.sub += 1;
        if self.sub >= self.subdivision {
            self.sub = 0;
            self.beat += 1;
            if self.beat >= self.beats_per_bar {
                self.beat = 0;
                self.bar = self.bar.wrapping_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(clock: &mut Clock, total: usize, block: usize, p: Params) -> Vec<Tick> {
        let mut out = Vec::new();
        let mut done = 0;
        while done < total {
            let n = block.min(total - done);
            clock.advance(n, p, |t| out.push(t));
            done += n;
        }
        out
    }

    fn started(rate: u32, p: Params) -> Clock {
        let mut c = Clock::new(rate);
        c.start(p);
        c
    }

    /// The whole point of the clock: a thousand beats in, the click is still
    /// on the sample it would have been on if every interval had been exact.
    #[test]
    fn a_thousand_beats_do_not_drift_by_a_sample() {
        for rate in [44_100u32, 48_000, 96_000] {
            for (bpm, subdivision) in [(30, 1), (97, 1), (120, 3), (240, 4), (300, 4)] {
                let p = Params {
                    bpm,
                    subdivision,
                    ..Params::default()
                };
                let period = rate as f64 * 60.0 / (bpm as f64 * subdivision as f64);
                let clicks = 1000 * subdivision as usize;
                let frames = (period * clicks as f64).ceil() as usize + 1;
                // An awkward block size, so clicks land on block edges too.
                let ticks = collect(&mut started(rate, p), frames, 441, p);
                assert!(ticks.len() >= clicks, "{rate} Hz {bpm} bpm lost clicks");
                for (i, t) in ticks.iter().take(clicks).enumerate() {
                    let ideal = period * i as f64;
                    let error = (t.at as f64 - ideal).abs();
                    assert!(
                        error < 1.0,
                        "{rate} Hz {bpm} bpm x{subdivision}: click {i} off by {error} samples"
                    );
                }
            }
        }
    }

    /// The same clicks whatever size the device happens to ask for.
    #[test]
    fn block_size_does_not_move_a_click() {
        let p = Params {
            bpm: 133,
            subdivision: 3,
            ..Params::default()
        };
        let a = collect(&mut started(48_000, p), 480_000, 480, p);
        let b = collect(&mut started(48_000, p), 480_000, 1, p);
        let c = collect(&mut started(48_000, p), 480_000, 4096, p);
        let at = |v: &[Tick]| v.iter().map(|t| t.at).collect::<Vec<_>>();
        assert_eq!(at(&a), at(&c));
        assert_eq!(at(&a), at(&b));
    }

    #[test]
    fn play_starts_with_an_accent_on_the_first_sample() {
        let p = Params::default();
        let mut c = started(48_000, p);
        let mut first = None;
        c.advance(512, p, |t| {
            first.get_or_insert(t);
        });
        let t = first.expect("a click in the first block");
        assert_eq!(t.offset, 0);
        assert_eq!(t.kind, Kind::Accent);
        assert_eq!((t.bar, t.beat, t.sub), (0, 0, 0));
    }

    #[test]
    fn accenting_can_be_turned_off() {
        let p = Params {
            accent_first: false,
            ..Params::default()
        };
        let ticks = collect(&mut started(48_000, p), 48_000 * 4, 256, p);
        assert!(ticks.iter().all(|t| t.kind == Kind::Beat));
    }

    #[test]
    fn beats_and_subdivisions_are_counted_through_the_bar() {
        let p = Params {
            bpm: 120,
            beats_per_bar: 3,
            subdivision: 2,
            accent_first: true,
        };
        // Two bars of 3/4 in eighths at 120: twelve clicks in three seconds.
        let ticks = collect(&mut started(48_000, p), 48_000 * 3, 300, p);
        let pattern: Vec<_> = ticks
            .iter()
            .map(|t| (t.bar, t.beat, t.sub, t.kind))
            .collect();
        assert_eq!(
            pattern,
            vec![
                (0, 0, 0, Kind::Accent),
                (0, 0, 1, Kind::Sub),
                (0, 1, 0, Kind::Beat),
                (0, 1, 1, Kind::Sub),
                (0, 2, 0, Kind::Beat),
                (0, 2, 1, Kind::Sub),
                (1, 0, 0, Kind::Accent),
                (1, 0, 1, Kind::Sub),
                (1, 1, 0, Kind::Beat),
                (1, 1, 1, Kind::Sub),
                (1, 2, 0, Kind::Beat),
                (1, 2, 1, Kind::Sub),
            ]
        );
    }

    /// A new tempo takes the interval after the click that is playing, never
    /// the one already under way.
    #[test]
    fn a_tempo_change_lands_on_the_next_click() {
        let slow = Params {
            bpm: 60,
            ..Params::default()
        };
        let fast = Params {
            bpm: 120,
            ..Params::default()
        };
        let mut c = started(48_000, slow);
        let mut ticks = Vec::new();
        // Half way through the first second, the tempo doubles.
        c.advance(24_000, slow, |t| ticks.push(t));
        c.advance(48_000 * 2 + 1, fast, |t| ticks.push(t));
        let at: Vec<u64> = ticks.iter().map(|t| t.at).collect();
        // 0 at 60 bpm; the interval that was running when the change came
        // still ends at 48 000; from there every interval is 24 000.
        assert_eq!(at, vec![0, 48_000, 72_000, 96_000, 120_000]);
        assert!(ticks.iter().skip(1).all(|t| t.bpm == 120));
    }

    /// A meter change waits for the downbeat, so the bar in progress keeps the
    /// length it started with.
    #[test]
    fn a_meter_change_waits_for_the_next_bar() {
        let four = Params {
            bpm: 240,
            beats_per_bar: 4,
            ..Params::default()
        };
        let three = Params {
            beats_per_bar: 3,
            ..four
        };
        let mut c = started(48_000, four);
        let mut ticks = Vec::new();
        // One and a half beats of 4/4, then the meter becomes 3/4.
        c.advance(18_000, four, |t| ticks.push(t));
        c.advance(48_000 * 3, three, |t| ticks.push(t));
        let beats: Vec<(u32, u8, u8)> = ticks
            .iter()
            .take(11)
            .map(|t| (t.bar, t.beat, t.beats_per_bar))
            .collect();
        assert_eq!(
            beats,
            vec![
                (0, 0, 4),
                (0, 1, 4),
                (0, 2, 4),
                (0, 3, 4),
                (1, 0, 3),
                (1, 1, 3),
                (1, 2, 3),
                (2, 0, 3),
                (2, 1, 3),
                (2, 2, 3),
                (3, 0, 3),
            ]
        );
    }

    /// A new grid starts on a beat, and the beat itself does not move.
    #[test]
    fn a_subdivision_change_waits_for_the_next_beat() {
        let quarters = Params::default();
        let triplets = Params {
            subdivision: 3,
            ..quarters
        };
        let mut c = started(48_000, quarters);
        let mut ticks = Vec::new();
        c.advance(10_000, quarters, |t| ticks.push(t));
        c.advance(48_000, triplets, |t| ticks.push(t));
        let at: Vec<(u64, u8)> = ticks.iter().map(|t| (t.at, t.sub)).collect();
        // Beats at 0 and 24 000 whatever the grid; triplets of 8 000 after.
        assert_eq!(
            at,
            vec![
                (0, 0),
                (24_000, 0),
                (32_000, 1),
                (40_000, 2),
                (48_000, 0),
                (56_000, 1)
            ]
        );
    }

    #[test]
    fn the_ends_of_the_tempo_range_keep_their_period() {
        for (bpm, period) in [(30u32, 96_000u64), (300, 9_600)] {
            let p = Params {
                bpm,
                ..Params::default()
            };
            let ticks = collect(&mut started(48_000, p), 96_000 * 3, 1024, p);
            for pair in ticks.windows(2) {
                assert_eq!(pair[1].at - pair[0].at, period, "{bpm} bpm");
            }
        }
    }

    #[test]
    fn starting_again_resets_the_pattern_without_resetting_time() {
        let p = Params::default();
        let mut c = started(48_000, p);
        let mut ticks = Vec::new();
        c.advance(30_000, p, |t| ticks.push(t));
        c.start(p);
        c.advance(512, p, |t| ticks.push(t));
        let last = ticks.last().unwrap();
        assert_eq!(last.at, 30_000);
        assert_eq!(last.offset, 0);
        assert_eq!(last.kind, Kind::Accent);
    }
}
