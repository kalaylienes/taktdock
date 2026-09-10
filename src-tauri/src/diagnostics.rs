//! The diagnostic report, written from the tray menu or by `--diagnose`.

use std::sync::Arc;

use chrono::Utc;
use tauri::{AppHandle, Manager};

use crate::settings::data_dir;
use crate::state::AppState;

/// Writes an environment summary next to the log files. From the tray the
/// folder is opened so the file is right there; from `--diagnose` it is not,
/// because that flag is also how the report is taken from a script, and a
/// script has no business putting an Explorer window on somebody's screen.
pub fn save_report(app: &AppHandle, reveal: bool) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let path = data_dir().join(format!(
        "diagnostic-{}.txt",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    let _ = std::fs::create_dir_all(data_dir());
    match std::fs::write(&path, report(&state)) {
        Ok(_) => {
            tracing::info!("diagnostic report written to {}", path.display());
            if reveal {
                crate::shell::open(&data_dir());
            }
        }
        Err(e) => message_box("TaktDock", &format!("The report could not be written: {e}")),
    }
}

fn report(state: &AppState) -> String {
    let settings = state.settings.get();
    let status = state.engine.status();
    let stats = state.engine.stats();

    let mut out = String::new();
    out.push_str(&format!(
        "TaktDock diagnostic report\n{}\n\n",
        Utc::now().to_rfc3339()
    ));
    out.push_str(&format!("version: {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("desktop: {}\n", crate::desktop()));
    out.push_str(&format!("uptime: {}s\n", crate::uptime_secs()));
    out.push_str(&format!("data directory: {}\n\n", data_dir().display()));

    out.push_str("process\n");
    match process_usage() {
        Some(p) => {
            out.push_str(&format!(
                "  resident memory: {:.1} MB\n",
                p.rss_bytes as f64 / 1_048_576.0
            ));
            out.push_str(&format!(
                "  peak resident memory: {:.1} MB\n",
                p.peak_rss_bytes as f64 / 1_048_576.0
            ));
            let up = crate::uptime_secs().max(1) as f64;
            out.push_str(&format!(
                "  cpu time: {:.2}s over {:.0}s, {:.3}% of one core on average\n",
                p.cpu_secs,
                up,
                p.cpu_secs / up * 100.0
            ));
        }
        None => out.push_str("  not available on this platform\n"),
    }

    out.push_str("\naudio\n");
    out.push_str(&format!("  playing: {}\n", state.engine.is_running()));
    out.push_str(&format!(
        "  chosen output: {}\n",
        settings
            .metronome
            .output_device
            .as_deref()
            .unwrap_or("system default")
    ));
    out.push_str(&format!(
        "  open on: {}\n",
        status.active_device.as_deref().unwrap_or(if status.silent {
            "nothing, keeping time silently"
        } else {
            "nothing (the stream opens on play)"
        })
    ));
    out.push_str(&format!(
        "  sample rate: {}\n",
        status
            .sample_rate
            .map(|r| format!("{r} Hz"))
            .unwrap_or_else(|| "-".into())
    ));
    out.push_str(&format!(
        "  problem: {}\n",
        status.problem.as_deref().unwrap_or("none")
    ));
    out.push_str(&format!("  callbacks: {}\n", stats.callbacks));
    out.push_str(&format!(
        "  frames rendered by this stream: {}\n",
        stats.frames
    ));
    out.push_str(&format!(
        "  slowest callback: {} us\n",
        stats.max_callback_us
    ));
    out.push_str(&format!("  xruns reported: {}\n", stats.xruns));
    out.push_str(&format!(
        "  output latency: {:.1} ms\n",
        stats.output_latency_us as f64 / 1000.0
    ));
    out.push_str(&format!(
        "  device clock against system clock: {}\n",
        stats
            .device_clock_ppm
            .map(|p| format!("{p:+.1} ppm"))
            .unwrap_or_else(|| "not measured yet".into())
    ));
    out.push_str("  outputs found:\n");
    if status.devices.is_empty() {
        out.push_str("    none\n");
    }
    for d in &status.devices {
        out.push_str(&format!("    {d}\n"));
    }

    out.push_str("\nmonitors\n");
    for m in crate::monitor::enumerate() {
        out.push_str(&format!(
            "  {} | {} | dpi {} | primary {} | id {}\n",
            m.gdi_name, m.friendly_name, m.dpi, m.primary, m.stable_id
        ));
    }

    out.push_str("\nsettings\n");
    out.push_str(&serde_json::to_string_pretty(&settings).unwrap_or_default());
    out.push_str("\n\nlast log lines\n");
    out.push_str(&last_log_lines(60));
    out.push('\n');
    out
}

struct ProcessUsage {
    rss_bytes: u64,
    peak_rss_bytes: u64,
    cpu_secs: f64,
}

#[cfg(windows)]
fn process_usage() -> Option<ProcessUsage> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    unsafe {
        let process = GetCurrentProcess();
        let mut mem = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        GetProcessMemoryInfo(process, &mut mem, mem.cb).ok()?;

        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user).ok()?;
        let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64;
        Some(ProcessUsage {
            rss_bytes: mem.WorkingSetSize as u64,
            peak_rss_bytes: mem.PeakWorkingSetSize as u64,
            // FILETIME counts hundreds of nanoseconds.
            cpu_secs: (ticks(kernel) + ticks(user)) / 10_000_000.0,
        })
    }
}

#[cfg(target_os = "linux")]
fn process_usage() -> Option<ProcessUsage> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb = |key: &str| -> Option<u64> {
        status
            .lines()
            .find(|l| l.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // Fields after the command name, which is in parentheses and may contain
    // spaces of its own.
    let after = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = after.split_whitespace().collect();
    let utime: f64 = fields.get(11)?.parse().ok()?;
    let stime: f64 = fields.get(12)?.parse().ok()?;
    // Clock ticks per second is 100 on every Linux this is built for.
    Some(ProcessUsage {
        rss_bytes: kb("VmRSS:")? * 1024,
        peak_rss_bytes: kb("VmHWM:")? * 1024,
        cpu_secs: (utime + stime) / 100.0,
    })
}

#[cfg(not(any(windows, target_os = "linux")))]
fn process_usage() -> Option<ProcessUsage> {
    None
}

/// The end of the newest log file, for the crash file and the report.
pub fn last_log_lines(n: usize) -> String {
    let dir = data_dir().join("logs");
    let newest = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("taktdock"))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    let Some(entry) = newest else {
        return "no log file".into();
    };
    let Ok(text) = std::fs::read_to_string(entry.path()) else {
        return "the log file could not be read".into();
    };
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(windows)]
pub fn message_box(title: &str, text: &str) {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from(title),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

/// There is no one dialog on Linux, and pulling in a toolkit to draw one for
/// two rare confirmations is more machinery than the job deserves.
/// `notify-send` is what a desktop with libnotify already has; without it the
/// message goes to the log, which is where the report itself lives anyway.
#[cfg(not(windows))]
pub fn message_box(title: &str, text: &str) {
    let sent = std::process::Command::new("notify-send")
        .arg(title)
        .arg(text)
        .spawn()
        .is_ok();
    if !sent {
        tracing::info!("{text}");
    }
}
