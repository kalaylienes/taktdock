/** Taps further apart than this start a new tempo rather than extend one. */
export const TAP_RESET_MS = 2000;

/** How many of the latest intervals are averaged. */
export const TAP_WINDOW = 4;

/**
 * Tap tempo. Every tap after the first gives a tempo, the average of up to the
 * last four intervals, so the reading settles as you keep tapping and one late
 * tap does not throw it far off. A pause of two seconds starts over.
 */
export class TapTempo {
  private taps: number[] = [];

  /** Records a tap and returns the tempo it implies, if it implies one yet. */
  tap(now: number): number | null {
    const last = this.taps[this.taps.length - 1];
    if (last !== undefined && now - last > TAP_RESET_MS) this.taps = [];
    this.taps.push(now);
    if (this.taps.length > TAP_WINDOW + 1) this.taps.shift();
    if (this.taps.length < 2) return null;

    const intervals: number[] = [];
    for (let i = 1; i < this.taps.length; i++) intervals.push(this.taps[i] - this.taps[i - 1]);
    const mean = intervals.reduce((a, b) => a + b, 0) / intervals.length;
    if (mean <= 0) return null;
    return Math.round(60_000 / mean);
  }

  reset() {
    this.taps = [];
  }
}
