import * as React from "react";

import { Bar, INSTANT, SPRING } from "@/components/Bar";
import { bpmToFill, fillToBpm } from "@/lib/utils";

interface Props {
  bpm: number;
  motionOn: boolean;
  /** Called with every whole tempo the drag passes through. */
  onSet: (bpm: number) => void;
}

/**
 * The tempo as a bar that can be dragged. While the pointer is down the fill
 * follows it directly; a spring would trail behind the hand. Otherwise it
 * eases to whatever tempo the wheel, a preset or a tap arrived at.
 */
export function TempoSlider({ bpm, motionOn, onSet }: Props) {
  const ref = React.useRef<HTMLDivElement | null>(null);
  const [dragBpm, setDragBpm] = React.useState<number | null>(null);
  const last = React.useRef<number | null>(null);

  const at = (clientX: number) => {
    const rect = ref.current?.getBoundingClientRect();
    if (!rect || rect.width <= 0) return null;
    return fillToBpm((clientX - rect.left) / rect.width);
  };

  const move = (clientX: number) => {
    const next = at(clientX);
    if (next === null) return;
    setDragBpm(next);
    if (next !== last.current) {
      last.current = next;
      onSet(next);
    }
  };

  const shown = dragBpm ?? bpm;

  return (
    <div
      ref={ref}
      className="td-slot td-slider"
      title={`Tempo ${shown} bpm. Drag, or scroll anywhere on the widget (Shift for steps of five).`}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        last.current = bpm;
        move(e.clientX);
      }}
      onPointerMove={(e) => {
        if (dragBpm !== null) move(e.clientX);
      }}
      onPointerUp={(e) => {
        e.currentTarget.releasePointerCapture(e.pointerId);
        setDragBpm(null);
      }}
      onPointerCancel={() => setDragBpm(null)}
    >
      <Bar
        value={bpmToFill(shown)}
        transition={dragBpm !== null || !motionOn ? INSTANT : SPRING}
      />
    </div>
  );
}
