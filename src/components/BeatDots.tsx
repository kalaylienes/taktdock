import { motion } from "framer-motion";

export interface LitBeat {
  beat: number;
  kind: "accent" | "beat" | "sub";
  /** Changes on every click, so a repeated beat index still flashes again. */
  seq: number;
  /** How long one beat lasts at the tempo it was played at. */
  beatMs: number;
}

interface Props {
  beats: number;
  lit: LitBeat | null;
  motionOn: boolean;
}

/**
 * One dot per beat in the bar. The beat that just sounded lights up and fades
 * over most of its length; the downbeat lights in the brighter core tone.
 * With motion off the dot simply holds its colour until the next beat.
 */
export function BeatDots({ beats, lit, motionOn }: Props) {
  const count = Math.max(1, Math.min(16, beats));
  return (
    <div className="td-dots" role="img" aria-label={lit ? `beat ${lit.beat + 1} of ${count}` : `${count} beats`}>
      {Array.from({ length: count }, (_, i) => (
        <span key={i} className="td-dot" data-beat={i}>
          {lit && lit.beat === i && (
            <motion.span
              key={lit.seq}
              className="td-dot-lit"
              data-kind={lit.kind}
              initial={{ opacity: 1 }}
              animate={{ opacity: motionOn ? 0.35 : 1 }}
              transition={
                motionOn
                  ? { duration: (lit.beatMs * 0.6) / 1000, ease: "easeOut" }
                  : { duration: 0 }
              }
            />
          )}
        </span>
      ))}
    </div>
  );
}
