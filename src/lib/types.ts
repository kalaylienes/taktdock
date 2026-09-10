/** How the widget should look. Sent by the backend as the `config` event. */
export interface AppearanceConfig {
  theme: string;
  resolved_theme: "dark" | "light" | string;
  animations: boolean;
  compact: boolean;
  expanded: boolean;
  /** "float" or "taskbar". */
  placement: string;
  /** Visibility, fullscreen and power state, decided on the Rust side. */
  motion_allowed: boolean;
  /** `#rrggbb`, or null for the built in turquoise. */
  accent: string | null;
}

/** What the metronome is doing. Sent as the `transport` event. */
export interface Transport {
  running: boolean;
  bpm: number;
  beats_per_bar: number;
  beat_unit: number;
  subdivision: number;
  accent_first: boolean;
  sound: string;
  volume: number;
  /** The device chosen in the settings; null follows the system default. */
  output_device: string | null;
  /** The device actually playing, which differs while falling back. */
  active_device: string | null;
  device_problem: string | null;
}

/** One click, sent as the `beat` event. */
export interface BeatEvent {
  bar: number;
  beat: number;
  sub: number;
  kind: "accent" | "beat" | "sub";
  beats_per_bar: number;
  subdivision: number;
  bpm: number;
  /** When the click is heard, in milliseconds since the Unix epoch. */
  at_ms: number;
}
