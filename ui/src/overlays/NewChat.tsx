// `new_chat::render_new_chat_modal` (src/views/overlays/new_chat.rs).

import { useEffect, useRef } from "react";
import { send, useModel } from "../core/engine";
import { Avatar, WithPresence } from "../ui/Avatar";
import {
  EmptyState,
  PrimaryButton,
  SearchFieldShell,
  SecondaryButton,
  SectionLabel,
  useFocused,
} from "../ui/common";
import { Icon } from "../ui/Icon";
import { Tooltip, shortcutGlyphs } from "../ui/Tooltip";
import { useModelInput } from "./MessageHit";
import type { NewChatModel, UserSummary } from "./types";

function displayName(user: UserSummary): string {
  return user.display_name.trim() === "" ? user.id : user.display_name;
}

/** `start_chat_label`. */
function startChatLabel(selected: UserSummary[], creating: boolean): string {
  if (creating) return "Starting…";
  if (selected.length === 0) return "Start chat";
  if (selected.length === 1) {
    const person = selected[0];
    return `Start chat with ${person.display_name.split(/\s+/).filter(Boolean)[0] ?? person.id}`;
  }
  return `Start group chat · ${selected.length} people`;
}

/** `new_chat_keyboard_hint`. */
function keyboardHint(hasCandidates: boolean, canStart: boolean): string | null {
  if (hasCandidates) return "↵ Add person · ⌘↵ Start chat";
  if (canStart) return "↵ Start chat";
  return null;
}

/** `matched_ranges`: case-insensitive occurrences of `query`, as [start, end) code-unit ranges. */
function matchedRanges(text: string, query: string): [number, number][] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];
  const haystack = text.toLowerCase();
  const ranges: [number, number][] = [];
  // Lowercasing can change length for a few scripts; fall back to no highlights then.
  if (haystack.length !== text.length) return ranges;
  let start = haystack.indexOf(needle);
  while (start >= 0) {
    ranges.push([start, start + needle.length]);
    start = haystack.indexOf(needle, start + needle.length);
  }
  return ranges;
}

function Highlighted({ text, query, underline }: { text: string; query: string; underline: boolean }) {
  const ranges = matchedRanges(text, query);
  const parts: React.ReactNode[] = [];
  let cursor = 0;
  ranges.forEach(([start, end], index) => {
    if (start > cursor) parts.push(text.slice(cursor, start));
    parts.push(
      <span key={index} style={{ fontWeight: 700, textDecoration: underline ? "underline" : undefined }}>
        {text.slice(start, end)}
      </span>,
    );
    cursor = end;
  });
  if (cursor < text.length) parts.push(text.slice(cursor));
  return <>{parts}</>;
}

export function NewChat() {
  const newChat = useModel("new_chat") as NewChatModel | undefined;
  const input = useModelInput(newChat?.search_query, (query) => send("overlays", "set_new_chat_query", { query }));
  const [focused, focusHandlers] = useFocused();
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => inputRef.current?.focus(), []);
  useEffect(() => {
    // `new_chat_create` keeps focus in the field while the modal stays open.
    if (newChat && !newChat.creating) inputRef.current?.focus();
  }, [newChat?.creating]);
  if (!newChat) return null;

  const query = newChat.search_query.trim();
  const highlighted = Math.min(newChat.highlighted_index, Math.max(0, newChat.search_results.length - 1));
  const canStart = newChat.selected_participants.length > 0 && !newChat.creating;
  const hint = keyboardHint(query !== "" && newChat.search_results.length > 0, canStart);

  let results: React.ReactNode;
  if (query === "") {
    results = (
      <EmptyState
        icon="dm"
        title="Start a conversation"
        body="Search for people by name or Keybase username. Add more than one for a group chat."
      />
    );
  } else if (newChat.search_results.length === 0 && newChat.is_searching) {
    results = <EmptyState icon="search" title="Searching Keybase…" />;
  } else if (newChat.search_results.length === 0) {
    results = (
      <EmptyState
        icon="search"
        title={`No people match “${query}”`}
        body="Check the spelling, or search by Keybase username."
      />
    );
  } else {
    results = (
      <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
        <SectionLabel style={{ padding: "0 8px 4px" }}>
          {newChat.is_searching ? "People · Searching Keybase…" : "People"}
        </SectionLabel>
        {newChat.search_results.map((user, index) => (
          <PersonRow
            key={user.id}
            user={user}
            query={query}
            added={newChat.selected_participants.some((selected) => selected.id === user.id)}
            highlighted={index === highlighted}
          />
        ))}
      </div>
    );
  }

  return (
    <div className="ov-dialog" onClick={(event) => event.stopPropagation()}>
      <div className="nc-title">New conversation</div>
      <div className="nc-top">
        {newChat.selected_participants.length > 0 && (
          <div style={{ display: "flex", alignItems: "flex-start", gap: 8 }}>
            <div
              style={{
                flexShrink: 0,
                height: 26,
                display: "flex",
                alignItems: "center",
                fontSize: 13,
                color: "var(--text-secondary)",
              }}
            >
              To
            </div>
            <div style={{ flex: 1, minWidth: 0, display: "flex", flexWrap: "wrap", gap: 6 }}>
              {newChat.selected_participants.map((user) => (
                <Chip key={user.id} user={user} />
              ))}
            </div>
          </div>
        )}
        <SearchFieldShell focused={focused} onClick={() => inputRef.current?.focus()}>
          <input
            placeholder="Search people by username"
            ref={inputRef}
            className="ov-input"
            spellCheck={false}
            autoComplete="off"
            data-overlay-input="new-chat"
            {...input}
            {...focusHandlers}
          />
        </SearchFieldShell>
        {newChat.error && (
          <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: "var(--affinity-broken)" }}>
            <Icon name="alert-triangle" size={13} color="var(--affinity-broken)" />
            {newChat.error}
          </div>
        )}
      </div>
      <div className="ov-scroll" style={{ padding: "0 8px 8px" }}>
        {results}
      </div>
      <div className="nc-footer">
        <div
          style={{
            flex: 1,
            minWidth: 0,
            overflow: "hidden",
            whiteSpace: "nowrap",
            textOverflow: "ellipsis",
            fontSize: 12,
            color: "var(--text-secondary)",
          }}
        >
          {hint?.replace("⌘↵", shortcutGlyphs("⌘↵"))}
        </div>
        <SecondaryButton onClick={() => send("overlays", "dismiss_overlays")}>Cancel</SecondaryButton>
        <PrimaryButton disabled={!canStart} onClick={() => send("overlays", "new_chat_create")}>
          {startChatLabel(newChat.selected_participants, newChat.creating)}
        </PrimaryButton>
      </div>
    </div>
  );
}

function Chip({ user }: { user: UserSummary }) {
  const name = displayName(user);
  return (
    <div className="nc-chip">
      <Avatar name={name} asset={user.avatar_asset} size={20} />
      {name}
      <Tooltip text={`Remove ${name}`} singleLine>
        <div
          className="nc-chip-remove"
          onClick={() => send("overlays", "new_chat_remove_participant", { user_id: user.id })}
        >
          <Icon name="x" size={14} color="var(--text-secondary)" />
        </div>
      </Tooltip>
    </div>
  );
}

/** `identity_marker_tinted` (with following): follow check or broken-proof warning, 13px, with a tooltip. */
function IdentityMarker({ affinity, name, highlighted }: { affinity: string; name: string; highlighted: boolean }) {
  const marker =
    affinity === "Positive"
      ? { icon: "user-check" as const, color: "var(--affinity-positive)", tip: `You follow ${name}` }
      : affinity === "Broken"
        ? {
            icon: "alert-triangle" as const,
            color: "var(--affinity-broken)",
            tip: `Identity broken: ${name}'s proofs changed since you followed them`,
          }
        : null;
  if (!marker) return null;
  return (
    <Tooltip text={marker.tip} singleLine>
      <span style={{ display: "flex", alignItems: "center", flexShrink: 0 }}>
        <Icon name={marker.icon} size={13} color={highlighted ? "var(--selected-fg)" : marker.color} />
      </span>
    </Tooltip>
  );
}

function PersonRow({
  user,
  query,
  added,
  highlighted,
}: {
  user: UserSummary;
  query: string;
  added: boolean;
  highlighted: boolean;
}) {
  const name = displayName(user);
  const detail = user.title.trim() === "" ? `@${user.id}` : `@${user.id} · ${user.title.trim()}`;
  const primary = highlighted ? "var(--selected-fg)" : "var(--text-primary)";
  const secondary = highlighted ? "var(--selected-secondary)" : "var(--text-secondary)";
  const surface = highlighted ? "var(--selected-bg)" : "var(--panel-bg)";
  return (
    <div
      className={`nc-person${highlighted ? " nc-highlighted" : added ? " nc-added" : ""}`}
      onClick={() =>
        send("overlays", added ? "new_chat_remove_participant" : "new_chat_add_participant", { user_id: user.id })
      }
    >
      <WithPresence size={28} availability={user.presence.availability} surface={surface} tooltip>
        <Avatar name={name} asset={user.avatar_asset} size={28} />
      </WithPresence>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <span style={{ fontSize: 13, color: primary }}>
            <Highlighted text={name} query={query} underline={highlighted} />
          </span>
          <IdentityMarker affinity={user.affinity} name={name} highlighted={highlighted} />
        </div>
        <div
          style={{
            overflow: "hidden",
            whiteSpace: "nowrap",
            textOverflow: "ellipsis",
            fontSize: 12,
            color: secondary,
          }}
        >
          <Highlighted text={detail} query={query} underline={highlighted} />
        </div>
      </div>
      {added && (
        <Icon name="check-circle" size={16} color={highlighted ? "var(--selected-fg)" : "var(--accent-strong)"} />
      )}
    </div>
  );
}
