//! The click sounds, rendered once when a stream opens and only copied after.
//!
//! Everything that costs anything (the sine, the noise, the filter, the
//! envelope) happens here, outside the audio callback. The callback only walks
//! an index through a slice.

use super::clock::Kind;

/// Peak of an ordinary beat before the volume is applied. Leaves room for an
/// accent three decibels louder and for two overlapping tails without clipping.
const BEAT_PEAK: f32 = 0.5;

/// +3 dB.
const ACCENT_RATIO: f32 = 1.412_537_5;

/// Clicks between beats sit well under the beat, so the beat stays the beat.
const SUB_RATIO: f32 = 0.6;

/// Rise time of every click. Instant onsets pop; one millisecond reads as a
/// click without the pop.
const ATTACK_MS: f64 = 1.0;

/// Tails are cut once they fall this far below the peak (about -43 dB).
const TAIL_FLOOR: f64 = 0.007;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Click,
    Wood,
}

impl Sound {
    pub fn from_name(name: &str) -> Self {
        match name {
            "wood" => Sound::Wood,
            _ => Sound::Click,
        }
    }

    pub fn index(self) -> u8 {
        match self {
            Sound::Click => 0,
            Sound::Wood => 1,
        }
    }

    pub fn from_index(i: u8) -> Self {
        if i == 1 {
            Sound::Wood
        } else {
            Sound::Click
        }
    }
}

/// The three buffers one sound needs.
pub struct Bank {
    pub accent: Vec<f32>,
    pub beat: Vec<f32>,
    pub sub: Vec<f32>,
}

impl Bank {
    pub fn get(&self, kind: Kind) -> &[f32] {
        match kind {
            Kind::Accent => &self.accent,
            Kind::Beat => &self.beat,
            Kind::Sub => &self.sub,
        }
    }

    pub fn render(sound: Sound, rate: u32) -> Self {
        match sound {
            // A short sine, higher and longer on the downbeat.
            Sound::Click => Self {
                accent: sine(1500.0, 30.0, rate, BEAT_PEAK * ACCENT_RATIO),
                beat: sine(1000.0, 20.0, rate, BEAT_PEAK),
                sub: sine(1000.0, 20.0, rate, BEAT_PEAK * SUB_RATIO),
            },
            // Band limited noise, which reads as a woodblock or a stick.
            Sound::Wood => Self {
                accent: wood(3000.0, 12.0, rate, BEAT_PEAK * ACCENT_RATIO, 0x5EED_0001),
                beat: wood(2000.0, 12.0, rate, BEAT_PEAK, 0x5EED_0002),
                sub: wood(2000.0, 12.0, rate, BEAT_PEAK * SUB_RATIO, 0x5EED_0003),
            },
        }
    }
}

/// Linear gain for a 0 to 100 volume.
///
/// Loudness is heard logarithmically, so a linear slider puts all of the useful
/// range in its bottom fifth. Forty decibels over the top of the range is what
/// a practice click needs: 100 is full scale, 50 is -25 dB, and 0 is silence
/// rather than a very quiet click.
pub fn gain(volume: u8) -> f32 {
    if volume == 0 {
        return 0.0;
    }
    let v = volume.min(100) as f32;
    10f32.powf((v - 100.0) / 40.0)
}

/// Attack then exponential decay. `decay_ms` is the time the tail takes to
/// fall to a twentieth of the peak (-26 dB), which is where a click stops
/// sounding like part of the beat.
fn envelope(i: usize, rate: f64, decay_ms: f64) -> f64 {
    let t_ms = i as f64 * 1000.0 / rate;
    let tau = decay_ms / 3.0;
    let attack = (t_ms / ATTACK_MS).min(1.0);
    let decay = if t_ms <= ATTACK_MS {
        1.0
    } else {
        (-(t_ms - ATTACK_MS) / tau).exp()
    };
    attack * decay
}

fn length(rate: f64, decay_ms: f64) -> usize {
    let tau = decay_ms / 3.0;
    let ms = ATTACK_MS + tau * (1.0 / TAIL_FLOOR).ln();
    (ms * rate / 1000.0).ceil() as usize
}

fn sine(freq: f64, decay_ms: f64, rate: u32, peak: f32) -> Vec<f32> {
    let r = rate as f64;
    let n = length(r, decay_ms);
    let raw: Vec<f64> = (0..n)
        .map(|i| {
            let phase = 2.0 * std::f64::consts::PI * freq * i as f64 / r;
            phase.sin() * envelope(i, r, decay_ms)
        })
        .collect();
    normalise(raw, peak)
}

fn wood(centre: f64, decay_ms: f64, rate: u32, peak: f32, seed: u32) -> Vec<f32> {
    let r = rate as f64;
    let n = length(r, decay_ms);
    let mut noise = XorShift(seed);
    let mut filter = BandPass::new(centre, 2.5, r);
    let raw: Vec<f64> = (0..n)
        .map(|i| filter.run(noise.next()) * envelope(i, r, decay_ms))
        .collect();
    normalise(raw, peak)
}

/// Scales to an exact peak, so every sound sits at the level the constants
/// above promise whatever the filter did to it, and ramps the last couple of
/// milliseconds to zero. Noise does not decay as neatly as its envelope, and a
/// buffer that stops on a non-zero sample ends in a tick of its own.
fn normalise(raw: Vec<f64>, peak: f32) -> Vec<f32> {
    let max = raw.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let scale = if max > 0.0 { peak as f64 / max } else { 0.0 };
    let n = raw.len();
    let fade = (n / 20).max(1);
    raw.into_iter()
        .enumerate()
        .map(|(i, v)| {
            let left = n - i;
            let ramp = if left <= fade {
                (left - 1) as f64 / fade as f64
            } else {
                1.0
            };
            (v * scale * ramp) as f32
        })
        .collect()
}

/// A tiny fixed-seed generator. The noise has to be the same on every run, or
/// the wood click would change character between launches.
struct XorShift(u32);

impl XorShift {
    fn next(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f64 / u32::MAX as f64) * 2.0 - 1.0
    }
}

/// RBJ cookbook band-pass, constant peak gain.
struct BandPass {
    b0: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl BandPass {
    fn new(centre: f64, q: f64, rate: f64) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * centre / rate;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b2: -alpha / a0,
            a1: -2.0 * w0.cos() / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(v: &[f32]) -> f32 {
        v.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn the_volume_curve_is_logarithmic() {
        assert_eq!(gain(0), 0.0);
        assert!((gain(100) - 1.0).abs() < 1e-6);
        // 50 is -25 dB.
        let db = 20.0 * gain(50).log10();
        assert!((db + 25.0).abs() < 0.01, "{db}");
        // Every step is audible and in the same direction.
        for v in 1..100u8 {
            assert!(gain(v + 1) > gain(v));
        }
        assert_eq!(gain(250), gain(100));
    }

    #[test]
    fn the_accent_is_three_decibels_over_the_beat_and_subs_sit_under_it() {
        for sound in [Sound::Click, Sound::Wood] {
            for rate in [44_100, 48_000, 96_000] {
                let bank = Bank::render(sound, rate);
                let db = 20.0 * (peak(&bank.accent) / peak(&bank.beat)).log10();
                assert!((db - 3.0).abs() < 0.05, "{sound:?} at {rate}: {db} dB");
                assert!((peak(&bank.sub) / peak(&bank.beat) - SUB_RATIO).abs() < 1e-4);
                // Two overlapping accents still fit without clipping.
                assert!(peak(&bank.accent) * 2.0 <= 1.5);
            }
        }
    }

    #[test]
    fn clicks_start_from_silence_and_end_in_it() {
        for sound in [Sound::Click, Sound::Wood] {
            let bank = Bank::render(sound, 48_000);
            for buf in [&bank.accent, &bank.beat, &bank.sub] {
                assert!(buf[0].abs() < 0.01, "{sound:?} starts with a pop");
                let tail = buf[buf.len() - 1].abs() / peak(buf);
                assert!(tail < 0.01, "{sound:?} is cut off at {tail}");
                // Short enough that sixteenths at 300 bpm (50 ms apart) never
                // stack up more than a couple deep.
                assert!(
                    buf.len() < 48_000 / 10,
                    "{sound:?} rings for {} frames",
                    buf.len()
                );
            }
        }
    }

    #[test]
    fn the_wood_click_is_the_same_on_every_launch() {
        let a = Bank::render(Sound::Wood, 48_000);
        let b = Bank::render(Sound::Wood, 48_000);
        assert_eq!(a.beat, b.beat);
    }

    #[test]
    fn sounds_are_found_by_name_and_index() {
        assert_eq!(Sound::from_name("wood"), Sound::Wood);
        assert_eq!(Sound::from_name("click"), Sound::Click);
        assert_eq!(Sound::from_name("anything"), Sound::Click);
        for s in [Sound::Click, Sound::Wood] {
            assert_eq!(Sound::from_index(s.index()), s);
        }
    }
}
