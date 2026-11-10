// Global search, shown in the main panel for `Route::Search` (src/views/search).

import { useEffect, useRef } from "react";
import { assetUrl, send, useModel } from "../core/engine";
import { EmptyState, Keycap, SearchFieldShell, useFocused } from "../ui/common";
import { Icon, type IconName } from "../ui/Icon";
import { Tooltip, shortcutGlyphs } from "../ui/Tooltip";
import { HitAvatar, HitColumn, SEARCH_PAGE_HIT, useModelInput } from "./MessageHit";
import "./overlays.css";
import type { AttachmentSummary, HitView, LinkPreview, SearchFilter, SearchModel, SearchResult } from "./types";

const FILTERS: { label: string; icon: IconName; filter: SearchFilter; name: string }[] = [
  { label: "Has file", icon: "paperclip", filter: "HasFile", name: "has_file" },
  { label: "Mentions me", icon: "at-sign", filter: "MentionsMe", name: "mentions_me" },
];

export function SearchView() {
  const search = useModel("search") as SearchModel | undefined;
  const navigation = useModel("navigation");
  const extra = useModel("overlays_extra");
  const input = useModelInput(search?.query, (query) => send("overlays", "set_search_query", { query }));
  const [focused, focusHandlers] = useFocused();
  const inputRef = useRef<HTMLInputElement>(null);
  const selectedRef = useRef<HTMLDivElement>(null);
  useEffect(() => selectedRef.current?.scrollIntoView({ block: "nearest" }), [search?.highlighted_index]);
  if (!search) return <div className="sv-root" />;

  const canGoBack = (navigation?.back_stack?.length ?? 0) > 0;
  const query = search.query.trim();
  const submitted = search.submitted_query?.trim() ?? null;
  const count = search.results.length;
  const noun = count === 1 ? "result" : "results";
  const status = search.is_loading
    ? "Searching…"
    : submitted !== null && count > 0
      ? submitted === ""
        ? `${count} ${noun}`
        : `${count} ${noun} for “${submitted}”`
      : null;
  const rows: HitView[] = extra?.search_rows ?? [];

  return (
    <div className="sv-root">
      <div className="sv-header" style={{ paddingLeft: canGoBack ? 16 : 24 }}>
        {canGoBack && (
          <Tooltip text="Back" shortcut={shortcutGlyphs("⌘[")}>
            <div className="sv-back" onClick={() => send("overlays", "navigate_back")}>
              <Icon name="chevron-left" size={16} color="var(--text-secondary)" />
            </div>
          </Tooltip>
        )}
        Search
      </div>
      <div className="sv-controls">
        <SearchFieldShell focused={focused} onClick={() => inputRef.current?.focus()}>
          <input
            placeholder="Search messages, people, channels"
            ref={inputRef}
            className="ov-input"
            spellCheck={false}
            autoComplete="off"
            data-overlay-input="search"
            {...input}
            {...focusHandlers}
          />
        </SearchFieldShell>
        <div className="sv-filters">
          {FILTERS.map(({ label, icon, filter, name }) => {
            const active = search.filters.some((existing) => JSON.stringify(existing) === JSON.stringify(filter));
            return (
              <div
                key={name}
                className={`sv-chip${active ? " sv-active" : ""}`}
                onClick={() => send("overlays", "toggle_search_filter", { filter: name })}
              >
                <Icon name={icon} size={12} color={active ? "#ffffff" : "var(--text-secondary)"} />
                {label}
              </div>
            );
          })}
          <div style={{ flex: 1 }} />
          {status && <div className="sv-status">{status}</div>}
        </div>
      </div>
      <div className="sv-results">
        {count === 0 ? (
          <EmptyBody search={search} query={query} submitted={submitted} />
        ) : (
          <div className="sv-list">
            {search.results.map((result, index) => {
              const hit = rows[index];
              if (!hit) return null;
              const selected = search.highlighted_index === index;
              return (
                <SearchRow
                  key={`${result.conversation_id}|${result.message.id}`}
                  index={index}
                  result={result}
                  hit={hit}
                  selected={selected}
                  rowRef={selected ? selectedRef : undefined}
                />
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function EmptyBody({ search, query, submitted }: { search: SearchModel; query: string; submitted: string | null }) {
  if (search.is_loading) return null;
  let state: React.ReactNode;
  if (submitted !== null && submitted === query) {
    state = (
      <EmptyState
        icon="search"
        title={submitted === "" ? "No matching messages" : `No results for “${submitted}”`}
        body={
          search.filters.length === 0
            ? "Check the spelling or try different words."
            : "Try removing a filter or searching for different words."
        }
      />
    );
  } else if (query !== "") {
    state = (
      <EmptyState icon="search" title={`Search for “${query}”`}>
        <div
          style={{
            paddingTop: 4,
            display: "flex",
            alignItems: "center",
            gap: 6,
            fontSize: 13,
            color: "var(--text-secondary)",
          }}
        >
          Press <Keycap>↵</Keycap> to search every conversation
        </div>
      </EmptyState>
    );
  } else {
    state = (
      <EmptyState
        icon="search"
        title="Search your messages"
        body="Find messages, people and files across every conversation you're in."
      />
    );
  }
  return <div style={{ paddingTop: 24 }}>{state}</div>;
}

/** `attachment_size_text`. */
function sizeText(bytes: number): string | null {
  if (bytes === 0) return null;
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit + 1 < units.length) {
    value /= 1024;
    unit += 1;
  }
  return value >= 10 ? `${value.toFixed(0)} ${units[unit]}` : `${value.toFixed(1)} ${units[unit]}`;
}

/** `attachment_file_row`. */
function FileRow({ attachment }: { attachment: AttachmentSummary }) {
  const size = sizeText(attachment.size_bytes);
  return (
    <div className="sv-file">
      <Icon name="files" size={16} color="var(--text-secondary)" />
      <span className="sv-file-name">{attachment.name}</span>
      {size && <span style={{ flexShrink: 0, fontSize: 12, color: "var(--text-secondary)" }}>{size}</span>}
    </div>
  );
}

/** `search_result_row`. */
function SearchRow({
  index,
  result,
  hit,
  selected,
  rowRef,
}: {
  index: number;
  result: SearchResult;
  hit: HitView;
  selected: boolean;
  rowRef?: React.Ref<HTMLDivElement>;
}) {
  const message = result.message;
  const replies = message.thread_reply_count;
  return (
    <div
      ref={rowRef}
      className={`sv-row${selected ? " sv-selected" : ""}`}
      onClick={() => send("overlays", "open_search_result", { index })}
    >
      <HitAvatar hit={hit} scale={SEARCH_PAGE_HIT} />
      <HitColumn hit={hit} scale={SEARCH_PAGE_HIT} onAccent={selected}>
        {message.attachments.length === 1 && (
          <div style={{ paddingTop: 4 }}>
            <FileRow attachment={message.attachments[0]} />
          </div>
        )}
        {message.attachments.length > 1 && (
          <div style={{ fontSize: 12, color: selected ? "var(--selected-secondary)" : "var(--text-secondary)" }}>
            {message.attachments.length} files
          </div>
        )}
        {replies > 0 && (
          <div style={{ fontSize: 12, fontWeight: 500, color: selected ? "var(--selected-fg)" : "var(--link)" }}>
            {replies} {replies === 1 ? "reply" : "replies"}
          </div>
        )}
        {message.link_previews.length > 0 && <LinkPreviews previews={message.link_previews} />}
      </HitColumn>
    </div>
  );
}

/** `media_frame_size`. */
function mediaFrame(width: number | null, height: number | null, maxWidth: number, maxHeight: number) {
  let w = width ?? 240;
  let h = height ?? 160;
  if (w <= 1 || h <= 1) return [Math.min(maxWidth, 240), Math.min(maxHeight, 160)] as const;
  const scale = Math.min(maxWidth / w, maxHeight / h, 1);
  w *= scale;
  h *= scale;
  return [Math.max(w, 96), Math.max(h, 72)] as const;
}

function openUrl(event: React.MouseEvent, url: string) {
  // `open_url_or_deep_link`: the shell resolves keybase:// links; others open in the browser.
  event.stopPropagation();
  const link = document.createElement("a");
  link.href = url;
  document.body.appendChild(link);
  link.click();
  link.remove();
}

/** `render_link_previews`: the first preview, then "+N more links". */
function LinkPreviews({ previews }: { previews: LinkPreview[] }) {
  const hidden = previews.length - 1;
  const preview = previews[0];
  const lower = preview.url.toLowerCase();
  const isGiphy = lower.includes("giphy.com") || lower.includes("gph.is");
  let body: React.ReactNode;
  if (preview.is_media) {
    const [width, height] = mediaFrame(preview.media_width, preview.media_height, 260, 180);
    const source = assetUrl(preview.thumbnail_asset ?? preview.url);
    body = (
      <div
        onClick={(event) => openUrl(event, preview.url)}
        style={{ position: "relative", width, height, borderRadius: 6, overflow: "hidden", cursor: "pointer" }}
      >
        <img src={source} alt={preview.site ?? "media"} style={{ width, height, objectFit: "contain" }} />
        {preview.is_video && (
          <div
            style={{
              position: "absolute",
              right: 4,
              bottom: 4,
              padding: "2px 4px",
              borderRadius: 4,
              background: "var(--panel-alt-bg)",
              border: "1px solid var(--border)",
              fontSize: 12,
              color: "var(--text-primary)",
            }}
          >
            video
          </div>
        )}
      </div>
    );
  } else {
    const title = isGiphy ? preview.title?.trim() || "GIPHY" : (preview.title ?? preview.url);
    let thumb: [number, number] = [260, 150];
    if (preview.media_width && preview.media_height) {
      const scale = Math.min(260 / preview.media_width, 150 / preview.media_height, 1);
      thumb = [preview.media_width * scale, preview.media_height * scale];
    }
    body = (
      <div className="sv-link" onClick={(event) => openUrl(event, preview.url)}>
        <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>{preview.site ?? "link"}</div>
        <div style={{ fontSize: 14 }}>{title}</div>
        {preview.title !== null && !isGiphy && (
          <div style={{ fontSize: 12, color: "var(--accent)" }}>{preview.url}</div>
        )}
        {preview.thumbnail_asset && (
          <img
            src={assetUrl(preview.thumbnail_asset)}
            alt=""
            style={{ marginTop: 2, width: thumb[0], height: thumb[1], borderRadius: 6, objectFit: "contain" }}
          />
        )}
      </div>
    );
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 4 }}>
      {body}
      {hidden > 0 && <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>+{hidden} more links</div>}
    </div>
  );
}
