// `ConversationView::render_find_bar` (src/views/conversation/mod.rs): the
// Cmd+F bar under the conversation header. The timeline area places it; the
// find logic is the overlays engine's.

import { useEffect, useRef } from "react";
import { send, useModel } from "../core/engine";
import { Icon } from "../ui/Icon";
import { Tooltip } from "../ui/Tooltip";
import { useModelInput } from "./MessageHit";
import type { FindInChatModel } from "./types";

/** Rendered while `find_in_chat.open`; returns null otherwise. */
export function FindBar() {
  const find = useModel("find_in_chat") as FindInChatModel | undefined;
  if (!find?.open) return null;
  return <FindBarOpen find={find} />;
}

function FindBarOpen({ find }: { find: FindInChatModel }) {
  const input = useModelInput(find.query, (query) => send("overlays", "set_find_query", { query }));
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => inputRef.current?.focus(), []);

  const count = find.results.length;
  const canNavigate = count > 0;
  const status =
    find.query.trim() === ""
      ? "Type to search this conversation"
      : find.loading
        ? "Searching…"
        : count === 0
          ? "0 results"
          : find.selected_index !== null
            ? `${find.selected_index + 1} of ${count}`
            : `${count} result${count === 1 ? "" : "s"}`;

  const navButton = (previous: boolean) => (
    <Tooltip
      text={previous ? "Previous match" : "Next match"}
      shortcut={previous ? "Shift+Enter" : "Enter"}
      key={previous ? "prev" : "next"}
    >
      <div
        className={`fb-button${canNavigate ? " fb-enabled" : ""}`}
        style={{ opacity: canNavigate ? 1 : 0.35 }}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => {
          if (!canNavigate) return;
          send("overlays", previous ? "find_previous" : "find_next");
          inputRef.current?.focus();
        }}
      >
        <Icon name={previous ? "chevron-up" : "chevron-down"} size={14} color="var(--text-secondary)" />
      </div>
    </Tooltip>
  );

  return (
    <div className="fb-bar">
      <Icon name="search" size={16} color="var(--accent)" />
      <div style={{ flex: 1, minWidth: 0, display: "flex" }} onClick={() => inputRef.current?.focus()}>
        <input
          placeholder="Find in this conversation"
          ref={inputRef}
          className="ov-input"
          spellCheck={false}
          autoComplete="off"
          data-overlay-input="find-in-chat"
          onKeyDown={(event) => {
            // FindInChatTextField: Return / Shift+Return walk the matches.
            if (event.key === "Enter" && !event.nativeEvent.isComposing) {
              event.preventDefault();
              event.stopPropagation();
              send("overlays", event.shiftKey ? "find_previous" : "find_next");
            }
          }}
          {...input}
        />
      </div>
      <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>{status}</div>
      {navButton(true)}
      {navButton(false)}
      <div className="fb-button fb-enabled" onClick={() => send("overlays", "close_find_in_chat")}>
        <Icon name="x" size={14} color="var(--text-secondary)" />
      </div>
    </div>
  );
}
