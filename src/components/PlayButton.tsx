import { motion } from "framer-motion";

import { SPRING } from "@/components/Bar";

interface Props {
  running: boolean;
  /** Glow strength while playing: full on the downbeat, softer between. */
  glow: number;
  motionOn: boolean;
  onToggle: () => void;
}

/** Play and stop, drawn in the ten pixel mark slot every row starts with. */
export function PlayButton({ running, glow, motionOn, onToggle }: Props) {
  const shadow = running
    ? `drop-shadow(0 0 3px color-mix(in srgb, var(--takt-glow) ${Math.round(glow * 100)}%, transparent))`
    : "none";
  return (
    <motion.button
      type="button"
      className="td-play"
      data-running={running}
      aria-label={running ? "Stop" : "Play"}
      title={running ? "Stop" : "Play"}
      onClick={onToggle}
      whileTap={motionOn ? { scale: 0.9 } : undefined}
      transition={SPRING}
    >
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden style={{ filter: shadow }}>
        {running ? (
          <rect x="1.75" y="1.75" width="6.5" height="6.5" rx="1" />
        ) : (
          <path d="M2.2 1.3 Q2.2 0.6 2.8 0.95 L8.9 4.55 Q9.5 5 8.9 5.45 L2.8 9.05 Q2.2 9.4 2.2 8.7 Z" />
        )}
      </svg>
    </motion.button>
  );
}
