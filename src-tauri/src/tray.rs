//! Tray icon and the menus.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use tauri::image::Image;
use tauri::menu::{
    CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, Submenu, SubmenuBuilder,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::control;
use crate::settings::{self, Settings, METERS, SUBDIVISIONS, TEMPO_PRESETS, VOLUME_STEPS};
use crate::state::AppState;
use crate::{diagnostics, monitor, window};

/// What the session calls the thing this switch turns on.
#[cfg(windows)]
const AUTOSTART_LABEL: &str = "Start with Windows";
#[cfg(not(windows))]
const AUTOSTART_LABEL: &str = "Start at login";

pub const TRAY_ID: &str = "main";

/// The menus the app owns.
///
/// The tray's menu and the widget's context menu were once one object shared
/// between them. Showing a menu borrows it and swapping the tray's
/// menu borrows it again, and nothing orders those two against each other: on
/// Linux a popup is not modal and returns while it is still on screen, so a
/// rebuild landing in that gap took a second borrow of the same object and
/// aborted the process. A popup gets a menu of its own that nothing else
/// ever touches.
pub struct MenuHandle {
    /// Attached to the tray icon.
    pub tray: RwLock<Option<Menu<Wry>>>,
    /// A description of what `tray` currently shows.
    pub signature: RwLock<Option<String>>,
    /// Behind the last widget popup, kept alive for as long as it is displayed.
    pub popup: RwLock<Option<Menu<Wry>>>,
    /// The tray menu one generation back. A swap can land while the menu it
    /// replaces is still being dismissed, and dropping the old object then
    /// tears down GTK widgets out from under a menu that is on screen. Holding
    /// it until the swap after next costs one menu's worth of memory.
    pub retired: RwLock<Option<Menu<Wry>>>,
}

const ICON_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const ICON_PLAYING: &[u8] = include_bytes!("../icons/tray-playing.png");

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    app.manage(MenuHandle {
        tray: RwLock::new(None),
        signature: RwLock::new(None),
        popup: RwLock::new(None),
        retired: RwLock::new(None),
    });

    let menu = build_menu(app)?;
    let signature = menu_signature(app);
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON_IDLE)?)
        .tooltip(tooltip(app))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                match button {
                    MouseButton::Left => window::toggle_visibility(tray.app_handle()),
                    // Play and stop without opening anything, for when the
                    // widget is hidden or behind a game on another screen.
                    MouseButton::Middle => {
                        control::toggle(tray.app_handle());
                    }
                    _ => {}
                }
            }
        })
        .build(app)?;

    let state = app.state::<MenuHandle>();
    state.tray.write().replace(menu);
    state.signature.write().replace(signature);
    Ok(())
}

/// Builds a menu for one popup on the widget.
///
/// Deliberately not the tray's menu. See `MenuHandle`.
pub fn build_popup_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    build_menu(app)
}

/// The meter or subdivision choices on their own, for a right click on the
/// pill that shows them.
pub fn build_pill_menu(app: &AppHandle, pill: &str) -> tauri::Result<Menu<Wry>> {
    let s = app.state::<Arc<AppState>>().settings.get();
    let mut menu = MenuBuilder::new(app);
    if pill == "subdivision" {
        for n in SUBDIVISIONS {
            menu = menu.item(
                &CheckMenuItemBuilder::with_id(format!("sub:{n}"), control::subdivision_name(n))
                    .checked(s.metronome.subdivision == n)
                    .build(app)?,
            );
        }
    } else {
        for (beats, unit) in METERS {
            menu = menu.item(
                &CheckMenuItemBuilder::with_id(
                    format!("meter:{beats}/{unit}"),
                    format!("{beats}/{unit}"),
                )
                .checked((s.metronome.beats_per_bar, s.metronome.beat_unit) == (beats, unit))
                .build(app)?,
            );
        }
        menu = menu.separator().item(
            &CheckMenuItemBuilder::with_id("accent", "Accent the first beat")
                .checked(s.metronome.accent_first)
                .build(app)?,
        );
    }
    menu.build()
}

/// True while the widget's context menu is being shown.
///
/// Only meaningful where showing a menu blocks. On Windows the popup runs a
/// modal loop and this covers the whole time the menu is up; on Linux the call
/// returns immediately and this covers almost nothing, which is why the popup
/// owns its menu outright rather than relying on this flag.
static POPUP_OPEN: AtomicBool = AtomicBool::new(false);
static REBUILD_PENDING: AtomicBool = AtomicBool::new(false);

/// The preset the current tempo sits on, if any. The tempo itself changes on
/// every wheel notch and must not drive menu rebuilds; which radio item is
/// ticked only changes when the tempo crosses a preset.
fn preset_match(bpm: u32) -> Option<u32> {
    TEMPO_PRESETS.iter().copied().find(|p| *p == bpm)
}

/// Description of everything the menu renders, so an unchanged menu is never
/// rebuilt. Swapping it has a real cost and a real hazard; doing it only when
/// the contents actually differ removes most of both.
fn menu_signature(app: &AppHandle) -> String {
    let state = app.state::<Arc<AppState>>();
    let s = state.settings.get();
    let m = &s.metronome;
    let status = state.engine.status();
    let mut sig = format!(
        "{}|{:?}|{}/{}|{}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}",
        state.engine.is_running(),
        preset_match(m.bpm),
        m.beats_per_bar,
        m.beat_unit,
        m.subdivision,
        m.accent_first,
        m.sound,
        m.volume,
        m.output_device,
        status.active_device,
        status.problem,
        status.devices,
    );
    sig.push_str(&format!(
        "|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        s.widget.visible,
        s.widget.mode,
        s.widget.click_through,
        s.widget.hide_on_fullscreen,
        s.appearance.theme,
        s.appearance.expanded,
        s.appearance.compact,
        s.appearance.animations,
        s.autostart,
        s.updates.check,
        s.widget.monitor_stable_id.as_deref().unwrap_or("-"),
    ));
    sig.push('|');
    sig.push_str(crate::update::available().as_deref().unwrap_or("-"));
    sig.push_str(if crate::update::installing() {
        "busy"
    } else {
        ""
    });

    // Only the identity of the attached monitors matters here. Their geometry
    // changes constantly and must not drive menu rebuilds.
    #[cfg(windows)]
    for mon in monitor::enumerate() {
        sig.push('|');
        sig.push_str(&mon.stable_id);
        sig.push_str(&mon.friendly_name);
    }
    sig
}

pub fn rebuild(app: &AppHandle) {
    if POPUP_OPEN.load(Ordering::SeqCst) {
        REBUILD_PENDING.store(true, Ordering::SeqCst);
        return;
    }

    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if POPUP_OPEN.load(Ordering::SeqCst) {
            REBUILD_PENDING.store(true, Ordering::SeqCst);
            return;
        }
        let Some(last) = handle.try_state::<MenuHandle>() else {
            return;
        };

        let signature = menu_signature(&handle);
        if last.signature.read().as_deref() == Some(signature.as_str()) {
            return;
        }

        let Ok(menu) = build_menu(&handle) else {
            return;
        };
        if let Some(tray) = handle.tray_by_id(TRAY_ID) {
            let _ = tray.set_menu(Some(menu.clone()));
        }
        let previous = last.tray.write().replace(menu);
        *last.retired.write() = previous;
        last.signature.write().replace(signature);
    });
}

/// Marks a context menu as displayed for as long as the guard lives.
pub struct PopupGuard;

impl Default for PopupGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl PopupGuard {
    pub fn new() -> Self {
        POPUP_OPEN.store(true, Ordering::SeqCst);
        Self
    }
}

impl Drop for PopupGuard {
    fn drop(&mut self) {
        POPUP_OPEN.store(false, Ordering::SeqCst);
    }
}

/// Applies a rebuild that was deferred while a menu was open.
pub fn flush_pending_rebuild(app: &AppHandle) {
    if REBUILD_PENDING.swap(false, Ordering::SeqCst) {
        rebuild(app);
    }
}

fn radio(
    app: &AppHandle,
    id: impl Into<String>,
    label: impl AsRef<str>,
    checked: bool,
) -> tauri::Result<tauri::menu::CheckMenuItem<Wry>> {
    CheckMenuItemBuilder::with_id(id.into(), label.as_ref())
        .checked(checked)
        .build(app)
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let state = app.state::<Arc<AppState>>();
    let s = state.settings.get();
    let m = &s.metronome;
    let status = state.engine.status();
    let running = state.engine.is_running();

    let header =
        MenuItemBuilder::with_id("header", format!("TaktDock {}", env!("CARGO_PKG_VERSION")))
            .enabled(false)
            .build(app)?;

    // A waiting update goes above everything else. Anywhere further down and it
    // is a line in a long menu nobody reads.
    let update_ready = match (
        crate::update::available().filter(|_| crate::update::self_updatable()),
        crate::update::installing(),
    ) {
        (_, true) => Some(
            MenuItemBuilder::with_id("update:install", "Updating, please wait")
                .enabled(false)
                .build(app)?,
        ),
        (Some(v), false) => Some(
            MenuItemBuilder::with_id("update:install", format!("Update to {v} and restart"))
                .build(app)?,
        ),
        (None, false) => None,
    };

    let play =
        MenuItemBuilder::with_id("play", if running { "Stop" } else { "Start" }).build(app)?;

    let mut tempo = SubmenuBuilder::new(app, "Tempo");
    for bpm in TEMPO_PRESETS {
        tempo = tempo.item(&radio(
            app,
            format!("tempo:{bpm}"),
            format!("{bpm} bpm"),
            m.bpm == bpm,
        )?);
    }
    let tempo = tempo.build()?;

    let mut meter = SubmenuBuilder::new(app, "Meter");
    for (beats, unit) in METERS {
        meter = meter.item(&radio(
            app,
            format!("meter:{beats}/{unit}"),
            format!("{beats}/{unit}"),
            (m.beats_per_bar, m.beat_unit) == (beats, unit),
        )?);
    }
    let meter = meter
        .separator()
        .item(&radio(
            app,
            "accent",
            "Accent the first beat",
            m.accent_first,
        )?)
        .build()?;

    let mut sub = SubmenuBuilder::new(app, "Subdivision");
    for n in SUBDIVISIONS {
        sub = sub.item(&radio(
            app,
            format!("sub:{n}"),
            control::subdivision_name(n),
            m.subdivision == n,
        )?);
    }
    let sub = sub.build()?;

    let mut sound = SubmenuBuilder::new(app, "Sound")
        .item(&radio(app, "sound:click", "Click", m.sound == "click")?)
        .item(&radio(app, "sound:wood", "Wood", m.sound == "wood")?)
        .separator();
    for v in VOLUME_STEPS {
        sound = sound.item(&radio(
            app,
            format!("vol:{v}"),
            format!("Volume {v}%"),
            m.volume == v,
        )?);
    }
    let sound = sound.build()?;

    let device = device_menu(app, &s, &status)?;

    let show_widget = radio(app, "show_widget", "Show widget", s.widget.visible)?;

    // Pinning is only offered where there is a panel this widget understands.
    let mut placement = SubmenuBuilder::new(app, "Placement").item(&radio(
        app,
        "place:float",
        "Floating widget",
        !s.widget.taskbar_mode(),
    )?);
    if monitor::supports_panel_docking() {
        placement = placement.item(&radio(
            app,
            "place:taskbar",
            "Pinned to taskbar",
            s.widget.taskbar_mode(),
        )?);
    } else {
        // Listed and greyed rather than left out. A Placement submenu holding
        // one placement reads as something that failed to load, and the answer
        // to "why not in the panel" belongs where the question is.
        placement = placement.item(
            &MenuItemBuilder::with_id("place:taskbar", "Pinned to taskbar (Windows only)")
                .enabled(false)
                .build(app)?,
        );
    }
    let placement = placement.build()?;

    let mut mon = SubmenuBuilder::new(app, "Monitor");
    {
        let monitors = monitor::enumerate();
        let selected = s.widget.monitor_stable_id.clone();
        let mut saved_present = false;

        for (i, mi) in monitors.iter().enumerate() {
            let checked = selected.as_deref() == Some(mi.stable_id.as_str())
                || (selected.is_none() && mi.primary);
            if selected.as_deref() == Some(mi.stable_id.as_str()) {
                saved_present = true;
            }
            let label = format!(
                "{}: {}{}",
                i + 1,
                mi.friendly_name,
                if mi.primary { " (primary)" } else { "" }
            );
            mon = mon.item(&radio(
                app,
                format!("mon:{}", mi.stable_id),
                label,
                checked,
            )?);
        }

        // A saved monitor that is currently unplugged stays listed so the
        // choice is visible and comes back when the display returns.
        if let (Some(id), false) = (selected.as_deref(), saved_present) {
            let name = s
                .widget
                .monitor_friendly_name
                .clone()
                .unwrap_or_else(|| "saved monitor".into());
            mon = mon.item(&radio(
                app,
                format!("mon:{id}"),
                format!("(not connected) {name}"),
                true,
            )?);
        }
    }
    let mon = mon
        .separator()
        .item(&MenuItemBuilder::with_id("reset_position", "Reset position").build(app)?)
        .build()?;

    let mut appearance = SubmenuBuilder::new(app, "Appearance");
    for (id, label) in [
        ("theme:system", "Theme: System"),
        ("theme:dark", "Theme: Dark"),
        ("theme:light", "Theme: Light"),
    ] {
        let key = id.split(':').nth(1).unwrap_or("system");
        appearance = appearance.item(&radio(app, id, label, s.appearance.theme == key)?);
    }
    let appearance = appearance
        .separator()
        .item(&radio(app, "expanded", "Expanded", s.appearance.expanded)?)
        .item(&radio(app, "compact", "Compact", s.appearance.compact)?)
        .item(&radio(
            app,
            "animations",
            "Animations",
            s.appearance.animations,
        )?)
        .separator()
        .item(&radio(
            app,
            "click_through",
            "Click-through",
            s.widget.click_through,
        )?)
        .item(&radio(
            app,
            "hide_fullscreen",
            "Hide during fullscreen apps",
            s.widget.hide_on_fullscreen,
        )?)
        .build()?;

    let autostart = radio(app, "autostart", AUTOSTART_LABEL, s.autostart)?;

    let diag = SubmenuBuilder::new(app, "Diagnostics")
        .item(&MenuItemBuilder::with_id("diag:report", "Save report").build(app)?)
        .item(&MenuItemBuilder::with_id("diag:logs", "Open log folder").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("update:check", "Check for updates").build(app)?)
        .item(&radio(
            app,
            "update:auto",
            "Check automatically",
            s.updates.check,
        )?)
        .build()?;

    let mut builder = MenuBuilder::new(app).item(&header);
    if let Some(item) = &update_ready {
        builder = builder.separator().item(item);
    }
    builder
        .separator()
        .item(&play)
        .item(&tempo)
        .item(&meter)
        .item(&sub)
        .item(&sound)
        .item(&device)
        .separator()
        .item(&show_widget)
        .item(&placement)
        .item(&mon)
        .item(&appearance)
        .item(&autostart)
        .separator()
        .item(&MenuItemBuilder::with_id("restart", "Force restart").build(app)?)
        .item(&MenuItemBuilder::with_id("open_settings", "Open settings file").build(app)?)
        .item(&diag)
        .separator()
        .item(&MenuItemBuilder::with_id("exit", "Exit").build(app)?)
        .build()
}

/// System default plus every endpoint the last scan found. A chosen device
/// that is not connected stays listed and ticked, the same way a monitor does,
/// so the choice is visible and comes back with the device.
fn device_menu(
    app: &AppHandle,
    s: &Settings,
    status: &crate::audio::Status,
) -> tauri::Result<Submenu<Wry>> {
    let chosen = s.metronome.output_device.as_deref();
    let mut menu = SubmenuBuilder::new(app, "Output device");
    if let Some(problem) = &status.problem {
        menu = menu
            .item(
                &MenuItemBuilder::with_id("device:problem", problem)
                    .enabled(false)
                    .build(app)?,
            )
            .separator();
    }
    menu = menu.item(&radio(app, "device:", "System default", chosen.is_none())?);
    for name in &status.devices {
        menu = menu.item(&radio(
            app,
            format!("device:{name}"),
            name,
            chosen == Some(name.as_str()),
        )?);
    }
    if let Some(name) = chosen {
        if !status.devices.iter().any(|d| d == name) {
            menu = menu.item(&radio(
                app,
                format!("device:{name}"),
                format!("(not connected) {name}"),
                true,
            )?);
        }
    }
    menu.build()
}

fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id().0.clone();
    let state = app.state::<Arc<AppState>>().inner().clone();

    let mut needs_rebuild = true;
    let mut needs_config = false;
    let mut needs_reposition = false;

    match id.as_str() {
        "play" => {
            control::toggle(app);
            needs_rebuild = false;
        }
        "accent" => {
            control::update(app, |m| m.accent_first = !m.accent_first);
            needs_rebuild = false;
        }
        "update:install" => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move { crate::update::install(&handle).await });
            needs_rebuild = false;
        }
        "update:check" => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                // A check the user asked for gets an answer either way. The
                // background one stays silent, but silence in response to a
                // click reads as a broken button.
                let found = crate::update::check(&handle).await;
                if found.is_none() {
                    diagnostics::message_box(
                        "TaktDock",
                        &format!(
                            "TaktDock {} is the newest version.",
                            env!("CARGO_PKG_VERSION")
                        ),
                    );
                }
            });
            needs_rebuild = false;
        }
        "update:auto" => {
            state
                .settings
                .update(|s| s.updates.check = !s.updates.check);
        }
        "restart" => {
            // app.restart() hands off to a child that immediately finds this
            // still-running instance through the single instance lock, defers
            // to it, and exits, leaving nothing behind once the parent goes.
            // A detached launcher that waits for this process to disappear is
            // the reliable version.
            relaunch_after_exit();
            crate::state::begin_quit();
            app.exit(0);
            return;
        }
        "show_widget" => window::toggle_visibility(app),
        "animations" => {
            state
                .settings
                .update(|s| s.appearance.animations = !s.appearance.animations);
            needs_config = true;
        }
        "expanded" => {
            state
                .settings
                .update(|s| s.appearance.expanded = !s.appearance.expanded);
            needs_config = true;
            // The height is measured by the interface once it has redrawn, but
            // the pass is asked for here as well so nothing waits on that.
            needs_reposition = true;
        }
        "compact" => {
            state
                .settings
                .update(|s| s.appearance.compact = !s.appearance.compact);
            needs_config = true;
            // Compact decides how wide the pinned strip is, so it has to be
            // measured again now rather than on whatever moves the widget next.
            needs_reposition = true;
        }
        "reset_position" => {
            state.settings.update(|s| {
                s.widget.monitor_stable_id = None;
                s.widget.monitor_friendly_name = None;
                s.widget.edge_offset_x = 0;
                s.widget.edge_offset_y = 0;
            });
            needs_reposition = true;
        }
        "click_through" => {
            let wanted = !state.settings.get().widget.click_through;
            // Applied first and written second, with a marker in between: if
            // applying it takes the process down, the next launch finds the
            // marker and turns it back off instead of dying the same way.
            settings::begin_apply("click_through");
            window::set_click_through(app, wanted);
            state.settings.update(|s| s.widget.click_through = wanted);
            settings_applied_later();
        }
        "hide_fullscreen" => {
            state
                .settings
                .update(|s| s.widget.hide_on_fullscreen = !s.widget.hide_on_fullscreen);
        }
        "autostart" => {
            let enabled = state
                .settings
                .update(|s| s.autostart = !s.autostart)
                .autostart;
            crate::autostart::apply(app, enabled);
        }
        "open_settings" => open_path(&settings::settings_path()),
        "diag:report" => diagnostics::save_report(app, true),
        "diag:logs" => open_path(&settings::data_dir().join("logs")),
        "exit" => {
            // Quitting here is deliberate, so the watchdog leaves it alone.
            settings::mark_stay_closed();
            crate::state::begin_quit();
            state.engine.shutdown();
            app.exit(0);
            return;
        }
        other => {
            needs_rebuild = false;
            if let Some(bpm) = other.strip_prefix("tempo:").and_then(|v| v.parse().ok()) {
                control::set_tempo(app, bpm);
            } else if let Some((beats, unit)) = other
                .strip_prefix("meter:")
                .and_then(|v| v.split_once('/'))
                .and_then(|(b, u)| Some((b.parse().ok()?, u.parse().ok()?)))
            {
                control::set_meter(app, beats, unit);
            } else if let Some(n) = other.strip_prefix("sub:").and_then(|v| v.parse().ok()) {
                control::set_subdivision(app, n);
            } else if let Some(sound) = other.strip_prefix("sound:") {
                let sound = sound.to_string();
                control::update(app, |m| m.sound = sound);
            } else if let Some(v) = other.strip_prefix("vol:").and_then(|v| v.parse().ok()) {
                control::update(app, |m| m.volume = v);
            } else if let Some(name) = other.strip_prefix("device:") {
                if name != "problem" {
                    let chosen = (!name.is_empty()).then(|| name.to_string());
                    control::update(app, |m| m.output_device = chosen);
                }
            } else if let Some(mode) = other.strip_prefix("place:") {
                let mode = mode.to_string();
                if mode == "taskbar" {
                    settings::begin_apply("placement");
                    settings_applied_later();
                }
                state.settings.update(|s| s.widget.mode = mode.clone());
                needs_reposition = true;
                needs_config = true;
                needs_rebuild = true;
            } else if let Some(theme) = other.strip_prefix("theme:") {
                let theme = theme.to_string();
                state
                    .settings
                    .update(|s| s.appearance.theme = theme.clone());
                needs_config = true;
                needs_rebuild = true;
            } else if let Some(mon_id) = other.strip_prefix("mon:") {
                let mon_id = mon_id.to_string();
                let friendly = monitor::enumerate()
                    .into_iter()
                    .find(|m| m.stable_id == mon_id)
                    .map(|m| m.friendly_name);
                state.settings.update(|s| {
                    s.widget.monitor_stable_id = Some(mon_id.clone());
                    s.widget.monitor_friendly_name = friendly.clone();
                    s.widget.edge_offset_x = 0;
                    s.widget.edge_offset_y = 0;
                });
                needs_reposition = true;
                needs_rebuild = true;
            }
        }
    }

    if needs_reposition {
        window::reposition(app);
    }
    if needs_config {
        let _ = app.emit("config", state.appearance());
    }
    if needs_rebuild {
        rebuild(app);
    }
}

/// Clears the "applying" marker once the change has survived a few turns of
/// the event loop. The abort this guards against came on the next turn, not
/// inside the call, so clearing it straight away would prove nothing.
fn settings_applied_later() {
    tauri::async_runtime::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        settings::end_apply();
    });
}

/// Which icon is up, so it is only swapped when it changes.
static LAST_ICON: AtomicU8 = AtomicU8::new(u8::MAX);

/// Grey when stopped, the accent colour while playing, and a tooltip with the
/// tempo, the meter and whatever is wrong with the output.
pub fn refresh_badge(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let running = state.engine.is_running();
    let kind = running as u8;
    if LAST_ICON.swap(kind, Ordering::Relaxed) != kind {
        let bytes = if running { ICON_PLAYING } else { ICON_IDLE };
        if let Ok(icon) = Image::from_bytes(bytes) {
            let _ = tray.set_icon(Some(icon));
        }
    }
    let _ = tray.set_tooltip(Some(tooltip(app)));
}

fn tooltip(app: &AppHandle) -> String {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return "TaktDock".into();
    };
    let m = state.settings.get().metronome;
    let mut text = format!(
        "TaktDock, {} bpm {}/{}, {}",
        m.bpm,
        m.beats_per_bar,
        m.beat_unit,
        if state.engine.is_running() {
            "playing"
        } else {
            "stopped"
        }
    );
    if let Some(problem) = state.engine.status().problem {
        text.push('\n');
        text.push_str(&problem);
    }
    text
}

/// Starts a detached copy of this executable that waits for this process to exit
/// and then launches a fresh one. Waiting matters: the single instance lock is
/// only released once this process is gone.
///
/// No console program does the waiting: a console program started in the
/// background can put a window on the user's screen, and this app has no
/// business doing that.
fn relaunch_after_exit() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = crate::shell::quiet_command(&exe.to_string_lossy())
        .args(["--relaunch-after", &std::process::id().to_string()])
        .spawn();
}

fn open_path(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(path));
    crate::shell::open(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scrolling through tempos must not rebuild the menu on every notch, only
    /// when the tempo lands on or leaves a preset.
    #[test]
    fn only_presets_reach_the_menu_signature() {
        assert_eq!(preset_match(120), Some(120));
        assert_eq!(preset_match(121), None);
        assert_eq!(preset_match(119), None);
        let changes = (30..=300u32)
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| preset_match(w[0]) != preset_match(w[1]))
            .count();
        assert_eq!(changes, TEMPO_PRESETS.len() * 2);
    }
}
