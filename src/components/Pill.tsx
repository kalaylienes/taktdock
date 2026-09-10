import { invoke } from "@tauri-apps/api/core";

interface Props {
  label: string;
  title: string;
  /** Which list a right click opens: "meter" or "subdivision". */
  menu: "meter" | "subdivision";
  onCycle: () => void;
}

/** A value that steps to the next one on click and lists them all on right click. */
export function Pill({ label, title, menu, onCycle }: Props) {
  return (
    <span
      className="td-pill"
      data-pill={menu}
      title={title}
      role="button"
      onClick={onCycle}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        invoke("show_pill_menu", { pill: menu }).catch(() => {});
      }}
    >
      {label}
    </span>
  );
}
