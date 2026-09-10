/**
 * One chosen colour, turned into the four value palette every accent in the
 * widget uses, plus the text colour for pills.
 *
 * Any colour is allowed, white and black included. The chosen colour itself
 * is the main tone in both themes, exactly as picked, because what you choose
 * is what you should see. Everything else is derived in OKLCH, where "lighter"
 * and "darker" mean the same thing for every hue: the dark end, the bright
 * core, a translucent glow, and an ink for text on an accent tinted pill that
 * is moved as far as it has to be to stay readable.
 */

export const DEFAULT_ACCENT = "#3fbfae";

export interface Palette {
  from: string;
  to: string;
  core: string;
  glow: string;
  ink: string;
}

export type Theme = "dark" | "light";

/** The opaque surfaces the widget is drawn on, from styles.css. */
export const SURFACE: Record<Theme, string> = { dark: "#1b1b1b", light: "#f7f7f5" };

/** How much of the accent tints a pill's background, from styles.css. */
const PILL_TINT = 0.18;

/** WCAG AA for body text, with a little room for rounding. */
const INK_CONTRAST = 4.6;

type Rgb = [number, number, number];

/** `#rgb` or `#rrggbb`, with or without the hash. Lowercase `#rrggbb` back. */
export function normaliseHex(text: string): string | null {
  const hex = text.trim().replace(/^#/, "");
  if (!/^[0-9a-fA-F]+$/.test(hex)) return null;
  if (hex.length === 3) {
    return `#${hex
      .split("")
      .map((c) => c + c)
      .join("")
      .toLowerCase()}`;
  }
  if (hex.length === 6) return `#${hex.toLowerCase()}`;
  return null;
}

export function hexToRgb(hex: string): Rgb {
  const h = normaliseHex(hex) ?? DEFAULT_ACCENT;
  return [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb;
}

export function rgbToHex([r, g, b]: Rgb): string {
  const byte = (v: number) =>
    Math.round(Math.min(255, Math.max(0, v)))
      .toString(16)
      .padStart(2, "0");
  return `#${byte(r)}${byte(g)}${byte(b)}`;
}

const toLinear = (c: number) => {
  const s = c / 255;
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
};
const fromLinear = (l: number) => {
  const s = l <= 0.0031308 ? 12.92 * l : 1.055 * l ** (1 / 2.4) - 0.055;
  return s * 255;
};

/** Relative luminance, as WCAG defines it. */
export function luminance(rgb: Rgb): number {
  const [r, g, b] = rgb.map(toLinear);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrast(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** `amount` of `a` over `b`, in sRGB, the way `color-mix(in srgb, ...)` does. */
export function mix(a: Rgb, b: Rgb, amount: number): Rgb {
  return [0, 1, 2].map((i) => a[i] * amount + b[i] * (1 - amount)) as Rgb;
}

interface Lch {
  l: number;
  c: number;
  h: number;
}

function toOklch(rgb: Rgb): Lch {
  const [r, g, b] = rgb.map(toLinear);
  const l_ = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m_ = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s_ = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l_ + 0.793617785 * m_ - 0.0040720468 * s_;
  const A = 1.9779984951 * l_ - 2.428592205 * m_ + 0.4505937099 * s_;
  const B = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.808675766 * s_;
  return { l: L, c: Math.hypot(A, B), h: Math.atan2(B, A) };
}

function fromOklch({ l, c, h }: Lch): Rgb {
  const A = c * Math.cos(h);
  const B = c * Math.sin(h);
  const l_ = (l + 0.3963377774 * A + 0.2158037573 * B) ** 3;
  const m_ = (l - 0.1055613458 * A - 0.0638541728 * B) ** 3;
  const s_ = (l - 0.0894841775 * A - 1.291485548 * B) ** 3;
  return [
    4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
    -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
    -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_,
  ].map(fromLinear) as Rgb;
}

/** Brings an OKLCH colour inside sRGB by giving up chroma, never lightness. */
function inGamut(lch: Lch): Rgb {
  let c = lch.c;
  for (let i = 0; i < 24; i++) {
    const rgb = fromOklch({ ...lch, c });
    if (rgb.every((v) => v >= -0.5 && v <= 255.5)) return rgb;
    c *= 0.85;
  }
  return fromOklch({ ...lch, c: 0 });
}

/** The pill background an ink has to read against. */
export function pillBackground(accent: string, theme: Theme): Rgb {
  return mix(hexToRgb(accent), hexToRgb(SURFACE[theme]), PILL_TINT);
}

/**
 * Text for a pill: the accent itself when it already reads, otherwise the
 * accent moved towards white on the dark theme or towards black on the light
 * one until it does. The hue stays, so the pill still says which colour was
 * chosen.
 */
function ink(accent: string, theme: Theme): Rgb {
  const bg = pillBackground(accent, theme);
  const base = toOklch(hexToRgb(accent));
  const step = theme === "dark" ? 0.02 : -0.02;
  let l = base.l;
  for (let i = 0; i < 60; i++) {
    const candidate = inGamut({ ...base, l });
    if (contrast(candidate, bg) >= INK_CONTRAST) return candidate;
    l = Math.min(1, Math.max(0, l + step));
  }
  return theme === "dark" ? [255, 255, 255] : [0, 0, 0];
}

export function palette(accent: string, theme: Theme): Palette {
  const hex = normaliseHex(accent) ?? DEFAULT_ACCENT;
  const base = toOklch(hexToRgb(hex));
  const from = inGamut({ ...base, l: base.l * 0.8 });
  const core = inGamut({ l: base.l + (1 - base.l) * 0.55, c: base.c * 0.55, h: base.h });
  // The same alphas the built in palette uses: a quarter on dark, less on
  // light where a glow reads as a smudge.
  const glow = `${hex}${theme === "dark" ? "40" : "26"}`;
  return { from: rgbToHex(from), to: hex, core: rgbToHex(core), glow, ink: rgbToHex(ink(hex, theme)) };
}

/** The CSS custom properties a palette is applied through. */
export const ACCENT_PROPERTIES = ["--takt-from", "--takt-to", "--takt-core", "--takt-glow", "--takt-ink"] as const;

/**
 * Sets the palette on an element, or clears it so the stylesheet's hand tuned
 * turquoise shows through again.
 */
export function applyAccent(el: HTMLElement, accent: string | null, theme: Theme) {
  if (!accent) {
    for (const p of ACCENT_PROPERTIES) el.style.removeProperty(p);
    return;
  }
  const p = palette(accent, theme);
  el.style.setProperty("--takt-from", p.from);
  el.style.setProperty("--takt-to", p.to);
  el.style.setProperty("--takt-core", p.core);
  el.style.setProperty("--takt-glow", p.glow);
  el.style.setProperty("--takt-ink", p.ink);
}

export interface Hsv {
  /** 0 to 360. */
  h: number;
  /** 0 to 1. */
  s: number;
  /** 0 to 1. */
  v: number;
}

export function hsvToHex({ h, s, v }: Hsv): string {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return rgbToHex([f(5) * 255, f(3) * 255, f(1) * 255]);
}

export function hexToHsv(hex: string): Hsv {
  const [r, g, b] = hexToRgb(hex).map((v) => v / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  let h = 0;
  if (d > 0) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  return { h, s: max === 0 ? 0 : d / max, v: max };
}

/** A starting set for the picker; anything else is one drag away. */
export const PRESETS = [
  "#3fbfae",
  "#58a6ff",
  "#9b7ef0",
  "#f06fa8",
  "#ef5350",
  "#f28c38",
  "#f5c84e",
  "#7cc86d",
  "#e8e6e1",
  "#8a8a8a",
] as const;
