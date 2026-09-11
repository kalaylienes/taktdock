//! Persistent settings, stored at `%APPDATA%\TaktDock\settings.json` on
//! Windows and `~/.config/TaktDock/settings.json` on Linux.

use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

pub const MIN_BPM: u32 = 30;
pub const MAX_BPM: u32 = 300;

/// The meters the widget offers, as (beats per bar, beat unit).
pub const METERS: [(u8, u8); 7] = [(1, 4), (2, 4), (3, 4), (4, 4), (5, 4), (6, 8), (7, 8)];

/// Clicks per beat: none, eighths, triplets, sixteenths.
pub const SUBDIVISIONS: [u8; 4] = [1, 2, 3, 4];

/// Tempo presets in the tray menu.
pub const TEMPO_PRESETS: [u32; 7] = [60, 80, 100, 120, 140, 160, 180];

pub const VOLUME_STEPS: [u8; 4] = [25, 50, 75, 100];

/// The sounds by the name the file stores. The audio side has the same list in
/// `audio::voice::Sound`, and a test keeps the two in step.
pub const SOUNDS: [&str; 3] = ["click", "wood", "hihat"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetronomeSettings {
    #[serde(default = "default_bpm")]
    pub bpm: u32,
    #[serde(default = "default_beats")]
    pub beats_per_bar: u8,
    #[serde(default = "default_unit")]
    pub beat_unit: u8,
    #[serde(default = "one")]
    pub subdivision: u8,
    #[serde(default = "yes")]
    pub accent_first: bool,
    /// "click", "wood" or "hihat". Anything else, including the meow and the
    /// bark an earlier build had, reads back as the click.
    #[serde(default = "default_sound")]
    pub sound: String,
    /// 0 to 100. Applied on a logarithmic curve, see `audio::voice::gain`.
    #[serde(default = "default_volume")]
    pub volume: u8,
    /// Output device by name. `None` follows the system default.
    #[serde(default)]
    pub output_device: Option<String>,
}

fn default_bpm() -> u32 {
    120
}
fn default_beats() -> u8 {
    4
}
fn default_unit() -> u8 {
    4
}
fn one() -> u8 {
    1
}
fn default_sound() -> String {
    "click".into()
}
fn default_volume() -> u8 {
    75
}

impl Default for MetronomeSettings {
    fn default() -> Self {
        Self {
            bpm: default_bpm(),
            beats_per_bar: default_beats(),
            beat_unit: default_unit(),
            subdivision: 1,
            accent_first: true,
            sound: default_sound(),
            volume: default_volume(),
            output_device: None,
        }
    }
}

impl MetronomeSettings {
    /// A hand edited file can hold anything. Everything the audio thread reads
    /// is brought back inside the range it was written for.
    pub fn sanitised(mut self) -> Self {
        self.bpm = self.bpm.clamp(MIN_BPM, MAX_BPM);
        if !METERS.contains(&(self.beats_per_bar, self.beat_unit)) {
            self.beats_per_bar = default_beats();
            self.beat_unit = default_unit();
        }
        if !SUBDIVISIONS.contains(&self.subdivision) {
            self.subdivision = 1;
        }
        if !SOUNDS.contains(&self.sound.as_str()) {
            self.sound = default_sound();
        }
        self.volume = self.volume.min(100);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetSettings {
    /// "float" for a free floating window, "taskbar" to pin into the strip.
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Persistent monitor identity. Raw coordinates are never stored, so a
    /// display coming back on a different arrangement cannot strand the window.
    #[serde(default)]
    pub monitor_stable_id: Option<String>,
    #[serde(default)]
    pub monitor_friendly_name: Option<String>,
    #[serde(default = "default_corner")]
    pub corner: String,
    #[serde(default)]
    pub edge_offset_x: i32,
    #[serde(default)]
    pub edge_offset_y: i32,
    /// Logical gap between the widget and the notification area.
    #[serde(default = "default_tray_gap")]
    pub tray_gap: i32,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default = "yes")]
    pub hide_on_fullscreen: bool,
    #[serde(default)]
    pub click_through: bool,
}

fn default_mode() -> String {
    "float".into()
}

fn default_corner() -> String {
    "bottom-right".into()
}

fn default_tray_gap() -> i32 {
    8
}

impl Default for WidgetSettings {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            monitor_stable_id: None,
            monitor_friendly_name: None,
            corner: default_corner(),
            edge_offset_x: 0,
            edge_offset_y: 0,
            tray_gap: default_tray_gap(),
            visible: true,
            hide_on_fullscreen: true,
            click_through: false,
        }
    }
}

impl WidgetSettings {
    pub fn taskbar_mode(&self) -> bool {
        self.mode == "taskbar"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceSettings {
    /// "system", "dark" or "light".
    #[serde(default = "default_theme")]
    pub theme: String,
    /// The taller floating layout with the bar progress and the sound row.
    #[serde(default)]
    pub expanded: bool,
    /// The narrower pinned layout without the tempo slider.
    #[serde(default)]
    pub compact: bool,
    #[serde(default = "yes")]
    pub animations: bool,
    /// The accent colour as `#rrggbb`, anything at all. `None` is the built in
    /// turquoise, whose two theme variants were tuned by hand.
    #[serde(default)]
    pub accent: Option<String>,
}

fn default_theme() -> String {
    "system".into()
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            expanded: false,
            compact: false,
            animations: true,
            accent: None,
        }
    }
}

/// The accent the built in palette is drawn around, for the picker to start
/// from and for the tray icon when nothing else was chosen.
pub const DEFAULT_ACCENT: &str = "#3fbfae";

/// `#rgb`, `#rrggbb`, with or without the hash, any case, surrounding space
/// ignored. Comes back as lowercase `#rrggbb`, or `None` for anything else.
pub fn normalise_accent(text: &str) -> Option<String> {
    let hex = text.trim().trim_start_matches('#');
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let full = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => hex.to_string(),
        _ => return None,
    };
    Some(format!("#{}", full.to_ascii_lowercase()))
}

/// The accent as red, green and blue, for drawing the tray icon.
pub fn accent_rgb(accent: Option<&str>) -> [u8; 3] {
    let hex = accent
        .and_then(normalise_accent)
        .unwrap_or_else(|| DEFAULT_ACCENT.to_string());
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
    [byte(1), byte(3), byte(5)]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSettings {
    #[serde(default = "yes")]
    pub check: bool,
    #[serde(default)]
    pub last_check: Option<DateTime<Utc>>,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check: true,
            last_check: None,
        }
    }
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub schema_version: u32,
    #[serde(default)]
    pub metronome: MetronomeSettings,
    #[serde(default)]
    pub widget: WidgetSettings,
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub updates: UpdateSettings,
    #[serde(default)]
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            metronome: Default::default(),
            widget: Default::default(),
            appearance: Default::default(),
            updates: Default::default(),
            autostart: false,
        }
    }
}

/// Brings a file written by any earlier schema up to the current one.
///
/// There is only one schema so far, so this only stamps the version. It exists
/// from the first release because the day a field changes meaning is the day
/// every settings file already out there has to be read correctly, and a
/// migration added after the fact cannot tell an old file from a broken one.
fn migrate(mut s: Settings) -> Settings {
    if s.schema_version < SCHEMA_VERSION {
        s.schema_version = SCHEMA_VERSION;
    }
    s.metronome = s.metronome.sanitised();
    s.appearance.accent = s.appearance.accent.as_deref().and_then(normalise_accent);
    s
}

/// Where settings, logs and reports live.
///
/// `TAKTDOCK_DATA_DIR` moves it, so a second build can run beside an installed
/// one without the two reading each other's settings: the updater test does
/// exactly that. It is inherited by the installer the updater starts and by the
/// copy the installer starts afterwards, so the whole chain stays in one place.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("TAKTDOCK_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("TaktDock")
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

/// Presence of this file tells the watchdog task that the app was closed on
/// purpose and should stay closed.
pub fn stay_closed_marker() -> PathBuf {
    data_dir().join("stay-closed")
}

/// Called when the user quits from the tray menu.
pub fn mark_stay_closed() {
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(stay_closed_marker(), "quit from the tray menu\n");
}

/// Called on every start, so a watchdog restart or a manual launch clears it.
pub fn clear_stay_closed() {
    let _ = std::fs::remove_file(stay_closed_marker());
}

/// Written while a setting that touches the window is being applied, and
/// removed once it has survived a few turns of the event loop.
///
/// This skeleton learned it the hard way: a setting saved before it was applied
/// aborted the process, and because it was already on disk every launch after
/// that aborted the same way. If this file is still here at startup, the last
/// launch died applying what it names, and that one setting is put back.
fn applying_marker() -> PathBuf {
    data_dir().join("applying")
}

pub fn begin_apply(what: &str) {
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(applying_marker(), what);
}

pub fn end_apply() {
    let _ = std::fs::remove_file(applying_marker());
}

/// The setting the previous launch died applying, if it did.
pub fn take_unfinished_apply() -> Option<String> {
    let what = std::fs::read_to_string(applying_marker()).ok()?;
    end_apply();
    Some(what.trim().to_string())
}

/// Puts back the one setting a crash was traced to. Returns what the user is
/// told, or `None` when the marker named something this build does not know.
pub fn revert_unfinished(s: &mut Settings, what: &str) -> Option<&'static str> {
    match what {
        "click_through" => {
            s.widget.click_through = false;
            Some("Click-through was turned off because TaktDock closed unexpectedly while applying it.")
        }
        "placement" => {
            s.widget.mode = default_mode();
            Some("The widget is floating again because TaktDock closed unexpectedly while pinning it to the taskbar.")
        }
        _ => None,
    }
}

/// Single instance for the lifetime of the process. Every mutation is written
/// through immediately.
pub struct SettingsStore {
    inner: RwLock<Settings>,
    path: PathBuf,
}

impl SettingsStore {
    pub fn load() -> Self {
        let path = settings_path();
        let settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| parse(&s))
            .unwrap_or_default();

        let store = Self {
            inner: RwLock::new(migrate(settings)),
            path,
        };
        let _ = store.persist();
        store
    }

    pub fn get(&self) -> Settings {
        self.inner.read().clone()
    }

    pub fn update<F: FnOnce(&mut Settings)>(&self, f: F) -> Settings {
        let snapshot = {
            let mut guard = self.inner.write();
            f(&mut guard);
            guard.metronome = guard.metronome.clone().sanitised();
            guard.appearance.accent = guard
                .appearance
                .accent
                .as_deref()
                .and_then(normalise_accent);
            guard.clone()
        };
        if let Err(e) = self.persist() {
            tracing::error!("settings could not be written: {e}");
        }
        snapshot
    }

    fn persist(&self) -> Result<()> {
        let dir = self
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(&*self.inner.read())?;
        write_atomic(&self.path, json.as_bytes())
    }
}

fn parse(text: &str) -> Option<Settings> {
    match serde_json::from_str::<Settings>(strip_bom(text)) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("settings.json could not be read, using defaults: {e}");
            None
        }
    }
}

/// The settings file is meant to be hand edited, and both Notepad and
/// PowerShell write UTF-8 with a byte order mark that serde refuses to parse.
/// Dropping it quietly avoids resetting every preference.
pub fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// Temp file plus rename, so a crash never leaves a half written file behind.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_byte_order_mark_does_not_reset_settings() {
        let json = "\u{feff}{\"schema_version\":1,\"widget\":{\"visible\":false}}";
        let parsed = parse(json).expect("parses");
        assert!(!parsed.widget.visible);
    }

    #[test]
    fn missing_sections_fall_back_to_defaults() {
        let parsed = parse(r#"{"schema_version":1}"#).expect("parses");
        assert!(parsed.widget.visible);
        assert!(!parsed.widget.taskbar_mode());
        assert_eq!(parsed.metronome.bpm, 120);
        assert_eq!(parsed.metronome.beats_per_bar, 4);
        assert!(parsed.metronome.accent_first);
        assert!(parsed.updates.check);
        assert!(parsed.appearance.animations);
    }

    /// A file written before a field existed has to come back with that
    /// field's default rather than failing to parse and losing everything.
    #[test]
    fn an_older_file_gains_the_fields_it_never_had() {
        let json = r#"{"schema_version":1,"metronome":{"bpm":96},"widget":{"mode":"taskbar"}}"#;
        let parsed = migrate(parse(json).expect("parses"));
        assert_eq!(parsed.metronome.bpm, 96);
        assert_eq!(parsed.metronome.sound, "click");
        assert_eq!(parsed.metronome.volume, 75);
        assert_eq!(parsed.metronome.output_device, None);
        assert!(parsed.widget.taskbar_mode());
        assert_eq!(parsed.widget.tray_gap, 8);
        assert!(parsed.widget.hide_on_fullscreen);
    }

    #[test]
    fn a_hand_edited_tempo_is_brought_back_into_range() {
        let json = r#"{"schema_version":1,"metronome":{"bpm":900,"beats_per_bar":9,"beat_unit":4,"subdivision":7,"sound":"cowbell","volume":250}}"#;
        let m = migrate(parse(json).expect("parses")).metronome;
        assert_eq!(m.bpm, MAX_BPM);
        assert_eq!((m.beats_per_bar, m.beat_unit), (4, 4));
        assert_eq!(m.subdivision, 1);
        assert_eq!(m.sound, "click");
        assert_eq!(m.volume, 100);

        let slow = MetronomeSettings {
            bpm: 3,
            ..Default::default()
        }
        .sanitised();
        assert_eq!(slow.bpm, MIN_BPM);
    }

    #[test]
    fn any_colour_is_accepted_in_any_way_it_is_usually_written() {
        assert_eq!(normalise_accent("#FF8800").as_deref(), Some("#ff8800"));
        assert_eq!(normalise_accent("ff8800").as_deref(), Some("#ff8800"));
        assert_eq!(normalise_accent(" #f80 ").as_deref(), Some("#ff8800"));
        assert_eq!(normalise_accent("#000").as_deref(), Some("#000000"));
        assert_eq!(normalise_accent("#ffffff").as_deref(), Some("#ffffff"));
        for bad in [
            "",
            "#",
            "#12",
            "#12345",
            "#1234567",
            "#ggg",
            "red",
            "rgb(1,2,3)",
        ] {
            assert_eq!(normalise_accent(bad), None, "{bad}");
        }
        assert_eq!(accent_rgb(Some("#ff8800")), [255, 136, 0]);
        assert_eq!(accent_rgb(None), [0x3f, 0xbf, 0xae]);
        assert_eq!(accent_rgb(Some("nonsense")), [0x3f, 0xbf, 0xae]);
    }

    #[test]
    fn a_hand_edited_accent_is_tidied_or_dropped() {
        let json = r##"{"schema_version":1,"appearance":{"accent":"#ABC"}}"##;
        assert_eq!(
            migrate(parse(json).unwrap()).appearance.accent.as_deref(),
            Some("#aabbcc")
        );
        let json = r##"{"schema_version":1,"appearance":{"accent":"teal"}}"##;
        assert_eq!(migrate(parse(json).unwrap()).appearance.accent, None);
    }

    #[test]
    fn the_file_and_the_audio_side_know_the_same_sounds() {
        use crate::audio::voice::Sound;
        let names: Vec<&str> = Sound::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(names, SOUNDS);
        for name in SOUNDS {
            let m = MetronomeSettings {
                sound: name.into(),
                ..Default::default()
            }
            .sanitised();
            assert_eq!(m.sound, name);
        }
    }

    #[test]
    fn writes_are_atomic_and_leave_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("taktdock-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        write_atomic(&path, b"{\"schema_version\":1}").unwrap();
        write_atomic(&path, b"{\"schema_version\":1,\"autostart\":true}").unwrap();
        let back = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(back.autostart);
        assert!(!path.with_extension("tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_setting_that_crashed_is_put_back() {
        let mut s = Settings::default();
        s.widget.click_through = true;
        s.widget.mode = "taskbar".into();

        assert!(revert_unfinished(&mut s, "click_through").is_some());
        assert!(!s.widget.click_through);
        assert!(
            s.widget.taskbar_mode(),
            "only the named setting is reverted"
        );

        assert!(revert_unfinished(&mut s, "placement").is_some());
        assert!(!s.widget.taskbar_mode());

        assert!(revert_unfinished(&mut s, "something else").is_none());
    }
}
