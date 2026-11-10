// Search pane: the conversation's hits from the current search (`render_search_panel`).
// The row follows `search_result_row` at the narrow scale, without location or previews.

import type { ReactNode } from "react";
import { send } from "../core/engine";
import { Avatar, EmptyState, PaneHeader, PaneWithHeader } from "./parts";
import type { RightPaneState, SearchResultView, SearchView } from "./types";

/** `compact_date_label`: time today, "Yesterday", "Mar 4", "Mar 4, 2023". */
function compactDateLabel(timestamp: number | null): string {
  if (timestamp == null) return "Now";
  const date = new Date(timestamp);
  const now = new Date();
  if (date.toDateString() === now.toDateString())
    return date.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
  if (date.toDateString() === new Date(now.getTime() - 86_400_000).toDateString()) return "Yesterday";
  return date.toLocaleDateString(
    "en-US",
    date.getFullYear() === now.getFullYear()
      ? { month: "short", day: "numeric" }
      : { month: "short", day: "numeric", year: "numeric" },
  );
}

/** The snippet with its highlighted byte ranges marked, whitespace collapsed. */
function snippetNodes(result: SearchResultView): ReactNode[] {
  const bytes = new TextEncoder().encode(result.snippet);
  const decoder = new TextDecoder();
  const ranges = [...result.snippet_highlight_ranges].sort((a, b) => a[0] - b[0]);
  const nodes: ReactNode[] = [];
  let at = 0;
  const clean = (text: string) => text.replace(/\s+/g, " ");
  ranges.forEach(([start, end], index) => {
    if (start < at || end > bytes.length) return;
    if (start > at) nodes.push(clean(decoder.decode(bytes.slice(at, start))));
    nodes.push(<mark key={index}>{clean(decoder.decode(bytes.slice(start, end)))}</mark>);
    at = end;
  });
  if (at < bytes.length) nodes.push(clean(decoder.decode(bytes.slice(at))));
  return nodes;
}

export function SearchPane({ search, state }: { search: SearchView; state: RightPaneState }) {
  const contexts = state.search_contexts;
  return (
    <PaneWithHeader header={<PaneHeader title="Search" subtitle={state.location} />}>
      <div className="rp-scroll" style={{ padding: 8, display: "flex", flexDirection: "column", gap: 2 }}>
        {contexts.length === 0 && (
          <EmptyState
            icon="search"
            title={search.is_loading ? "Searching…" : "No matches here"}
            body={search.is_loading ? null : "Search results from this conversation appear here."}
          />
        )}
        {contexts.map((context) => {
          const result = search.results[context.index];
          if (!result) return null;
          const selected = search.highlighted_index === context.index;
          const files = result.message.attachments;
          const replies = result.message.thread_reply_count;
          return (
            <div
              key={context.index}
              className={`rp-search-hit${selected ? " rp-selected" : ""}`}
              onClick={() => send("right_pane", "open_search_result", { index: context.index })}
            >
              <div style={{ flexShrink: 0 }}>
                <Avatar name={context.author_name} asset={context.author_avatar} size={24} />
              </div>
              <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
                <div style={{ width: "100%", display: "flex", alignItems: "center", gap: 6 }}>
                  <div style={{ flexShrink: 0, fontSize: 13, lineHeight: "18px", fontWeight: 600 }}>{context.author_name}</div>
                  <div style={{ flex: 1 }} />
                  <div
                    style={{ flexShrink: 0, fontSize: 12, lineHeight: "18px", color: selected ? "inherit" : "var(--text-secondary)" }}
                    title={result.message.timestamp_ms ? new Date(result.message.timestamp_ms).toLocaleString() : undefined}
                  >
                    {compactDateLabel(result.message.timestamp_ms)}
                  </div>
                </div>
                <div className="rp-search-snippet">{snippetNodes(result)}</div>
                {files.length === 1 && (
                  <div style={{ paddingTop: 4, fontSize: 12, color: selected ? "inherit" : "var(--text-secondary)" }}>
                    {files[0].name}
                  </div>
                )}
                {files.length > 1 && (
                  <div style={{ fontSize: 12, color: selected ? "inherit" : "var(--text-secondary)" }}>{files.length} files</div>
                )}
                {replies > 0 && (
                  <div style={{ fontSize: 12, fontWeight: 500, color: selected ? "inherit" : "var(--link)" }}>
                    {replies} {replies === 1 ? "reply" : "replies"}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </PaneWithHeader>
  );
}
