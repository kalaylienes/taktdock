import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { Bar, INSTANT } from "@/components/Bar";
import {
  DEFAULT_ACCENT,
  PRESETS,
  applyAccent,
  hexToHsv,
  hsvToHex,
  normaliseHex,
  type Hsv,
  type Theme,
} from "@/lib/accent";
import type { AppearanceConfig } from "@/lib/types";
import { cn } from "@/lib/utils";

/**
 * The accent colour picker, in a window of its own.
 *
 * Every colour there is: a saturation and brightness square, a hue strip, a
 * hex field, and a row of starting points. While a handle is being dragged
 * the widget is repainted on every frame without anything being saved; the
 * colour is stored when the pointer lets go, when a hex value is entered, or
 * when a preset is picked. Cancel puts back the colour the window opened
 * with.
 */
export default function Picker() {
  const [theme, setTheme] = React.useState<Theme>("dark");
  // `undefined` until the backend has answered; `null` is the built in colour.
  const [original, setOriginal] = React.useState<string | null | undefined>(undefined);
  const [hsv, setHsv] = React.useState<Hsv>(() => hexToHsv(DEFAULT_ACCENT));
  const [text, setText] = React.useState(DEFAULT_ACCENT);
  const [invalid, setInvalid] = React.useState(false);
  const [isDefault, setIsDefault] = React.useState(true);

  const hex = hsvToHex(hsv);

  React.useEffect(() => {
    const unlisteners: Array<() => void> = [];
    invoke<AppearanceConfig>("get_appearance")
      .then((a) => {
        if (!a) return;
        setTheme(a.resolved_theme === "light" ? "light" : "dark");
        setOriginal(a.accent ?? null);
        const start = a.accent ?? DEFAULT_ACCENT;
        setHsv(hexToHsv(start));
        setText(start);
        setIsDefault(!a.accent);
      })
      .catch(() => setOriginal(null));
    listen<AppearanceConfig>("config", (e) =>
      setTheme(e.payload.resolved_theme === "light" ? "light" : "dark"),
    ).then((un) => unlisteners.push(un));
    return () => unlisteners.forEach((un) => un());
  }, []);

  React.useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  // The picker wears the colour it is showing, so its own buttons and the
  // preview row are the widget in miniature.
  React.useEffect(() => {
    applyAccent(document.documentElement, isDefault ? null : hex, theme);
  }, [hex, theme, isDefault]);

  const frame = React.useRef(0);
  const preview = React.useCallback((next: string) => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => {
      invoke("preview_accent", { color: next }).catch(() => {});
    });
  }, []);

  const commit = React.useCallback((next: string | null) => {
    cancelAnimationFrame(frame.current);
    invoke("set_accent", { color: next }).catch(() => {});
  }, []);

  const choose = (next: Hsv, live: boolean) => {
    setHsv(next);
    const h = hsvToHex(next);
    setText(h);
    setInvalid(false);
    setIsDefault(false);
    if (live) preview(h);
    else commit(h);
  };

  const pick = (value: string) => {
    const h = normaliseHex(value);
    if (!h) {
      setInvalid(true);
      return;
    }
    setHsv(hexToHsv(h));
    setText(h);
    setInvalid(false);
    setIsDefault(false);
    commit(h);
  };

  const reset = () => {
    setHsv(hexToHsv(DEFAULT_ACCENT));
    setText(DEFAULT_ACCENT);
    setInvalid(false);
    setIsDefault(true);
    commit(null);
  };

  const close = () => {
    getCurrentWindow()
      .close()
      .catch(() => {});
  };

  const cancel = () => {
    if (original !== undefined) commit(original);
    close();
  };

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancel();
      if (e.key === "Enter" && !(e.target instanceof HTMLInputElement)) close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  return (
    <div className="td-picker">
      <div className="td-picker-title">Accent colour</div>

      <Drag2d
        className="td-sv"
        style={{ backgroundColor: hsvToHex({ h: hsv.h, s: 1, v: 1 }) }}
        x={hsv.s}
        y={1 - hsv.v}
        onMove={(x, y, done) => choose({ h: hsv.h, s: x, v: 1 - y }, !done)}
        label="Saturation and brightness"
      />

      <Drag2d
        className="td-hue"
        x={hsv.h / 360}
        y={0.5}
        onMove={(x, _y, done) => choose({ ...hsv, h: Math.min(359.9, x * 360) }, !done)}
        label="Hue"
      />

      <div className="td-picker-row">
        <span className="td-picker-swatch" style={{ background: hex }} />
        <input
          className={cn("td-hex", invalid && "td-hex-invalid")}
          value={text}
          spellCheck={false}
          maxLength={9}
          aria-label="Hex colour"
          onChange={(e) => {
            setText(e.target.value);
            setInvalid(false);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") pick(text);
          }}
          onBlur={() => {
            if (normaliseHex(text) !== hex) pick(text);
          }}
        />
        <button type="button" className="td-picker-link" onClick={reset} disabled={isDefault}>
          Default
        </button>
      </div>

      <div className="td-presets">
        {PRESETS.map((p) => (
          <button
            key={p}
            type="button"
            className={cn("td-preset", p === hex && !isDefault && "td-preset-on")}
            style={{ background: p }}
            title={p}
            aria-label={p}
            onClick={() => pick(p)}
          />
        ))}
      </div>

      <div className="td-picker-preview" aria-hidden>
        <span className="td-pill">4/4</span>
        <span className="td-picker-bar">
          <Bar value={62} transition={INSTANT} />
        </span>
        <span className="td-dots">
          <span className="td-dot">
            <span className="td-dot-lit" data-kind="accent" />
          </span>
          <span className="td-dot">
            <span className="td-dot-lit" data-kind="beat" />
          </span>
          <span className="td-dot" />
          <span className="td-dot" />
        </span>
      </div>

      <div className="td-picker-actions">
        <button type="button" className="td-button" onClick={cancel}>
          Cancel
        </button>
        <button type="button" className="td-button td-button-main" onClick={close}>
          Done
        </button>
      </div>
    </div>
  );
}

/**
 * A surface with a handle that follows the pointer while it is held. Reports
 * where the handle is as fractions of the width and height, and says when the
 * pointer lets go so the caller can store the value then rather than on every
 * move.
 */
function Drag2d({
  className,
  style,
  x,
  y,
  onMove,
  label,
}: {
  className: string;
  style?: React.CSSProperties;
  x: number;
  y: number;
  onMove: (x: number, y: number, done: boolean) => void;
  label: string;
}) {
  const ref = React.useRef<HTMLDivElement | null>(null);
  const held = React.useRef(false);

  const report = (e: React.PointerEvent, done: boolean) => {
    const r = ref.current?.getBoundingClientRect();
    if (!r || r.width <= 0 || r.height <= 0) return;
    const fx = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    const fy = Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
    onMove(fx, fy, done);
  };

  return (
    <div
      ref={ref}
      className={className}
      style={style}
      role="slider"
      aria-label={label}
      aria-valuenow={Math.round(x * 100)}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        held.current = true;
        e.currentTarget.setPointerCapture(e.pointerId);
        report(e, false);
      }}
      onPointerMove={(e) => {
        if (held.current) report(e, false);
      }}
      onPointerUp={(e) => {
        if (!held.current) return;
        held.current = false;
        e.currentTarget.releasePointerCapture(e.pointerId);
        report(e, true);
      }}
      onPointerCancel={() => {
        held.current = false;
      }}
    >
      <span className="td-handle" style={{ left: `${x * 100}%`, top: `${y * 100}%` }} />
    </div>
  );
}
