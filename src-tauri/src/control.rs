//! Every way of changing what the metronome does goes through here: the
//! widget, the tray menu and the context menus all call the same functions,
//! so the settings file, the audio thread, the tray and the interface can never
//! disagree about the tempo.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::settings::{self, MetronomeSettings, METERS, SUBDIVISIONS};
use crate::state::AppState;

fn state(app: &AppHandle) -> Option<Arc<AppState>> {
    app.try_state::<Arc<AppState>>().map(|s| s.inner().clone())
}

/// Applies a change to the metronome settings and tells everyone who shows
/// them: the store writes through as part of the update, the audio thread picks
/// the new values up from atomics, and only then do the interface and the tray
/// hear about it.
pub fn update(app: &AppHandle, f: impl FnOnce(&mut MetronomeSettings)) {
    let Some(state) = state(app) else { return };
    let snapshot = state.settings.update(|s| f(&mut s.metronome));
    state.engine.apply(&snapshot.metronome);
    announce(app, &state);
}

fn announce(app: &AppHandle, state: &AppState) {
    let _ = app.emit("transport", state.transport());
    crate::tray::refresh_badge(app);
    crate::tray::rebuild(app);
}

/// Something about the output changed: a device appeared, went away, or the
/// engine fell back to another one.
pub fn output_changed(app: &AppHandle) {
    if let Some(state) = state(app) {
        announce(app, &state);
    }
}

pub fn start(app: &AppHandle) {
    let Some(state) = state(app) else { return };
    state.engine.start();
    announce(app, &state);
}

pub fn stop(app: &AppHandle) {
    let Some(state) = state(app) else { return };
    state.engine.stop();
    announce(app, &state);
}

/// Returns whether it is playing afterwards.
pub fn toggle(app: &AppHandle) -> bool {
    let Some(state) = state(app) else {
        return false;
    };
    let running = state.engine.toggle();
    announce(app, &state);
    running
}

pub fn set_tempo(app: &AppHandle, bpm: u32) {
    let bpm = bpm.clamp(settings::MIN_BPM, settings::MAX_BPM);
    let Some(current) = state(app).map(|s| s.settings.get().metronome.bpm) else {
        return;
    };
    if current != bpm {
        update(app, |m| m.bpm = bpm);
    }
}

pub fn nudge_tempo(app: &AppHandle, delta: i32) {
    let Some(current) = state(app).map(|s| s.settings.get().metronome.bpm) else {
        return;
    };
    set_tempo(app, (current as i32 + delta).max(0) as u32);
}

pub fn set_meter(app: &AppHandle, beats: u8, unit: u8) {
    if METERS.contains(&(beats, unit)) {
        update(app, |m| {
            m.beats_per_bar = beats;
            m.beat_unit = unit;
        });
    }
}

/// The next meter in the list, wrapping round.
pub fn next_meter(current: (u8, u8)) -> (u8, u8) {
    let i = METERS.iter().position(|m| *m == current).unwrap_or(3);
    METERS[(i + 1) % METERS.len()]
}

pub fn cycle_meter(app: &AppHandle) {
    let Some(m) = state(app).map(|s| s.settings.get().metronome) else {
        return;
    };
    let (beats, unit) = next_meter((m.beats_per_bar, m.beat_unit));
    set_meter(app, beats, unit);
}

pub fn set_subdivision(app: &AppHandle, n: u8) {
    if SUBDIVISIONS.contains(&n) {
        update(app, |m| m.subdivision = n);
    }
}

pub fn next_subdivision(current: u8) -> u8 {
    let i = SUBDIVISIONS.iter().position(|s| *s == current).unwrap_or(0);
    SUBDIVISIONS[(i + 1) % SUBDIVISIONS.len()]
}

pub fn cycle_subdivision(app: &AppHandle) {
    let Some(current) = state(app).map(|s| s.settings.get().metronome.subdivision) else {
        return;
    };
    set_subdivision(app, next_subdivision(current));
}

pub fn nudge_volume(app: &AppHandle, delta: i32) {
    let Some(current) = state(app).map(|s| s.settings.get().metronome.volume) else {
        return;
    };
    let next = (current as i32 + delta).clamp(0, 100) as u8;
    if next != current {
        update(app, |m| m.volume = next);
    }
}

pub fn cycle_sound(app: &AppHandle) {
    update(app, |m| {
        m.sound = if m.sound == "click" { "wood" } else { "click" }.into();
    });
}

/// How a subdivision is named in menus and tooltips.
pub fn subdivision_name(n: u8) -> &'static str {
    match n {
        2 => "Eighths",
        3 => "Triplets",
        4 => "Sixteenths",
        _ => "None",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meter_pill_walks_every_meter_and_comes_back() {
        let mut m = (4, 4);
        let mut seen = vec![m];
        for _ in 0..METERS.len() {
            m = next_meter(m);
            seen.push(m);
        }
        assert_eq!(seen.first(), seen.last());
        for meter in METERS {
            assert!(seen.contains(&meter), "{meter:?} is never reached");
        }
        // Something the list does not know starts the walk from 4/4.
        assert_eq!(next_meter((9, 16)), (5, 4));
    }

    #[test]
    fn the_subdivision_pill_walks_every_grid() {
        assert_eq!(next_subdivision(1), 2);
        assert_eq!(next_subdivision(2), 3);
        assert_eq!(next_subdivision(3), 4);
        assert_eq!(next_subdivision(4), 1);
        assert_eq!(next_subdivision(9), 2);
    }
}
