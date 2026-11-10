// views/popup_menu.rs: a small macOS-style menu that pops up from a control.
// The owner keeps the open state and places it; a press outside dismisses it.

import { useEffect, useRef, type CSSProperties } from "react";
import { Icon } from "./Icon";

export type PopupMenuEntry =
  | { label: string; checked?: boolean; shortcut?: string; onClick: () => void }
  | "separator";

export function PopupMenu({
  entries,
  onDismiss,
  style,
}: {
  entries: PopupMenuEntry[];
  /** A press anywhere outside the menu. */
  onDismiss: (event: MouseEvent) => void;
  style?: CSSProperties;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  useEffect(() => {
    const onDown = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) dismiss.current(event);
    };
    window.addEventListener("mousedown", onDown, true);
    return () => window.removeEventListener("mousedown", onDown, true);
  }, []);
  const hasChecks = entries.some((entry) => entry !== "separator" && entry.checked !== undefined);
  return (
    <div ref={ref} className={`popup-menu${hasChecks ? " with-checks" : ""}`} style={style} role="menu">
      {entries.map((entry, index) =>
        entry === "separator" ? (
          <div key={index} className="popup-menu-separator" />
        ) : (
          <button key={index} className="popup-menu-item" role="menuitem" onClick={entry.onClick}>
            {hasChecks && (
              <span className="popup-menu-check">
                {entry.checked && <Icon name="check" size={12} />}
              </span>
            )}
            <span style={{ flex: 1 }}>{entry.label}</span>
            {entry.shortcut && <span className="popup-menu-shortcut">{entry.shortcut}</span>}
          </button>
        ),
      )}
    </div>
  );
}
