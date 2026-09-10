//! Shared application state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

use crate::audio::Engine;
use crate::settings::SettingsStore;
use crate::theme;

/// Set just before a deliberate quit so the exit guard lets it through.
static QUITTING: AtomicBool = AtomicBool::new(false);

pub fn begin_quit() {
    QUITTING.store(true, Ordering::SeqCst);
}

pub fn is_quitting() -> bool {
    QUITTING.load(Ordering::SeqCst)
}

/// How the widget should look, sent to the interface as the `config` event.
#[derive(Debug, Clone, Serialize)]
pub struct AppearanceConfig {
    pub theme: String,
    pub resolved_theme: String,
    pub animations: bool,
    pub compact: bool,
    pub expanded: bool,
    /// "float" or "taskbar".
    pub placement: String,
    /// Visibility, fullscreen and power state, decided on the Rust side. Only
    /// the picture stops; the sound never looks at this.
    pub motion_allowed: bool,
    /// `#rrggbb`, or `None` for the built in turquoise.
    pub accent: Option<String>,
}

/// What the metronome is doing, sent to the interface as the `transport`
/// event.
#[derive(Debug, Clone, Serialize)]
pub struct Transport {
    pub running: bool,
    pub bpm: u32,
    pub beats_per_bar: u8,
    pub beat_unit: u8,
    pub subdivision: u8,
    pub accent_first: bool,
    pub sound: String,
    pub volume: u8,
    /// The device chosen in the settings, `None` for the system default.
    pub output_device: Option<String>,
    /// The device actually playing, which differs while falling back.
    pub active_device: Option<String>,
    pub device_problem: Option<String>,
}

pub struct AppState {
    pub settings: Arc<SettingsStore>,
    pub engine: Arc<Engine>,
    pub motion_allowed: AtomicBool,
}

impl AppState {
    pub fn new(settings: Arc<SettingsStore>, engine: Arc<Engine>) -> Arc<Self> {
        Arc::new(Self {
            settings,
            engine,
            motion_allowed: AtomicBool::new(true),
        })
    }

    pub fn set_motion_allowed(&self, value: bool) {
        self.motion_allowed.store(value, Ordering::Relaxed);
    }

    pub fn appearance(&self) -> AppearanceConfig {
        let s = self.settings.get();
        AppearanceConfig {
            resolved_theme: theme::resolve(&s.appearance.theme).to_string(),
            theme: s.appearance.theme,
            animations: s.appearance.animations,
            compact: s.appearance.compact,
            expanded: s.appearance.expanded,
            placement: s.widget.mode.clone(),
            motion_allowed: self.motion_allowed.load(Ordering::Relaxed),
            accent: s.appearance.accent,
        }
    }

    pub fn transport(&self) -> Transport {
        let m = self.settings.get().metronome;
        let status = self.engine.status();
        Transport {
            running: self.engine.is_running(),
            bpm: m.bpm,
            beats_per_bar: m.beats_per_bar,
            beat_unit: m.beat_unit,
            subdivision: m.subdivision,
            accent_first: m.accent_first,
            sound: m.sound,
            volume: m.volume,
            output_device: m.output_device,
            active_device: status.active_device,
            device_problem: status.problem,
        }
    }
}
