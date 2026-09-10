//! The click sounds, rendered once when a stream opens and only copied after.
//!
//! Everything that costs anything (the sine, the noise, the filters, the
//! envelopes, reading and resampling the recordings) happens here, outside the
//! audio callback. The callback only walks an index through a slice.
//!
//! Three of the sounds are synthesised. The meow is two real kittens, both
//! recordings released under CC0; see `sounds/README.md` for where they came
//! from.

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
    HiHat,
    Meow,
}

impl Sound {
    /// Every sound, in the order the menus list them.
    pub const ALL: [Sound; 4] = [Sound::Click, Sound::Wood, Sound::HiHat, Sound::Meow];

    /// The name stored in the settings file.
    pub fn name(self) -> &'static str {
        match self {
            Sound::Click => "click",
            Sound::Wood => "wood",
            Sound::HiHat => "hihat",
            Sound::Meow => "meow",
        }
    }

    /// The name shown in menus.
    pub fn label(self) -> &'static str {
        match self {
            Sound::Click => "Click",
            Sound::Wood => "Wood",
            Sound::HiHat => "Hi-hat",
            Sound::Meow => "Meow",
        }
    }

    /// Anything the file does not know is the plain click.
    pub fn from_name(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|s| s.name() == name)
            .unwrap_or(Sound::Click)
    }

    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0) as u8
    }

    pub fn from_index(i: u8) -> Self {
        Self::ALL.get(i as usize).copied().unwrap_or(Sound::Click)
    }

    /// The next one in the list, wrapping round, for the sound pill.
    pub fn next(self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len() as u8)
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
            // Closed on the beat, a little open on the downbeat, a tick
            // between.
            Sound::HiHat => Self {
                accent: hat(120.0, rate, BEAT_PEAK * ACCENT_RATIO, 0x5EED_0011),
                beat: hat(45.0, rate, BEAT_PEAK, 0x5EED_0012),
                sub: hat(30.0, rate, BEAT_PEAK * SUB_RATIO, 0x5EED_0013),
            },
            // The older kitten's fuller meow on the downbeat, the younger one's
            // high mew on the beat and, quieter, between beats.
            Sound::Meow => Self {
                accent: recording(KITTEN_8_WEEKS, rate, BEAT_PEAK * ACCENT_RATIO),
                beat: recording(KITTEN_3_WEEKS, rate, BEAT_PEAK),
                sub: recording(KITTEN_3_WEEKS, rate, BEAT_PEAK * SUB_RATIO),
            },
        }
    }
}

/// Bends anything above the knee towards full scale instead of clipping it.
///
/// A click never gets near it. A meow does: at sixteenths and a fast tempo
/// several of them overlap, and a hard clip there is a crackle on top of a cat.
/// Below the knee the sample passes untouched, so the other sounds keep
/// exactly the level they were designed at.
pub fn soft_limit(v: f32) -> f32 {
    const KNEE: f32 = 0.8;
    // Just under full scale, so even a pile of overlapping meows never
    // quite gets there.
    const CEILING: f32 = 0.98;
    let a = v.abs();
    if a <= KNEE {
        v
    } else {
        v.signum() * (KNEE + (CEILING - KNEE) * ((a - KNEE) / (CEILING - KNEE)).tanh())
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
    let mut filter = BiQuad::band_pass(centre, 2.5, r);
    let raw: Vec<f64> = (0..n)
        .map(|i| filter.run(noise.next()) * envelope(i, r, decay_ms))
        .collect();
    normalise(raw, peak)
}

/// A hi-hat the way drum machines have always made one: six square waves at
/// inharmonic pitches for the metal, some noise for the air, and a band of the
/// top octaves kept.
fn hat(decay_ms: f64, rate: u32, peak: f32, seed: u32) -> Vec<f32> {
    // The six oscillator pitches of the classic analogue cymbal circuit.
    const METAL: [f64; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
    let r = rate as f64;
    let n = length(r, decay_ms);
    let mut noise = XorShift(seed);
    let mut band = BiQuad::band_pass((10_000.0f64).min(r * 0.4), 0.9, r);
    let mut high = BiQuad::high_pass((7_000.0f64).min(r * 0.3), 0.707, r);
    let raw: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / r;
            let metal: f64 = METAL
                .iter()
                .map(|f| if (t * f).fract() < 0.5 { 1.0 } else { -1.0 })
                .sum::<f64>()
                / METAL.len() as f64;
            let x = 0.55 * metal + 0.45 * noise.next();
            high.run(band.run(x)) * envelope(i, r, decay_ms)
        })
        .collect();
    normalise(raw, peak)
}

/// A three week old kitten. CC0, barkenov on freesound.org, sound 440697.
const KITTEN_3_WEEKS: &[u8] = include_bytes!("../../sounds/kitten-3-weeks.wav");

/// An eight week old kitten. CC0, Luke100000 on freesound.org, sound 476918.
const KITTEN_8_WEEKS: &[u8] = include_bytes!("../../sounds/kitten-8-weeks.wav");

/// A recording at the device's rate and at an exact peak.
///
/// The files are trimmed to start on the kitten's first breath, so the beat
/// lands where the meow is heard to begin and not a silence before it.
fn recording(bytes: &[u8], rate: u32, peak: f32) -> Vec<f32> {
    match read_wav(bytes) {
        Some((from, samples)) => normalise(resample(&samples, from, rate), peak),
        None => vec![0.0],
    }
}

/// Reads a 16 bit PCM WAV file into mono samples, mixing channels down.
/// Enough for the files this crate ships and nothing more: anything else
/// comes back as `None`.
fn read_wav(bytes: &[u8]) -> Option<(u32, Vec<f64>)> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let u16_at = |i: usize| Some(u16::from_le_bytes(bytes.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |i: usize| Some(u32::from_le_bytes(bytes.get(i..i + 4)?.try_into().ok()?));

    let mut format = None;
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_at(pos + 4)? as usize;
        let body = pos + 8;
        if id == b"fmt " {
            let tag = u16_at(body)?;
            let channels = u16_at(body + 2)?;
            let rate = u32_at(body + 4)?;
            let bits = u16_at(body + 14)?;
            if tag != 1 || bits != 16 || channels == 0 {
                return None;
            }
            format = Some((channels as usize, rate));
        } else if id == b"data" {
            let (channels, rate) = format?;
            let data = bytes.get(body..body + size)?;
            let samples = data
                .chunks_exact(2 * channels)
                .map(|frame| {
                    frame
                        .chunks_exact(2)
                        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f64 / 32768.0)
                        .sum::<f64>()
                        / channels as f64
                })
                .collect();
            return Some((rate, samples));
        }
        // Chunks are padded to an even length.
        pos = body + size + (size & 1);
    }
    None
}

/// Changes the sample rate with Catmull-Rom interpolation. A short meow does
/// not need a polyphase filter, and cubic is clean enough that the few kHz of
/// a kitten's voice come through untouched between 44.1, 48 and 96 kHz.
fn resample(samples: &[f64], from: u32, to: u32) -> Vec<f64> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    // In integers, so a whole second stays exactly a whole second.
    let n = (samples.len() as u64 * to as u64 / from as u64) as usize;
    let at = |i: isize| -> f64 {
        let i = i.clamp(0, samples.len() as isize - 1) as usize;
        samples[i]
    };
    (0..n)
        .map(|k| {
            let x = k as f64 * ratio;
            let i = x.floor() as isize;
            let t = x - i as f64;
            let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            p1 + 0.5
                * t
                * (p2 - p0
                    + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
        })
        .collect()
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

/// An RBJ cookbook biquad, band-pass with constant peak gain or high-pass.
struct BiQuad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl BiQuad {
    fn empty() -> Self {
        Self {
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn band_pass(centre: f64, q: f64, rate: f64) -> Self {
        let mut f = Self::empty();
        f.tune_band_pass(centre, q, rate);
        f
    }

    fn high_pass(cutoff: f64, q: f64, rate: f64) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * cutoff / rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + cos) / 2.0 / a0,
            b1: -(1.0 + cos) / a0,
            b2: (1.0 + cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            ..Self::empty()
        }
    }

    /// New coefficients, same state, so a moving formant does not click.
    fn tune_band_pass(&mut self, centre: f64, q: f64, rate: f64) {
        let w0 = 2.0 * std::f64::consts::PI * centre / rate;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.b1 = 0.0;
        self.b2 = -alpha / a0;
        self.a1 = -2.0 * w0.cos() / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
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
        for sound in Sound::ALL {
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
        for sound in Sound::ALL {
            let bank = Bank::render(sound, 48_000);
            // The clicks are short enough that sixteenths at 300 bpm (50 ms
            // apart) never stack up more than a couple deep. A hi-hat is
            // allowed to ring a little, and a cat takes the time a cat takes.
            let longest_ms = match sound {
                Sound::Click | Sound::Wood => 100,
                Sound::HiHat => 250,
                Sound::Meow => 600,
            };
            for buf in [&bank.accent, &bank.beat, &bank.sub] {
                assert!(buf[0].abs() < 0.01, "{sound:?} starts with a pop");
                let tail = buf[buf.len() - 1].abs() / peak(buf);
                assert!(tail < 0.01, "{sound:?} is cut off at {tail}");
                assert!(
                    buf.len() < 48 * longest_ms,
                    "{sound:?} rings for {} frames",
                    buf.len()
                );
            }
        }
    }

    #[test]
    fn the_noisy_sounds_are_the_same_on_every_launch() {
        for sound in [Sound::Wood, Sound::HiHat, Sound::Meow] {
            let a = Bank::render(sound, 48_000);
            let b = Bank::render(sound, 48_000);
            assert_eq!(a.beat, b.beat, "{sound:?}");
        }
    }

    #[test]
    fn sounds_are_found_by_name_and_index_and_step_round() {
        assert_eq!(Sound::from_name("wood"), Sound::Wood);
        assert_eq!(Sound::from_name("hihat"), Sound::HiHat);
        assert_eq!(Sound::from_name("meow"), Sound::Meow);
        assert_eq!(Sound::from_name("anything"), Sound::Click);
        for s in Sound::ALL {
            assert_eq!(Sound::from_index(s.index()), s);
            assert_eq!(Sound::from_name(s.name()), s);
        }
        let mut s = Sound::Click;
        for _ in 0..Sound::ALL.len() {
            s = s.next();
        }
        assert_eq!(s, Sound::Click);
    }

    /// Zero crossings a second, halved: a crude brightness meter.
    fn crossings_per_second(buf: &[f32], rate: f64) -> f64 {
        let n = buf
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();
        n as f64 / (buf.len() as f64 / rate) / 2.0
    }

    #[test]
    fn a_hi_hat_is_bright() {
        let hat = Bank::render(Sound::HiHat, 48_000);
        let bright = crossings_per_second(&hat.beat, 48_000.0);
        assert!(bright > 5_000.0, "{bright}");
    }

    /// Both recordings are really in the binary, and come out the same length
    /// in time at every rate a device might ask for.
    #[test]
    fn the_kittens_play_at_every_rate() {
        let (rate, three) = read_wav(KITTEN_3_WEEKS).expect("the 3 week file parses");
        assert_eq!(rate, 48_000);
        let (_, eight) = read_wav(KITTEN_8_WEEKS).expect("the 8 week file parses");
        assert!(
            three.len() > 48_000 / 4,
            "the mew is a quarter second or more"
        );
        assert!(
            eight.len() > three.len(),
            "the downbeat meow is the longer one"
        );

        for device in [44_100u32, 48_000, 96_000] {
            let bank = Bank::render(Sound::Meow, device);
            let seconds = bank.beat.len() as f64 / device as f64;
            let expected = three.len() as f64 / 48_000.0;
            assert!((seconds - expected).abs() < 0.001, "{device}: {seconds} s");
            assert!(peak(&bank.beat) > 0.49);
        }
    }

    #[test]
    fn resampling_keeps_a_tone_a_tone() {
        // 1 kHz at 48 kHz, taken to 44.1 kHz, still crosses zero 2000 times a
        // second.
        let tone: Vec<f64> = (0..48_000)
            .map(|i| (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48_000.0).sin())
            .collect();
        let out: Vec<f32> = resample(&tone, 48_000, 44_100)
            .iter()
            .map(|v| *v as f32)
            .collect();
        assert_eq!(out.len(), 44_100);
        let hz = crossings_per_second(&out, 44_100.0);
        assert!((hz - 1000.0).abs() < 2.0, "{hz}");
    }

    #[test]
    fn a_file_that_is_not_a_plain_wav_is_refused() {
        assert!(read_wav(b"").is_none());
        assert!(read_wav(b"RIFF\0\0\0\0WAVE").is_none());
        assert!(read_wav(b"OggS and so on").is_none());
    }

    #[test]
    fn the_limiter_leaves_ordinary_levels_alone_and_never_reaches_full_scale() {
        for v in [0.0f32, 0.3, -0.5, 0.8, -0.8] {
            assert_eq!(soft_limit(v), v);
        }
        for v in [0.9f32, 1.5, 4.0, -3.0] {
            let out = soft_limit(v);
            assert!(out.abs() < 1.0 && out.abs() > 0.8, "{v} -> {out}");
            assert_eq!(out.signum(), v.signum());
        }
        // Louder in, louder out: it bends, it does not fold.
        assert!(soft_limit(1.2) > soft_limit(1.0));
    }

    /// Writes every sound as a WAV file, to listen to them without starting
    /// the app:
    ///
    /// cargo test --manifest-path src-tauri/Cargo.toml --lib write_sound_samples -- --ignored
    ///
    /// The files land in src-tauri/target/sounds.
    #[test]
    #[ignore]
    fn write_sound_samples() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/sounds");
        std::fs::create_dir_all(&dir).unwrap();
        let rate = 48_000u32;
        for sound in Sound::ALL {
            let bank = Bank::render(sound, rate);
            for (kind, buf) in [
                ("accent", &bank.accent),
                ("beat", &bank.beat),
                ("sub", &bank.sub),
            ] {
                let mut bytes = Vec::new();
                let data = (buf.len() * 2) as u32;
                bytes.extend_from_slice(b"RIFF");
                bytes.extend_from_slice(&(36 + data).to_le_bytes());
                bytes.extend_from_slice(b"WAVEfmt ");
                bytes.extend_from_slice(&16u32.to_le_bytes());
                bytes.extend_from_slice(&1u16.to_le_bytes());
                bytes.extend_from_slice(&1u16.to_le_bytes());
                bytes.extend_from_slice(&rate.to_le_bytes());
                bytes.extend_from_slice(&(rate * 2).to_le_bytes());
                bytes.extend_from_slice(&2u16.to_le_bytes());
                bytes.extend_from_slice(&16u16.to_le_bytes());
                bytes.extend_from_slice(b"data");
                bytes.extend_from_slice(&data.to_le_bytes());
                for s in buf.iter() {
                    bytes.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
                }
                std::fs::write(dir.join(format!("{}-{kind}.wav", sound.name())), bytes).unwrap();
            }
        }
    }
}
