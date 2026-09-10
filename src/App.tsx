import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { BarProgress } from "@/components/BarProgress";
import { BeatDots, type LitBeat } from "@/components/BeatDots";
import { Pill } from "@/components/Pill";
import { PlayButton } from "@/components/PlayButton";
import { TempoSlider } from "@/components/TempoSlider";
import { applyAccent } from "@/lib/accent";
import { TapTempo } from "@/lib/tap";
import type { AppearanceConfig, BeatEvent, Transport } from "@/lib/types";
import { beatMs, cn, soundLabel, subdivisionLabel, subdivisionName } from "@/lib/utils";

const DEFAULT_APPEARANCE: AppearanceConfig = {
  theme: "system",
  resolved_theme: "dark",
  animations: true,
  compact: false,
  expanded: false,
  placement: "float",
  motion_allowed: true,
  accent: null,
};

const DEFAULT_TRANSPORT: Transport = {
  running: false,
  bpm: 120,
  beats_per_bar: 4,
  beat_unit: 4,
  subdivision: 1,
  accent_first: true,
  sound: "click",
  volume: 75,
  output_device: null,
  active_device: null,
  device_problem: null,
};

/** An event further ahead than this is shown at once rather than waited for. */
const MAX_SCHEDULE_MS = 300;

/** Wheel travel that makes one tempo step. A mouse notch is more than this; a
 *  touchpad gets there over several small events instead of one per event. */
const WHEEL_STEP = 50;

interface Live {
  lit: LitBeat;
  /** The bar the click belongs to, counted by the audio thread from zero. */
  bar: number;
  beats: number;
  bpm: number;
}

export default function App() {
  const [appearance, setAppearance] = React.useState<AppearanceConfig>(DEFAULT_APPEARANCE);
  const [transport, setTransport] = React.useState<Transport>(DEFAULT_TRANSPORT);
  const [live, setLive] = React.useState<Live | null>(null);
  const [osReduced, setOsReduced] = React.useState(false);
  // A colour the picker is showing while its handle is dragged. It lives only
  // until the next `config`, which carries whatever was finally chosen.
  const [previewAccent, setPreviewAccent] = React.useState<string | null | undefined>(undefined);

  React.useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const apply = () => setOsReduced(mq.matches);
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, []);

  React.useEffect(() => {
    const unlisteners: Array<() => void> = [];
    const timers = new Set<number>();

    listen<AppearanceConfig>("config", (e) => {
      setAppearance(e.payload);
      setPreviewAccent(undefined);
    }).then((un) => unlisteners.push(un));
    listen<string | null>("accent-preview", (e) => setPreviewAccent(e.payload)).then((un) =>
      unlisteners.push(un),
    );
    listen<Transport>("transport", (e) => setTransport(e.payload)).then((un) =>
      unlisteners.push(un),
    );
    // The event arrives a little before the click is heard: the audio thread
    // renders ahead of the speaker. Waiting until the stamped time lines the
    // dot up with the sound instead of with the render.
    listen<BeatEvent>("beat", (e) => {
      const ev = e.payload;
      if (ev.kind === "sub") return;
      const show = () =>
        setLive((prev) => ({
          lit: {
            beat: ev.beat,
            kind: ev.kind,
            seq: (prev?.lit.seq ?? 0) + 1,
            beatMs: beatMs(ev.bpm),
          },
          bar: ev.bar,
          beats: ev.beats_per_bar,
          bpm: ev.bpm,
        }));
      const wait = ev.at_ms - Date.now();
      if (wait > 4 && wait < MAX_SCHEDULE_MS) {
        const id = window.setTimeout(() => {
          timers.delete(id);
          show();
        }, wait);
        timers.add(id);
      } else {
        show();
      }
    }).then((un) => unlisteners.push(un));

    invoke<AppearanceConfig>("get_appearance").then(setAppearance).catch(() => {});
    invoke<Transport>("get_transport").then(setTransport).catch(() => {});

    return () => {
      unlisteners.forEach((un) => un());
      timers.forEach((id) => window.clearTimeout(id));
    };
  }, []);

  // A stopped metronome shows no lit beat, and the next start opens on a
  // fresh bar rather than the one that was interrupted.
  React.useEffect(() => {
    if (!transport.running) setLive(null);
  }, [transport.running]);

  React.useEffect(() => {
    document.documentElement.dataset.theme = appearance.resolved_theme;
  }, [appearance.resolved_theme]);

  const accent = previewAccent === undefined ? appearance.accent : previewAccent;
  React.useEffect(() => {
    applyAccent(
      document.documentElement,
      accent ?? null,
      appearance.resolved_theme === "light" ? "light" : "dark",
    );
  }, [accent, appearance.resolved_theme]);

  const motionOn = appearance.animations && !osReduced && appearance.motion_allowed;

  React.useEffect(() => {
    const onContext = (e: MouseEvent) => {
      e.preventDefault();
      if ((e.target as Element | null)?.closest?.(".td-pill")) return;
      invoke("show_context_menu").catch(() => {});
    };
    window.addEventListener("contextmenu", onContext);
    return () => window.removeEventListener("contextmenu", onContext);
  }, []);

  const startDrag = React.useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    getCurrentWindow()
      .startDragging()
      .catch(() => {});
  }, []);

  // The widget never takes focus, so there is no keyboard: the wheel is the
  // quick way to nudge the tempo, anywhere over it. Shift turns the wheel
  // sideways in the browser, which is why either axis counts.
  const wheel = React.useRef(0);
  const onWheel = React.useCallback((e: React.WheelEvent) => {
    const d = e.deltaY || e.deltaX;
    if (d === 0) return;
    const step = e.shiftKey ? 5 : 1;
    if (e.deltaMode !== 0) {
      invoke("nudge_tempo", { delta: d < 0 ? step : -step }).catch(() => {});
      return;
    }
    wheel.current += d;
    if (Math.abs(wheel.current) >= WHEEL_STEP) {
      invoke("nudge_tempo", { delta: wheel.current < 0 ? step : -step }).catch(() => {});
      wheel.current = 0;
    }
  }, []);

  const tapper = React.useRef(new TapTempo());
  const tap = React.useCallback(() => {
    const bpm = tapper.current.tap(performance.now());
    if (bpm !== null) invoke("set_tempo", { bpm }).catch(() => {});
  }, []);

  const toggle = React.useCallback(() => {
    // Shown at once; the transport event that follows confirms it.
    setTransport((t) => ({ ...t, running: !t.running }));
    invoke("toggle_play").catch(() => {});
  }, []);
  const setTempo = React.useCallback((bpm: number) => {
    setTransport((t) => ({ ...t, bpm }));
    invoke("set_tempo", { bpm }).catch(() => {});
  }, []);
  const cycleMeter = React.useCallback(() => {
    invoke("cycle_meter").catch(() => {});
  }, []);
  const cycleSubdivision = React.useCallback(() => {
    invoke("cycle_subdivision").catch(() => {});
  }, []);

  const pinned = appearance.placement === "taskbar";
  const expanded = !pinned && appearance.expanded;
  const compact = pinned && appearance.compact;

  const shellRef = React.useRef<HTMLDivElement | null>(null);
  const contentRef = React.useRef<HTMLDivElement | null>(null);

  // The floating window is sized to what is actually on screen, so the plain
  // layout does not sit in a window built for the expanded one. Taskbar
  // placement is left alone: there the height belongs to the strip.
  React.useEffect(() => {
    if (pinned) return;
    const content = contentRef.current;
    const shell = shellRef.current;
    const rows = content?.parentElement;
    if (!content || !shell || !rows) return;

    let last = 0;
    const report = () => {
      const rowsStyle = getComputedStyle(rows);
      const shellStyle = getComputedStyle(shell);
      // Read the frame from the stylesheet rather than repeating it here, so
      // changing the padding in one place cannot leave the window wrong.
      const frame =
        parseFloat(rowsStyle.paddingTop) +
        parseFloat(rowsStyle.paddingBottom) +
        parseFloat(shellStyle.borderTopWidth) +
        parseFloat(shellStyle.borderBottomWidth);
      const needed = Math.ceil(content.getBoundingClientRect().height + frame);
      if (needed > 0 && needed !== last) {
        last = needed;
        invoke("set_content_height", { height: needed }).catch(() => {});
      }
    };

    report();
    const observer = new ResizeObserver(report);
    observer.observe(content);
    return () => observer.disconnect();
  }, [pinned, expanded]);

  // What the dots and the sweep draw from. While playing, the meter comes
  // from the clicks themselves, because a new meter only lands at the next
  // bar and the picture has to follow the sound.
  const running = transport.running;
  const beats = running && live ? live.beats : transport.beats_per_bar;
  const lit = running && live ? live.lit : null;
  const glow = lit?.kind === "accent" ? 1 : 0.6;

  const meterLabel = `${transport.beats_per_bar}/${transport.beat_unit}`;
  const subLabel = subdivisionLabel(transport.subdivision, transport.beat_unit);

  const play = (
    <PlayButton running={running} glow={glow} motionOn={motionOn} onToggle={toggle} />
  );
  const meter = (
    <Pill
      menu="meter"
      label={meterLabel}
      title={`Meter ${meterLabel}${transport.accent_first ? ", first beat accented" : ""}. Click for the next, right click for the list.`}
      onCycle={cycleMeter}
    />
  );
  const grid = (
    <Pill
      menu="subdivision"
      label={subLabel}
      title={`Subdivision: ${subdivisionName(transport.subdivision)}. Click for the next, right click for the list.`}
      onCycle={cycleSubdivision}
    />
  );
  const dots = <BeatDots beats={beats} lit={lit} motionOn={motionOn} />;
  const slider = <TempoSlider bpm={transport.bpm} motionOn={motionOn} onSet={setTempo} />;
  const tapLink = (
    <span className="td-link" role="button" title="Tap four times or more to set the tempo" onClick={tap}>
      tap
    </span>
  );
  const bpmValue = (
    <span className="td-value td-tabular" title={`${transport.bpm} beats per minute`}>
      <span className="td-bpm">{transport.bpm}</span>
    </span>
  );

  return (
    <div
      className={cn("td-shell", compact && "td-compact")}
      data-placement={appearance.placement}
      ref={shellRef}
      onWheel={onWheel}
    >
      {!pinned && (
        <div className="td-grip" onMouseDown={startDrag} title="Drag">
          <div className="td-grip-dots" />
        </div>
      )}
      <div className="td-rows">
        <div className="td-content" ref={contentRef}>
          {pinned ? (
            compact ? (
              <div className="td-group">
                <div className="td-row">
                  <span className="td-mark">{play}</span>
                  <span className="td-label">{meter}</span>
                  {bpmValue}
                </div>
                <div className="td-row">
                  <span className="td-mark" />
                  <span className="td-slot">{dots}</span>
                </div>
              </div>
            ) : (
              <div className="td-group">
                <div className="td-row">
                  <span className="td-mark">{play}</span>
                  <span className="td-label">{meter}</span>
                  <span className="td-slot">{dots}</span>
                  {bpmValue}
                </div>
                <div className="td-row">
                  <span className="td-mark" />
                  <span className="td-label">{tapLink}</span>
                  {slider}
                  <span className="td-value td-unit">bpm</span>
                </div>
              </div>
            )
          ) : (
            <>
              <div className="td-group">
                <div className="td-row">
                  <span className="td-mark">{play}</span>
                  <span className="td-label">{meter}</span>
                  {slider}
                  {bpmValue}
                  <span className="td-tail">bpm</span>
                </div>
                <div className="td-row">
                  <span className="td-mark" />
                  <span className="td-label" />
                  <span className="td-slot">{dots}</span>
                  <span className="td-value">{tapLink}</span>
                  <span className="td-tail">{grid}</span>
                </div>
              </div>
              {expanded && (
                <Expanded
                  transport={transport}
                  live={running ? live : null}
                  glow={glow}
                  motionOn={motionOn}
                />
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * The taller floating layout: the bar as a sweep, and what is playing where.
 */
function Expanded({
  transport,
  live,
  glow,
  motionOn,
}: {
  transport: Transport;
  live: Live | null;
  glow: number;
  motionOn: boolean;
}) {
  const beats = live?.beats ?? transport.beats_per_bar;
  const bpm = live?.bpm ?? transport.bpm;
  const device = transport.active_device ?? transport.output_device ?? "System default";
  const deviceTitle = transport.device_problem
    ? transport.device_problem
    : `Output: ${transport.output_device ?? "system default"}${
        transport.active_device && transport.active_device !== transport.output_device
          ? ` (playing on ${transport.active_device})`
          : ""
      }`;
  const onVolumeWheel = (e: React.WheelEvent) => {
    e.stopPropagation();
    const d = e.deltaY || e.deltaX;
    if (d !== 0) invoke("nudge_volume", { delta: d < 0 ? 5 : -5 }).catch(() => {});
  };

  return (
    <div className="td-group">
      <div className="td-row">
        <span className="td-mark" />
        <span className="td-label">bar</span>
        <span className="td-slot">
          <BarProgress
            running={transport.running}
            bar={live?.bar ?? 0}
            beat={live?.lit.beat ?? 0}
            beats={beats}
            barMs={beats * beatMs(bpm)}
            glow={glow}
            motionOn={motionOn}
          />
        </span>
        <span className="td-value td-tabular" title="Bars since play was pressed">
          {live ? live.bar + 1 : ""}
        </span>
        <span className="td-tail" />
      </div>
      <div className="td-row">
        <span className="td-mark" />
        <span className="td-label">out</span>
        <span className={cn("td-slot td-device", transport.device_problem && "td-warn")} title={deviceTitle}>
          {device}
        </span>
        <span
          className="td-value td-tabular"
          title="Volume. Scroll to change it."
          onWheel={onVolumeWheel}
        >
          {transport.volume}%
        </span>
        <span className="td-tail">
          <span
            className="td-pill"
            role="button"
            title={`Sound: ${soundLabel(transport.sound)}. Click for the next one.`}
            onClick={() => invoke("cycle_sound").catch(() => {})}
          >
            {soundLabel(transport.sound)}
          </span>
        </span>
      </div>
    </div>
  );
}
