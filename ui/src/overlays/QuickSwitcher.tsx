// `OverlayHost::render_search_overlay` (src/views/overlays/mod.rs): the Cmd+K
// switcher with Unread / Recent sections, or Channels and people / Messages.

import { useEffect, useRef } from "react";
import { send, useModel } from "../core/engine";
import { Avatar, ParticipantsAvatar } from "../ui/Avatar";
import { EmptyState, Keycap, SectionLabel } from "../ui/common";
import { Icon } from "../ui/Icon";
import { shortcutGlyphs } from "../ui/Tooltip";
import { HitAvatar, HitColumn, SWITCHER_HIT, useModelInput } from "./MessageHit";
import type { QuickSwitcherModel, QuickSwitcherResult, Segment, SwitcherRowView } from "./types";

const RENDER_LIMIT_UNREAD = 14;
const RENDER_LIMIT_RECENT = 14;
const RECENT_SHORTCUT_LIMIT = 5;
const RENDER_LIMIT_CONVERSATIONS = 3;
const RENDER_LIMIT_MESSAGES = 10;

export function QuickSwitcher() {
  const switcher = useModel("quick_switcher") as QuickSwitcherModel | undefined;
  const extra = useModel("overlays_extra");
  const input = useModelInput(switcher?.query, (query) =>
    send("overlays", "set_quick_switcher_query", { query }),
  );
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => inputRef.current?.focus(), []);
  const selectedRef = useRef<HTMLDivElement>(null);
  useEffect(() => selectedRef.current?.scrollIntoView({ block: "nearest" }), [switcher?.selected_index]);
  if (!switcher) return null;

  const rows = extra?.quick_switcher_rows ?? [];
  const queryEmpty = switcher.query.trim() === "";
  const unread: number[] = [];
  const recent: number[] = [];
  const conversations: number[] = [];
  const messages: number[] = [];
  switcher.results.forEach((result, index) => {
    if (queryEmpty) {
      if (result.kind === "UnreadChannel") {
        if (unread.length < RENDER_LIMIT_UNREAD) unread.push(index);
      } else if (recent.length < RENDER_LIMIT_RECENT) {
        recent.push(index);
      }
    } else if (result.kind === "Message") {
      if (messages.length < RENDER_LIMIT_MESSAGES) messages.push(index);
    } else if (conversations.length < RENDER_LIMIT_CONVERSATIONS) {
      conversations.push(index);
    }
  });

  const section = (title: string, indices: number[], showShortcuts: boolean) =>
    indices.length > 0 && (
      <div className="qs-section">
        <SectionLabel style={{ padding: "0 8px 4px" }}>{title}</SectionLabel>
        {indices.map((index, displayIndex) => {
          const result = switcher.results[index];
          const view = rows[index];
          const selected = switcher.selected_index === index;
          const hint = showShortcuts && displayIndex < RECENT_SHORTCUT_LIMIT ? shortcutGlyphs(`⌘${displayIndex + 1}`) : null;
          return (
            <SwitcherRow
              key={index}
              index={index}
              result={result}
              view={view}
              selected={selected}
              hint={hint}
              rowRef={selected ? selectedRef : undefined}
            />
          );
        })}
      </div>
    );

  return (
    <div className="ov-dialog" onClick={(event) => event.stopPropagation()}>
      <div className="qs-header">
        <Icon name="search" size={16} color="var(--text-secondary)" />
        <input
          placeholder="Jump to channel, DM, or message"
          ref={inputRef}
          className="ov-input"
          spellCheck={false}
          autoComplete="off"
          data-overlay-input="quick-switcher"
          {...input}
        />
      </div>
      <div className="ov-scroll">
        <div className="qs-results">
          {queryEmpty ? (
            <>
              {section("Unread", unread, false)}
              {section("Recent", recent, true)}
            </>
          ) : (
            <>
              {section("Channels and people", conversations, false)}
              {section("Messages", messages, false)}
            </>
          )}
          {switcher.results.length === 0 &&
            !switcher.loading_messages &&
            (queryEmpty ? (
              <EmptyState
                icon="dm"
                title="No conversations yet"
                body="Channels and people you talk to will show up here."
              />
            ) : (
              <EmptyState
                icon="search"
                title={`No matches for “${switcher.query.trim()}”`}
                body="Try a channel, a person, or words from a message."
              />
            ))}
          {switcher.loading_messages && !queryEmpty && <div className="qs-status">Searching messages…</div>}
          {extra?.quick_switcher_indexing_status && !queryEmpty && (
            <div className="qs-status">{extra.quick_switcher_indexing_status}</div>
          )}
        </div>
      </div>
      <div className="qs-footer">
        <Hint keys={["↑", "↓"]} label="Navigate" />
        <Hint keys={["↵"]} label="Open" />
        <Hint keys={["esc"]} label="Close" />
        <div style={{ flex: 1 }} />
        <Hint keys={[shortcutGlyphs("⌘K")]} label="Toggle" />
      </div>
    </div>
  );
}

function Hint({ keys, label }: { keys: string[]; label: string }) {
  return (
    <div className="qs-hint">
      {keys.map((key) => (
        <Keycap key={key}>{key}</Keycap>
      ))}
      <div style={{ paddingLeft: 2 }}>{label}</div>
    </div>
  );
}

function Segments({ segments, color, weight }: { segments: Segment[]; color: string; weight: number }) {
  return (
    <>
      {segments.map((segment, index) =>
        segment.matched ? (
          <span key={index} className="qs-match">
            {segment.text}
          </span>
        ) : (
          <span key={index} style={{ color, fontWeight: weight }}>
            {segment.text}
          </span>
        ),
      )}
    </>
  );
}

function SwitcherRow({
  index,
  result,
  view,
  selected,
  hint,
  rowRef,
}: {
  index: number;
  result: QuickSwitcherResult;
  view: SwitcherRowView | undefined;
  selected: boolean;
  hint: string | null;
  rowRef?: React.Ref<HTMLDivElement>;
}) {
  const open = () => send("overlays", "open_quick_switcher_result", { index });
  const surface = selected ? "var(--switcher-selection)" : "var(--panel-bg)";
  if (view?.hit) {
    // `quick_switcher_message_row`: the Search result anatomy.
    return (
      <div ref={rowRef} className={`qs-row qs-message${selected ? " qs-selected" : ""}`} onClick={open}>
        <HitAvatar hit={view.hit} scale={SWITCHER_HIT} />
        <HitColumn hit={view.hit} scale={SWITCHER_HIT} onAccent={false} />
      </div>
    );
  }
  const isMessage = result.kind === "Message";
  const weight = result.kind === "UnreadChannel" ? 600 : 400;
  const primary = view?.primary ?? [{ text: result.label, matched: false }];
  return (
    <div
      ref={rowRef}
      className={`qs-row${isMessage ? " qs-message" : ""}${selected ? " qs-selected" : ""}`}
      onClick={open}
    >
      <div className="qs-icon">
        <RowIcon view={view} label={result.label} surface={surface} />
      </div>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        <div className="qs-line">
          <Segments segments={primary} color="var(--text-primary)" weight={isMessage ? 500 : weight} />
          {view?.simple_team && <span style={{ color: "var(--text-secondary)", whiteSpace: "pre" }}>{"  #general"}</span>}
          {view?.team && (
            <>
              <span style={{ color: "var(--text-secondary)", whiteSpace: "pre" }}>{"  "}</span>
              <Segments segments={view.team} color="var(--text-secondary)" weight={400} />
            </>
          )}
        </div>
      </div>
      {hint && <Keycap>{hint}</Keycap>}
    </div>
  );
}

/** `quick_switcher_icon`: DM faces, team mark, or `#`. */
function RowIcon({ view, label, surface }: { view: SwitcherRowView | undefined; label: string; surface: string }) {
  if (view?.is_dm) {
    return view.participants.length > 0 ? (
      <ParticipantsAvatar participants={view.participants} fallbackName={label} size={20} surface={surface} />
    ) : (
      <Icon name="dm" size={18} color="var(--text-secondary)" />
    );
  }
  if (view?.team_name) return <Avatar name={view.team_name} asset={view.team_avatar} size={20} team />;
  return <Icon name="hash" size={16} color="var(--text-secondary)" />;
}
