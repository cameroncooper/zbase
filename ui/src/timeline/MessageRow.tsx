// One message row: `TimelineList::render_message_row` and `render_message`
// (views/timeline/mod.rs). Shared with the thread pane: pass `thread` for a
// row in the right pane (always a header, the full timestamp, no reply quote
// or thread summary, delete but no edit) and its media column width.

import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { assetUrl, send, useModel } from "../core/engine";
import { Avatar } from "../ui/Avatar";
import { Icon } from "../ui/Icon";
import { Tooltip } from "../ui/Tooltip";
import { Attachments, LinkPreviews } from "./Attachments";
import {
  BUTTON_SIZE,
  cancelHoverClear,
  clearHoverNow,
  clearHoverSoon,
  hoverMove,
  hoverWithoutToolbar,
  placeToolbar,
  showToolbarNow,
  toolbarSize,
  useHover,
} from "./hover";
import { MessageBlocks } from "./RichText";
import type { MessageRow as MessageRowModel, MessageView, ReactionView, ThreadSummary } from "./types";
import "./timeline.css";

/** `QUICK_REACT_EMOJI`. */
const QUICK_REACT_EMOJI: [string, string][] = [
  ["+1", "\u{1F44D}"],
  ["heart", "❤️"],
  ["joy", "\u{1F602}"],
  ["tada", "\u{1F389}"],
  ["eyes", "\u{1F440}"],
];

export type MessageRowProps = {
  row: MessageRowModel;
  /** A row in the thread pane rather than the main timeline. */
  thread?: boolean;
  /** Media width limit in the thread pane (gpui's `thread_media_max_width`). */
  mediaMaxWidth?: number;
  /** "2 replies" under thread roots; the main timeline shows it. */
  showThreadBadge?: boolean;
  /** The root's author, while its header is scrolled above the viewport. */
  clippedParentAuthor?: string | null;
};

export function MessageRow({
  row,
  thread = false,
  mediaMaxWidth,
  showThreadBadge = !thread,
  clippedParentAuthor = null,
}: MessageRowProps) {
  const timeline = useModel("timeline");
  const extra = useModel("timeline_extra");
  const hover = useHover();
  const message = row.message;
  const view = extra?.messages[message.id];
  if (!timeline || !view) return null;

  const hovered = hover?.id === message.id && hover.thread === thread;
  const isMainThreadReply = !thread && message.reply_to !== null;
  const showHeader = row.show_header || isMainThreadReply || thread;
  const uploading = view.attachments.some((attachment) => attachment.upload);
  const sending = message.send_state === "Pending" && !uploading;
  const highlighted =
    timeline.highlighted_message_id === message.id || timeline.editing_message_id === message.id;
  const openThreadParent = !thread && timeline.open_thread_root_id === message.id;
  const me = timeline.current_user_id;
  const isYou = !!me && me.toLowerCase() === row.author.id.toLowerCase();

  const onMove = (event: React.PointerEvent) => {
    hoverMove(message.id, thread, event.clientX, event.clientY);
    event.stopPropagation();
  };
  const hold = (event: React.PointerEvent) => {
    hoverWithoutToolbar(message.id, thread);
    event.stopPropagation();
  };
  const openProfile = () => send("right_pane", "open_profile", { user_id: row.author.id });
  const openThread = (rootId: string) => send("right_pane", "open_thread", { root_id: rootId });

  const classes = [
    "tl-row",
    showHeader && "header",
    hovered && !highlighted && !openThreadParent && "hovered",
    openThreadParent && "open-thread",
  ]
    .filter(Boolean)
    .join(" ");

  const content = (
    <div className="tl-content">
      {showHeader && (
        <div className="tl-author-line">
          <Tooltip
            text={
              row.author.affinity === "Positive" && !isYou
                ? `@${row.author.id} · You follow ${row.author.display_name}`
                : `@${row.author.id}`
            }
            singleLine
          >
            <span className="tl-author-name" onClick={openProfile} onPointerMove={hold}>
              {row.author.display_name}
            </span>
          </Tooltip>
          {!isYou && row.author.affinity === "Broken" && (
            <Tooltip
              text={`Identity broken: ${row.author.display_name}'s proofs changed since you followed them`}
              singleLine
            >
              <span style={{ display: "flex", color: "var(--affinity-broken)" }}>
                <Icon name="alert-triangle" size={13} />
              </span>
            </Tooltip>
          )}
          <Timestamp view={view} thread={thread} />
          {sending && <PendingClock />}
        </div>
      )}
      {!thread && message.reply_to && (
        <div
          className="tl-reply-quote"
          onClick={() => openThread(message.thread_root_id ?? message.reply_to!)}
          onPointerMove={hold}
        >
          {view.reply ? (
            <>
              <span className="tl-reply-author">{view.reply.author}</span>
              {view.reply.excerpt && ` ${view.reply.excerpt}`}
            </>
          ) : (
            "Reply in thread"
          )}
        </div>
      )}
      <MessageBody
        row={row}
        view={view}
        thread={thread}
        highlighted={highlighted}
        hovered={hovered}
        mediaMaxWidth={mediaMaxWidth}
        showThreadBadge={showThreadBadge}
        threadSummary={timeline.thread_summaries[message.id]}
        clippedParentAuthor={clippedParentAuthor}
        onMove={onMove}
        hold={hold}
        openThread={openThread}
      />
    </div>
  );

  return (
    <div
      className={classes}
      data-message-id={thread ? undefined : message.id}
      onPointerMove={onMove}
      onPointerLeave={clearHoverSoon}
      onContextMenu={(event) => {
        event.preventDefault();
        showToolbarNow(message.id, thread, event.clientX, event.clientY);
      }}
    >
      {showHeader ? (
        <div className="tl-avatar-col" onClick={openProfile} onPointerMove={hold}>
          <Avatar name={row.author.display_name} asset={row.author.avatar_asset} size={32} />
        </div>
      ) : (
        // A continuation's clock sits in the avatar column, level with its first line.
        <div className="tl-gutter">{sending && <PendingClock />}</div>
      )}
      {content}
    </div>
  );
}

function Timestamp({ view, thread }: { view: MessageView; thread: boolean }) {
  const label = <span className="tl-timestamp">{thread ? view.timestamp : view.time}</span>;
  return view.timestamp_tooltip ? (
    <Tooltip text={view.timestamp_tooltip} singleLine>
      {label}
    </Tooltip>
  ) : (
    label
  );
}

function PendingClock() {
  return (
    <Tooltip text="Sending…" singleLine>
      <span className="tl-pending-clock">
        <Icon name="clock" size={12} />
      </span>
    </Tooltip>
  );
}

type BodyProps = {
  row: MessageRowModel;
  view: MessageView;
  thread: boolean;
  highlighted: boolean;
  hovered: boolean;
  mediaMaxWidth?: number;
  showThreadBadge: boolean;
  threadSummary: ThreadSummary | undefined;
  clippedParentAuthor: string | null;
  onMove: (event: React.PointerEvent) => void;
  hold: (event: React.PointerEvent) => void;
  openThread: (rootId: string) => void;
};

/** `render_message`: body, previews, attachments, reactions, strip, thread summary, send state. */
function MessageBody({
  row,
  view,
  thread,
  highlighted,
  hovered,
  mediaMaxWidth,
  showThreadBadge,
  threadSummary,
  clippedParentAuthor,
  onMove,
  hold,
  openThread,
}: BodyProps) {
  const message = row.message;
  const ref = useRef<HTMLDivElement>(null);
  const hover = useHover();
  const settled = hovered && !!hover?.settled;
  const uploading = view.attachments.some((attachment) => attachment.upload);
  const classes = [
    "tl-message",
    highlighted && "highlighted",
    message.send_state === "Pending" && !uploading && "pending",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div ref={ref} className={classes}>
      {!view.hide_body && <MessageBlocks blocks={view.blocks} emojiOnly={view.emoji_only} />}
      {message.link_previews.length > 0 && (
        <LinkPreviews
          previews={message.link_previews}
          hidden={view.hidden_previews}
          tightBottom={message.attachments.length === 0 && message.thread_reply_count === 0}
          mediaMaxWidth={mediaMaxWidth}
          onHover={onMove}
        />
      )}
      {view.attachments.length > 0 && (
        <Attachments
          messageId={message.id}
          thread={thread}
          attachments={view.attachments}
          caption={view.caption}
          mediaMaxWidth={mediaMaxWidth}
          roomy={showThreadBadge}
          onHover={onMove}
        />
      )}
      {view.edited_line && <div className="tl-edited-line">(edited)</div>}
      {view.reactions.length > 0 && (
        <div className="tl-reactions">
          {view.reactions.map((reaction) => (
            <ReactionTile key={reaction.emoji} messageId={message.id} reaction={reaction} hold={hold} />
          ))}
          {settled && (
            <Tooltip text="Add reaction" singleLine>
              <div
                className="tl-reaction-add"
                onClick={() => {
                  clearHoverNow();
                  send("overlays", "open_reaction_picker", { message_id: message.id });
                }}
              >
                <Icon name="plus" size={18} />
              </div>
            </Tooltip>
          )}
        </div>
      )}
      {settled && hover && (
        <HoverToolbar messageRef={ref} row={row} thread={thread} pointer={hover} />
      )}
      {showThreadBadge && message.thread_reply_count > 0 && (
        <div style={{ display: "flex", minWidth: 0 }}>
          <ThreadSummaryRow
            replyCount={message.thread_reply_count}
            summary={threadSummary}
            lastReply={view.last_reply}
            clippedParentAuthor={clippedParentAuthor}
            onClick={() => openThread(message.id)}
            onPointerMove={hold}
          />
        </div>
      )}
      {message.send_state === "Failed" && (
        <div className="tl-failed">
          <span style={{ display: "flex", color: "var(--danger)" }}>
            <Icon name="alert-triangle" size={12} />
          </span>
          <span className="label">Not sent</span>
          <span>·</span>
          <Tooltip text="Send this message again" singleLine>
            <span
              className="retry"
              onPointerMove={hold}
              onClick={() => send("composer", "retry_send", { message_id: message.id })}
            >
              Retry
            </span>
          </Tooltip>
          <span>·</span>
          <Tooltip text="Remove this unsent message" singleLine>
            <span
              className="discard"
              onPointerMove={hold}
              onClick={() => send("timeline", "discard_failed_send", { message_id: message.id })}
            >
              Delete
            </span>
          </Tooltip>
        </div>
      )}
    </div>
  );
}

function ReactionGlyph({ reaction }: { reaction: ReactionView }) {
  const [failed, setFailed] = useState(false);
  if (reaction.unicode) return <span className="tl-reaction-glyph">{reaction.unicode}</span>;
  if (reaction.asset && !failed) {
    return (
      <img
        src={assetUrl(reaction.asset)}
        width={16}
        height={16}
        style={{ objectFit: "contain", borderRadius: 2 }}
        onError={() => setFailed(true)}
        draggable={false}
        alt={reaction.emoji}
      />
    );
  }
  return <span style={{ fontSize: 12 }}>{reaction.emoji}</span>;
}

function ReactionTile({
  messageId,
  reaction,
  hold,
}: {
  messageId: string;
  reaction: ReactionView;
  hold: (event: React.PointerEvent) => void;
}) {
  const tile = (
    <div
      className={`tl-reaction${reaction.mine ? " mine" : ""}`}
      onPointerMove={hold}
      onClick={() => send("timeline", "react", { message_id: messageId, emoji: reaction.emoji })}
    >
      <ReactionGlyph reaction={reaction} />
      <span className="tl-reaction-count">{reaction.count}</span>
    </div>
  );
  return reaction.tooltip ? <Tooltip text={reaction.tooltip}>{tile}</Tooltip> : tile;
}

function ToolbarButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <Tooltip text={label} singleLine>
      <div
        className="tl-toolbar-button"
        onClick={() => {
          clearHoverNow();
          onClick();
        }}
      >
        {children}
      </div>
    </Tooltip>
  );
}

/** The quick-react strip and message actions, beside the pointer. */
function HoverToolbar({
  messageRef,
  row,
  thread,
  pointer,
}: {
  messageRef: React.RefObject<HTMLDivElement | null>;
  row: MessageRowModel;
  thread: boolean;
  pointer: { x: number; y: number };
}) {
  const timeline = useModel("timeline");
  const extra = useModel("timeline_extra");
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null);
  const message = row.message;
  const view = extra?.messages[message.id];
  const recent = timeline?.quick_react_recent ?? null;
  const recentButton = recent && (recent.unicode || recent.asset_path) ? recent : null;
  const own = view?.own ?? false;
  // Edits happen in the main composer, so the thread pane offers delete only.
  const canEdit = own && !thread && !message.event;
  const size = toolbarSize(QUICK_REACT_EMOJI.length, !!recentButton, (own ? 1 : 0) + (canEdit ? 1 : 0));

  useLayoutEffect(() => {
    const element = messageRef.current;
    // Selecting text hides the strip, as in gpui.
    if (!element || !window.getSelection()?.isCollapsed) {
      setPosition(null);
      return;
    }
    const rect = element.getBoundingClientRect();
    setPosition(
      placeToolbar(
        pointer,
        { left: rect.left, top: rect.top, width: rect.width },
        { width: window.innerWidth, height: window.innerHeight },
        size,
      ),
    );
  }, [pointer.x, pointer.y, size.width]);

  if (!position) return null;
  const react = (emoji: string) => send("timeline", "react", { message_id: message.id, emoji });
  return createPortal(
    <div
      className="tl-toolbar"
      style={{ left: position.x, top: position.y, width: size.width, height: size.height }}
      onPointerMove={(event) => {
        cancelHoverClear();
        event.stopPropagation();
      }}
      onPointerDown={cancelHoverClear}
      onPointerLeave={clearHoverSoon}
    >
      {QUICK_REACT_EMOJI.map(([alias, unicode]) => (
        <ToolbarButton key={alias} label={`:${alias}:`} onClick={() => react(unicode)}>
          {unicode}
        </ToolbarButton>
      ))}
      {recentButton && (
        <>
          <div className="tl-toolbar-separator" />
          <div
            className="tl-toolbar-button"
            onClick={() => {
              clearHoverNow();
              react(recentButton.unicode ?? `:${recentButton.alias}:`);
            }}
          >
            {recentButton.unicode ?? (
              <img src={assetUrl(recentButton.asset_path)} width={18} height={18} alt={recentButton.alias} />
            )}
          </div>
        </>
      )}
      <ToolbarButton label="Add reaction" onClick={() => send("overlays", "open_reaction_picker", { message_id: message.id })}>
        <Icon name="plus" size={18} />
      </ToolbarButton>
      <div className="tl-toolbar-separator" />
      <ToolbarButton label="Reply in thread" onClick={() => send("right_pane", "open_thread", { root_id: message.id })}>
        <Icon name="thread" size={18} />
      </ToolbarButton>
      <ToolbarButton label="Copy link" onClick={() => send("timeline", "copy_message_link", { message_id: message.id })}>
        <Icon name="copy" size={18} />
      </ToolbarButton>
      <ToolbarButton label="Copy text" onClick={() => send("timeline", "copy_message_text", { message_id: message.id })}>
        <Icon name="clipboard" size={18} />
      </ToolbarButton>
      {canEdit && (
        <ToolbarButton label="Edit" onClick={() => send("composer", "edit_message", { message_id: message.id })}>
          <Icon name="pencil" size={18} />
        </ToolbarButton>
      )}
      {own && (
        <ToolbarButton label="Delete" onClick={() => send("timeline", "delete_message", { message_id: message.id })}>
          <Icon name="trash" size={18} color="var(--danger)" />
        </ToolbarButton>
      )}
    </div>,
    document.body,
  );
}

/** "2 replies", or "2 replies to Alice Johnson" while the parent's author line is out of view. */
function threadSummaryLabel(replyCount: number, clippedParentAuthor: string | null): string {
  const noun = replyCount === 1 ? "reply" : "replies";
  const author = clippedParentAuthor?.trim();
  return author ? `${replyCount} ${noun} to ${author}` : `${replyCount} ${noun}`;
}

function ThreadSummaryRow({
  replyCount,
  summary,
  lastReply,
  clippedParentAuthor,
  onClick,
  onPointerMove,
}: {
  replyCount: number;
  summary: ThreadSummary | undefined;
  lastReply: string | null;
  clippedParentAuthor: string | null;
  onClick: () => void;
  onPointerMove: (event: React.PointerEvent) => void;
}) {
  const repliers = summary?.repliers ?? [];
  return (
    <div className="tl-thread-summary" onClick={onClick} onPointerMove={onPointerMove}>
      {repliers.length === 0 ? (
        <Icon name="thread" size={18} />
      ) : (
        <div className="tl-thread-faces">
          {repliers.map((replier) => (
            <Avatar key={replier.id} name={replier.display_name} asset={replier.avatar_asset} size={20} />
          ))}
        </div>
      )}
      <span className="tl-thread-label">{threadSummaryLabel(replyCount, clippedParentAuthor)}</span>
      {lastReply && <span className="tl-thread-last">{lastReply}</span>}
      <span className="tl-thread-view">View thread</span>
    </div>
  );
}

/** The width of a toolbar button, for callers laying out around the strip. */
export const TOOLBAR_BUTTON_SIZE = BUTTON_SIZE;
