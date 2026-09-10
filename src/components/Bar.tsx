import * as React from "react";
import { motion, type Transition } from "framer-motion";

export const SPRING: Transition = { type: "spring", stiffness: 120, damping: 20 };
export const INSTANT: Transition = { duration: 0 };

// A five stop gradient squeezed into a sliver leaves one bright dot, so the
// core fades in with the fill instead of appearing all at once.
const BAND_FADE_START = 15;
const BAND_FADE_END = 40;

export interface BarProps {
  /** Fill percentage, 0 to 100. */
  value: number;
  /** How the fill reaches `value`. */
  transition: Transition;
  /** Where the fill starts when the bar mounts, for a bar that sweeps. */
  from?: number;
  /** Glow strength, 0 to 1. */
  glow?: number;
  /** A dimmer gradient for a control that is not doing anything. */
  muted?: boolean;
  /**
   * Stretch the gradient over the fill rather than the track. Right for a
   * value that sits still, so the bright core is always in the middle of what
   * is lit; wrong for a sweep, which should uncover a gradient that stays put.
   */
  fitGradient?: boolean;
}

/**
 * The track and fill every bar in the widget is drawn with.
 *
 * The fill is two opposing translations rather than an animated width: the
 * outer layer slides left and carries an undistorted pill cap, the inner layer
 * slides back by the same amount so the gradient stays anchored to the track.
 * Both stay on the compositor. There is no sheen and no red past ninety
 * percent: nothing about a tempo is a warning.
 */
export function Bar({ value, transition, from, glow = 1, muted, fitGradient = true }: BarProps) {
  const p = clamp(value);
  const shift = 100 - p;
  const startShift = from === undefined ? undefined : 100 - clamp(from);

  const bandStrength = Math.min(
    1,
    Math.max(0, (p - BAND_FADE_START) / (BAND_FADE_END - BAND_FADE_START)),
  );
  const litCore = `color-mix(in oklab, var(--takt-core) ${Math.round(bandStrength * 100)}%, var(--takt-to))`;
  const gradient = muted
    ? "linear-gradient(90deg, var(--text-secondary), var(--text-secondary))"
    : `linear-gradient(90deg, var(--takt-from) 0%, var(--takt-to) 35%, ${litCore} 55%, var(--takt-to) 78%, var(--takt-from) 100%)`;

  return (
    <div className="td-track-wrap">
      {/* Kept outside the track so the track's overflow does not clip it. */}
      <motion.div
        aria-hidden
        className="td-glow"
        style={{
          boxShadow: muted ? "none" : "0 0 3px 0 var(--takt-glow)",
          transformOrigin: "left center",
        }}
        initial={startShift === undefined ? false : { scaleX: (100 - startShift) / 100 }}
        animate={{ scaleX: p / 100, opacity: glow }}
        transition={{ scaleX: transition, opacity: INSTANT }}
      />
      <div className="td-track">
        <motion.div
          className="td-clip"
          initial={startShift === undefined ? false : { x: `-${startShift}%` }}
          animate={{ x: `-${shift}%` }}
          transition={transition}
        >
          <motion.div
            className="td-fill"
            style={{
              backgroundImage: gradient,
              backgroundSize: fitGradient ? `${Math.max(p, 0.001)}% 100%` : "100% 100%",
              backgroundRepeat: "no-repeat",
              boxShadow: muted
                ? "none"
                : "inset 0 0.45px 0 rgba(255,255,255,0.5), inset 0 -0.6px 1px rgba(0,0,0,0.28)",
            }}
            initial={startShift === undefined ? false : { x: `${startShift}%` }}
            animate={{ x: `${shift}%` }}
            transition={transition}
          />
        </motion.div>
      </div>
    </div>
  );
}

function clamp(v: number): number {
  return Math.min(100, Math.max(0, Number.isFinite(v) ? v : 0));
}

export default React.memo(Bar);
