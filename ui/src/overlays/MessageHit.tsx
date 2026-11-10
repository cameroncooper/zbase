// `views::search::MessageHit`: author avatar, author · location · time, then
// the cleaned snippet with query matches and mentions marked. Shared by the
// quick switcher and the Search page at different scales.

import { useEffect, useRef, useState, type CSSProperties } from "react";
import { Avatar } from "../ui/Avatar";
import { Tooltip } from "../ui/Tooltip";
import type { HitView, SnippetRun } from "./types";

export type HitScale = {
  avatar: number;
  snippetSize: number;
  snippetLineHeight: number;
  snippetLines: number;
};

export const SEARCH_PAGE_HIT: HitScale = { avatar: 24, snippetSize: 14, snippetLineHeight: 21, snippetLines: 3 };
export const SWITCHER_HIT: HitScale = { avatar: 20, snippetSize: 13, snippetLineHeight: 18, snippetLines: 2 };

/** `MessageHit::avatar`: centered on the 18px header line. */
export function HitAvatar({ hit, scale }: { hit: HitView; scale: HitScale }) {
  return (
    <div style={{ flexShrink: 0, paddingTop: Math.max(0, (18 - scale.avatar) / 2), display: "flex" }}>
      <Avatar name={hit.author_name} asset={hit.author_avatar} size={scale.avatar} />
    </div>
  );
}

/** `MessageHit::column`; `onAccent` is the accent-filled selected row. */
export function HitColumn({
  hit,
  scale,
  onAccent,
  showLocation = true,
  children,
}: {
  hit: HitView;
  scale: HitScale;
  onAccent: boolean;
  showLocation?: boolean;
  children?: React.ReactNode;
}) {
  const primary = onAccent ? "var(--selected-fg)" : "var(--text-primary)";
  const secondary = onAccent ? "var(--selected-secondary)" : "var(--text-secondary)";
  const time = (
    <span className="hit-time" style={{ color: secondary }}>
      {hit.time_label}
    </span>
  );
  return (
    <div className="hit-column">
      <div className="hit-header">
        <span className="hit-author" style={{ color: primary }}>
          {hit.author_name}
        </span>
        <span className="hit-location" style={{ color: secondary }}>
          {showLocation ? hit.location : null}
        </span>
        {hit.full_time ? (
          <Tooltip text={hit.full_time} singleLine>
            {time}
          </Tooltip>
        ) : (
          time
        )}
      </div>
      <Snippet runs={hit.snippet} scale={scale} onAccent={onAccent} />
      {children}
    </div>
  );
}

/** `snippet_run_paint`. */
function runStyle(run: SnippetRun, onAccent: boolean): CSSProperties | undefined {
  if (!run.mention && !run.matched) return undefined;
  if (onAccent) {
    return { fontWeight: run.matched ? 700 : 600, textDecoration: run.matched ? "underline" : undefined };
  }
  if (run.matched) {
    return { fontWeight: 700, color: "var(--search-match-fg)", background: "var(--search-match-bg)" };
  }
  const color =
    run.mention === "user" ? "var(--mention)" : run.mention === "broadcast" ? "var(--warning)" : "var(--accent-strong)";
  return { fontWeight: 600, color };
}

/** `snippet_text`: clamped to `scale.snippetLines`. */
export function Snippet({ runs, scale, onAccent }: { runs: SnippetRun[]; scale: HitScale; onAccent: boolean }) {
  return (
    <div
      className="hit-snippet"
      style={{
        fontSize: scale.snippetSize,
        lineHeight: `${scale.snippetLineHeight}px`,
        color: onAccent ? "var(--selected-fg)" : "var(--text-primary)",
        WebkitLineClamp: scale.snippetLines,
      }}
    >
      {runs.map((run, index) => (
        <span key={index} style={runStyle(run, onAccent)}>
          {run.text}
        </span>
      ))}
    </div>
  );
}

/**
 * A text field whose value lives in a model: typing is local and sent as it
 * changes; a model value we did not send (the engine cleared it) replaces it.
 */
export function useModelInput(modelValue: string | undefined, onChange: (value: string) => void) {
  const [value, setValue] = useState(modelValue ?? "");
  // Values sent but not yet echoed back; their echoes are not external changes.
  const pending = useRef<string[]>([]);
  useEffect(() => {
    if (modelValue === undefined) return;
    const echoed = pending.current.lastIndexOf(modelValue);
    if (echoed >= 0) {
      pending.current = pending.current.slice(echoed + 1);
      return;
    }
    pending.current = [];
    setValue(modelValue);
  }, [modelValue]);
  return {
    value,
    onChange: (event: React.ChangeEvent<HTMLInputElement>) => {
      const next = event.target.value;
      setValue(next);
      pending.current.push(next);
      onChange(next);
    },
  };
}

/** Asks the composer to take focus back, like `AppWindow::restore_chat_focus`. */
export function restoreChatFocus() {
  window.dispatchEvent(new CustomEvent("zbase:restore-chat-focus"));
}

/** Which composer has focus: `data-composer="conversation" | "thread"` on its container. */
export function focusedComposer(): "conversation" | "thread" | null {
  const host = document.activeElement?.closest?.("[data-composer]") as HTMLElement | null;
  const kind = host?.dataset.composer;
  return kind === "conversation" || kind === "thread" ? kind : null;
}
