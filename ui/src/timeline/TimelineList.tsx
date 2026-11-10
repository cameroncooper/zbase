// The scrolling timeline: `TimelineList::render` plus the gpui list behaviour
// AppWindow relies on. Following the newest message while at the bottom
// (`anchor_timeline_before_layout`), keeping the reading position when rows
// change above, the engine's scroll instructions (jumps, anchors after older
// pages) and the layout facts it reads back (`reportView`).

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { readModel, reportView, send, useModel } from "../core/engine";
import { Icon, type IconName } from "../ui/Icon";
import { clearHoverNow, clearHoverSoon } from "./hover";
import { MessageRow } from "./MessageRow";
import type { EventSpan, SystemEventIcon, TimelineRow } from "./types";

/** `is_list_near_bottom` / `is_list_near_top`. */
const EDGE_PX = 24;
/** How much of a row may scroll away before its author line is gone. */
const HEADER_CLIP_PX = 20;
const SCROLL_REPORT_INTERVAL_MS = 100;

const EVENT_ICONS: Record<SystemEventIcon, [IconName, number]> = {
  Join: ["arrow-right", 16],
  Leave: ["arrow-left", 16],
  Add: ["plus", 18],
  Remove: ["activity", 18],
  Pin: ["pin", 18],
  Description: ["hash", 16],
  Settings: ["sliders", 18],
  Info: ["activity", 18],
};

/** Stable keys, so the reading anchor survives rows added above it. */
function rowKeys(rows: TimelineRow[]): string[] {
  const keys = new Array<string>(rows.length);
  let next = "end";
  const seen = new Map<string, number>();
  for (let index = rows.length - 1; index >= 0; index--) {
    const row = rows[index];
    let key: string;
    if ("Message" in row) {
      next = row.Message.message.id;
      key = `m:${next}`;
    } else if ("DateDivider" in row) key = `d:${row.DateDivider}:${next}`;
    else if ("UnreadDivider" in row) key = `u:${next}`;
    else if ("LoadingIndicator" in row) key = `l:${next}`;
    else key = `e:${next}`;
    const count = seen.get(key) ?? 0;
    seen.set(key, count + 1);
    keys[index] = count ? `${key}:${count}` : key;
  }
  return keys;
}

function SystemEvent({ icon, spans }: { icon: SystemEventIcon; spans: EventSpan[] }) {
  const [name, size] = EVENT_ICONS[icon];
  return (
    <div className="tl-event-row">
      <Icon name={name} size={size} />
      <div className="tl-event-spans">
        {spans.map((span, index) => {
          if ("Actor" in span) return <span key={index} className="tl-event-actor">{span.Actor}</span>;
          if ("Text" in span) return <span key={index}>{span.Text}</span>;
          if ("ChannelLink" in span) {
            const link = span.ChannelLink;
            return (
              <span
                key={index}
                className="tl-event-link"
                onPointerMove={(event) => {
                  clearHoverNow();
                  event.stopPropagation();
                }}
                onClick={() =>
                  send("timeline", "open_channel_link", {
                    channel_name: link.channel_name,
                    team_name: link.team_name,
                    conv_id: link.conv_id,
                  })
                }
              >
                #{link.channel_name}
              </span>
            );
          }
          return (
            <span
              key={index}
              className="tl-event-link"
              onPointerMove={(event) => {
                clearHoverNow();
                event.stopPropagation();
              }}
              onClick={() => send("right_pane", "open_profile", { user_id: span.UserLink })}
            >
              {span.UserLink}
            </span>
          );
        })}
      </div>
    </div>
  );
}

function Row({ row, clippedParentAuthor }: { row: TimelineRow; clippedParentAuthor: string | null }) {
  if ("DateDivider" in row) {
    return (
      <div className="tl-day-divider">
        <div className="tl-hairline" />
        <div className="tl-divider-label">{row.DateDivider}</div>
        <div className="tl-hairline" />
      </div>
    );
  }
  if ("UnreadDivider" in row) {
    return (
      <div className="tl-unread-divider">
        <div className="tl-hairline" />
        <div className="tl-divider-label">{row.UnreadDivider}</div>
      </div>
    );
  }
  if ("SystemEvent" in row) return <SystemEvent {...row.SystemEvent} />;
  if ("LoadingIndicator" in row) {
    return (
      <div className="tl-loading-row">
        <Icon name="activity" size={18} color="var(--accent)" />
        {row.LoadingIndicator}
      </div>
    );
  }
  return <MessageRow row={row.Message} clippedParentAuthor={clippedParentAuthor} />;
}

type Anchor = { key: string; offset: number };

export function TimelineList() {
  const timeline = useModel("timeline");
  const extra = useModel("timeline_extra");
  const settings = useModel("settings") as { density?: string } | undefined;
  const scrollRef = useRef<HTMLDivElement>(null);
  const rowsRef = useRef<HTMLDivElement>(null);
  /** At the newest message: keep following it as content grows. */
  const pinned = useRef(true);
  const anchor = useRef<Anchor | null>(null);
  const appliedSeq = useRef(0);
  const userScrollUntil = useRef(0);
  const lastScrolledSent = useRef(0);
  const scrolledTimer = useRef<number | undefined>(undefined);
  const [atBottom, setAtBottom] = useState(true);
  const [scrolledDown, setScrolledDown] = useState(false);
  const [clippedRoot, setClippedRoot] = useState<string | null>(null);
  const rows = timeline?.rows ?? [];
  const keys = useMemo(() => rowKeys(rows), [rows]);

  const nearBottom = () => {
    const el = scrollRef.current;
    if (!el) return true;
    return el.scrollHeight - el.clientHeight - el.scrollTop <= EDGE_PX;
  };

  /** The row at the top edge of the viewport and how far it is scrolled into. */
  const topRow = (): { element: HTMLElement; offset: number } | null => {
    const el = scrollRef.current;
    const list = rowsRef.current;
    if (!el || !list) return null;
    const top = el.scrollTop;
    for (const child of Array.from(list.children) as HTMLElement[]) {
      if (child.offsetTop + child.offsetHeight > top) return { element: child, offset: top - child.offsetTop };
    }
    return null;
  };

  const captureAnchor = () => {
    const top = topRow();
    anchor.current = top ? { key: top.element.dataset.key!, offset: top.offset } : null;
  };

  const rowElement = (selector: string) => rowsRef.current?.querySelector<HTMLElement>(selector) ?? null;

  const scrollToBottom = () => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
    pinned.current = true;
  };

  const restoreAnchor = () => {
    const el = scrollRef.current;
    const saved = anchor.current;
    if (!el || !saved) return;
    const row = rowElement(`[data-key="${CSS.escape(saved.key)}"]`);
    if (row) el.scrollTop = row.offsetTop + saved.offset;
  };

  /** Layout facts for the engine: near the bottom, first visible message, clipped thread root. */
  const report = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const bottom = nearBottom();
    setAtBottom(bottom);
    setScrolledDown(el.scrollTop > 0);
    const top = topRow();
    const latestRows = readModel("timeline")?.rows ?? [];
    const state = readModel("timeline_extra");
    let firstVisible: string | null = null;
    let clipped: string | null = null;
    if (top) {
      const index = Number(top.element.dataset.index);
      const row = latestRows[index];
      if (row && "Message" in row && row.Message.message.thread_reply_count > 0 && top.offset > HEADER_CLIP_PX) {
        clipped = row.Message.message.id;
      }
      for (let i = index; i < latestRows.length; i++) {
        const candidate = latestRows[i];
        if (candidate && "Message" in candidate) {
          firstVisible = candidate.Message.message.id;
          break;
        }
      }
    }
    setClippedRoot(clipped);
    // gpui only reads it while scrolled up or jumping, and not while an
    // older-page restore owns the position.
    const useFirst = (!bottom || state?.jump_to_message_active) && !state?.older_restore_pending;
    reportView({ timeline_near_bottom: bottom, first_visible_message_id: useFirst ? firstVisible : null });
  }, []);

  const sendScrolled = (nearTop: boolean) => {
    window.clearTimeout(scrolledTimer.current);
    const now = performance.now();
    const fire = () => {
      lastScrolledSent.current = performance.now();
      send("timeline", "scrolled", { near_top: nearTop });
    };
    if (now - lastScrolledSent.current >= SCROLL_REPORT_INTERVAL_MS) fire();
    else scrolledTimer.current = window.setTimeout(fire, SCROLL_REPORT_INTERVAL_MS);
  };

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const user = performance.now() < userScrollUntil.current;
    if (user) {
      pinned.current = nearBottom();
      sendScrolled(el.scrollTop <= EDGE_PX);
    }
    captureAnchor();
    report();
  };

  const markUserScroll = () => {
    userScrollUntil.current = performance.now() + 300;
  };

  /** Applies the engine's newest scroll instruction once its target is laid out. */
  const applyCommand = () => {
    const command = extra?.scroll;
    if (!command || command.seq <= appliedSeq.current) return;
    const el = scrollRef.current;
    if (!el) return;
    const target = command.target;
    switch (target.kind) {
      case "none":
        break;
      case "bottom":
        scrollToBottom();
        break;
      case "message": {
        const row = rowElement(`[data-key="${CSS.escape(`m:${target.message_id}`)}"]`);
        if (!row) return;
        el.scrollTop = row.offsetTop + target.offset;
        pinned.current = nearBottom();
        break;
      }
      case "anchor": {
        const key = target.message_id ? `m:${target.message_id}` : null;
        if (key && anchor.current?.key === key && rowElement(`[data-key="${CSS.escape(key)}"]`)) {
          // Same row the reader was on: keep the exact offset into it.
          restoreAnchor();
        } else {
          const row = key
            ? rowElement(`[data-key="${CSS.escape(key)}"]`)
            : rowElement(`[data-index="${target.row_index}"]`);
          if (!row) return;
          el.scrollTop = row.offsetTop;
        }
        pinned.current = nearBottom();
        break;
      }
    }
    appliedSeq.current = command.seq;
    send("timeline", "scroll_applied", { seq: command.seq });
    captureAnchor();
    report();
  };

  const explicitScrollPending = () => {
    const state = readModel("timeline_extra");
    const model = readModel("timeline");
    return (
      !!state?.jump_to_message_active ||
      !!model?.pending_scroll_target ||
      (state?.scroll.seq ?? 0) > appliedSeq.current
    );
  };

  /** `anchor_timeline_before_layout`: follow the newest message, else keep the reading position. */
  const settle = () => {
    if (pinned.current && !explicitScrollPending()) scrollToBottom();
    else restoreAnchor();
    applyCommand();
    report();
  };

  useLayoutEffect(settle, [rows, extra?.messages, extra?.scroll.seq]);

  useEffect(() => {
    const el = scrollRef.current;
    const list = rowsRef.current;
    if (!el || !list) return;
    // Images, previews and highlighting change heights after the rows render.
    const observer = new ResizeObserver(() => {
      if (pinned.current && !explicitScrollPending()) scrollToBottom();
      else restoreAnchor();
      report();
    });
    observer.observe(el);
    observer.observe(list);
    return () => observer.disconnect();
  }, []);

  useEffect(() => () => window.clearTimeout(scrolledTimer.current), []);

  const conversationId = timeline?.conversation_id;
  useEffect(() => {
    // A newly opened conversation starts at the bottom until told otherwise.
    pinned.current = true;
    anchor.current = null;
  }, [conversationId]);

  const showJump = (!atBottom && rows.length > 0) || (extra?.unseen_count ?? 0) > 0;
  const unseen = extra?.unseen_count ?? 0;
  const clippedAuthor = (id: string) => {
    if (id !== clippedRoot) return null;
    const row = rows.find((candidate) => "Message" in candidate && candidate.Message.message.id === id);
    return row && "Message" in row ? row.Message.author.display_name : null;
  };

  return (
    <div className={`tl-body${settings?.density === "Compact" ? " tl-compact" : ""}`}>
      <div
        ref={scrollRef}
        className="tl-scroll"
        onScroll={onScroll}
        onWheel={(event) => {
          markUserScroll();
          const el = scrollRef.current;
          // gpui reacts to the wheel even when there is nothing to scroll.
          if (el && el.scrollHeight <= el.clientHeight && event.deltaY < 0) sendScrolled(true);
        }}
        onKeyDown={markUserScroll}
        onPointerDown={markUserScroll}
        onTouchMove={markUserScroll}
        onPointerLeave={clearHoverSoon}
        tabIndex={-1}
      >
        <div ref={rowsRef} className="tl-rows" style={{ position: "relative" }}>
          {rows.map((row, index) => (
            <div key={keys[index]} data-key={keys[index]} data-index={index} style={{ width: "100%", minWidth: 0 }}>
              <Row
                row={row}
                clippedParentAuthor={"Message" in row ? clippedAuthor(row.Message.message.id) : null}
              />
            </div>
          ))}
        </div>
      </div>
      {scrolledDown && <div className="tl-edge-shadow" />}
      {showJump && (
        <div className="tl-jump-wrap">
          <div
            className="tl-jump-pill"
            onClick={() => send("timeline", "jump_to_latest")}
            onPointerMove={(event) => {
              clearHoverSoon();
              event.stopPropagation();
            }}
          >
            <Icon name="chevron-down" size={14} />
            {unseen > 0 ? `${unseen} new message${unseen === 1 ? "" : "s"}` : "Jump to latest"}
          </div>
        </div>
      )}
    </div>
  );
}
