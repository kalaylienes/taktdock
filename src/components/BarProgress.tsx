import { Bar, INSTANT } from "@/components/Bar";

interface Props {
  running: boolean;
  /** Increases with every bar, so a new bar restarts the sweep. */
  bar: number;
  beat: number;
  beats: number;
  /** Length of one bar at the tempo it is played at. */
  barMs: number;
  /** Glow strength: full on the downbeat, softer between. */
  glow: number;
  motionOn: boolean;
}

/**
 * How far through the bar the music is. It sweeps linearly across the bar and
 * snaps back to empty on the downbeat, rather than draining back, because the
 * downbeat is an event and not a direction. With motion off it steps once per
 * beat instead of sweeping.
 */
export function BarProgress({ running, bar, beat, beats, barMs, glow, motionOn }: Props) {
  if (!running) {
    return <Bar value={0} transition={INSTANT} muted />;
  }
  if (!motionOn) {
    return (
      <Bar value={((beat + 1) / Math.max(1, beats)) * 100} transition={INSTANT} glow={glow} fitGradient={false} />
    );
  }
  return (
    <Bar
      key={bar}
      from={0}
      value={100}
      transition={{ duration: barMs / 1000, ease: "linear" }}
      glow={glow}
      fitGradient={false}
    />
  );
}
