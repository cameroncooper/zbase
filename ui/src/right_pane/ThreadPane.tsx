// Thread pane: the root, its replies and the reply composer (`ThreadPane::render`).

import { useLayoutEffect, useRef } from "react";
import { send } from "../core/engine";
import { Composer } from "../composer/Composer";
import { MessageRow } from "../timeline/MessageRow";
import type { MessageRow as TimelineMessageRow } from "../timeline/types";
import { EmptyState, PaneHeader } from "./parts";
import type { RightPaneState, ThreadPaneModel } from "./types";

export function ThreadPane({ thread, state }: { thread: ThreadPaneModel; state: RightPaneState }) {
  const rows = state.thread_rows;
  const rootId = thread.root_message_id;
  const scroll = useRef<HTMLDivElement>(null);
  const lastCount = useRef({ root: rootId, replies: thread.replies.length, pending: true });

  // `pending_thread_scroll_to_bottom` on open, and `sync_scroll_indicators`:
  // new replies scroll the thread to the bottom.
  useLayoutEffect(() => {
    const node = scroll.current;
    const last = lastCount.current;
    if (last.root !== rootId) {
      lastCount.current = { root: rootId, replies: thread.replies.length, pending: true };
    }
    const current = lastCount.current;
    const grew = thread.replies.length > current.replies;
    if (node && ((current.pending && !thread.loading) || grew)) {
      node.scrollTop = node.scrollHeight;
      if (!thread.loading) current.pending = false;
    }
    current.replies = thread.replies.length;
  }, [rootId, thread.replies.length, thread.loading, rows]);

  // Media in the thread stays inside the message column: row padding (32),
  // avatar and gap (44) and a guard (8).
  const mediaMaxWidth = Math.min(280, Math.max(72, thread.width_px - 4 - 84));

  return (
    <div className="rp-pane" style={{ overflow: "hidden" }}>
      <PaneHeader title="Thread" subtitle={state.location} />
      <div style={{ flex: 1, minHeight: 0, position: "relative" }}>
        <div ref={scroll} className="rp-scroll" style={{ height: "100%" }}>
          <div style={{ paddingTop: 12, paddingBottom: 16, display: "flex", flexDirection: "column", gap: 4 }}>
            {rows.map((row) => {
              const isRoot = row.message.id === rootId;
              return (
                <div key={row.message.id}>
                  <MessageRow row={row as unknown as TimelineMessageRow} thread mediaMaxWidth={mediaMaxWidth} />
                  {isRoot &&
                    (state.thread_reply_count > 0 ? (
                      <RepliesDivider count={state.thread_reply_count} />
                    ) : (
                      <NoRepliesNote />
                    ))}
                </div>
              );
            })}
          </div>
          {rows.length === 0 && (
            <EmptyState icon="thread" title={thread.loading ? "Loading thread…" : "This message isn't available"} />
          )}
        </div>
        <div className="rp-fade-top" />
      </div>
      <div className="rp-thread-composer">
        {rootId && <Composer target={{ kind: "thread", rootId }} />}
      </div>
    </div>
  );
}

/** `thread_replies_divider`. */
function RepliesDivider({ count }: { count: number }) {
  return (
    <div className="rp-thread-divider">
      <div className="rp-thread-divider-label">
        {count} {count === 1 ? "reply" : "replies"}
      </div>
      <div className="rp-hairline" />
    </div>
  );
}

/** `thread_no_replies_note`. */
function NoRepliesNote() {
  return (
    <div style={{ padding: "12px 16px 8px", display: "flex", flexDirection: "column", gap: 6 }}>
      <div style={{ width: "100%", display: "flex", alignItems: "center", gap: 12 }}>
        <div className="rp-thread-divider-label">No replies yet</div>
        <div className="rp-hairline" />
      </div>
      <div className="rp-thread-note">Reply below. Replies also appear in the conversation, quoting this message.</div>
    </div>
  );
}

/** Opens the thread pane for `rootId`, for other areas that render "Reply in thread". */
export function openThread(rootId: string) {
  return send("right_pane", "open_thread", { root_id: rootId });
}
