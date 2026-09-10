import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export const MIN_BPM = 30;
export const MAX_BPM = 300;

export function clampBpm(bpm: number): number {
  return Math.min(MAX_BPM, Math.max(MIN_BPM, Math.round(bpm)));
}

/** Slider fill for a tempo, 0 to 100. */
export function bpmToFill(bpm: number): number {
  return ((clampBpm(bpm) - MIN_BPM) / (MAX_BPM - MIN_BPM)) * 100;
}

/** Tempo for a point along the slider, where 0 is the left edge and 1 the right. */
export function fillToBpm(fraction: number): number {
  const f = Math.min(1, Math.max(0, fraction));
  return clampBpm(MIN_BPM + f * (MAX_BPM - MIN_BPM));
}

/** One beat, in milliseconds. The tempo counts the beat unit, so 6/8 at 120
 *  is 120 eighths a minute. */
export function beatMs(bpm: number): number {
  return 60_000 / Math.max(1, bpm);
}

/**
 * The grid as a note value. The clicks between beats are named against the
 * beat unit, so eighths in 4/4 read 1/8 and in 6/8 read 1/16.
 */
export function subdivisionLabel(subdivision: number, beatUnit: number): string {
  const unit = beatUnit === 8 ? 8 : 4;
  switch (subdivision) {
    case 2:
      return `1/${unit * 2}`;
    case 3:
      return `1/${unit * 2}T`;
    case 4:
      return `1/${unit * 4}`;
    default:
      return `1/${unit}`;
  }
}

export function subdivisionName(subdivision: number): string {
  switch (subdivision) {
    case 2:
      return "eighths";
    case 3:
      return "triplets";
    case 4:
      return "sixteenths";
    default:
      return "none";
  }
}
