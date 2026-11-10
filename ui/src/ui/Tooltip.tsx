// `TextTooltip` (src/views/tooltip.rs): a plain-text hover tooltip shown after
// gpui's hover delay at the pointer, removed on pointer exit.

import { useEffect, useRef, useState, type ReactElement, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { IS_MAC } from "../core/shortcuts";

/** gpui's `TOOLTIP_SHOW_DELAY`. */
const SHOW_DELAY_MS = 500;
/** gpui places tooltips just below and right of the pointer. */
const POINTER_OFFSET = { x: 0, y: 20 };

type Props = {
  text: string;
  /** Secondary shortcut, e.g. "⇧⌘M"; makes the tooltip one line like `with_shortcut`. */
  shortcut?: string;
  /** `single_line`; otherwise long labels wrap at 360px (`builder`). */
  singleLine?: boolean;
  children: ReactNode;
};

export function Tooltip({ text, shortcut, singleLine = shortcut !== undefined, children }: Props) {
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const pointer = useRef({ x: 0, y: 0 });
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const hide = () => {
    window.clearTimeout(timer.current);
    setPosition(null);
  };
  return (
    <span
      style={{ display: "contents" }}
      onPointerEnter={(event) => {
        pointer.current = { x: event.clientX, y: event.clientY };
        window.clearTimeout(timer.current);
        timer.current = window.setTimeout(() => setPosition({ ...pointer.current }), SHOW_DELAY_MS);
      }}
      onPointerMove={(event) => {
        pointer.current = { x: event.clientX, y: event.clientY };
      }}
      onPointerLeave={hide}
      onPointerDown={hide}
    >
      {children}
      {position && createPortal(<TooltipBubble text={text} shortcut={shortcut} singleLine={singleLine} at={position} />, document.body)}
    </span>
  );
}

function TooltipBubble({
  text,
  shortcut,
  singleLine,
  at,
}: {
  text: string;
  shortcut?: string;
  singleLine: boolean;
  at: { x: number; y: number };
}): ReactElement {
  const ref = useRef<HTMLDivElement>(null);
  const [shift, setShift] = useState({ x: 0, y: 0 });
  useEffect(() => {
    const rect = ref.current?.getBoundingClientRect();
    if (!rect) return;
    setShift({
      x: Math.min(0, window.innerWidth - 4 - rect.right),
      y: rect.bottom > window.innerHeight - 4 ? -(rect.height + POINTER_OFFSET.y + 4) : 0,
    });
  }, []);
  return (
    <div
      ref={ref}
      role="tooltip"
      style={{
        position: "fixed",
        left: at.x + POINTER_OFFSET.x + shift.x,
        top: at.y + POINTER_OFFSET.y + shift.y,
        zIndex: 10000,
        maxWidth: 360,
        padding: "4px 8px",
        borderRadius: 6,
        background: "var(--panel-bg)",
        border: "1px solid var(--shell-border-strong)",
        fontSize: 12,
        lineHeight: "16px",
        color: "var(--text-primary)",
        display: "flex",
        alignItems: "center",
        gap: 8,
        whiteSpace: singleLine ? "nowrap" : "normal",
        pointerEvents: "none",
      }}
    >
      {text}
      {shortcut && <span style={{ color: "var(--text-secondary)" }}>{shortcut}</span>}
    </div>
  );
}

/**
 * `tooltip::shortcut_label`: "⇧⌘D" on macOS, "Ctrl+Shift+D" elsewhere; other
 * text passes through.
 */
export function shortcutGlyphs(keys: string): string {
  if (IS_MAC) return keys;
  let label = "";
  for (const [symbol, name] of [
    ["⌘", "Ctrl+"],
    ["⌥", "Alt+"],
    ["⇧", "Shift+"],
  ] as const) {
    if (keys.includes(symbol)) label += name;
  }
  return label + keys.replace(/[⌘⌥⇧]/g, "");
}
