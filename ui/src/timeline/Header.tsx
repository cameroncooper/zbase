// `ConversationHeader` and the strips under it (views/conversation/mod.rs):
// title, subtitle, badge, pane toggles, the connection banner and the pinned banner.

import { useEffect, useState } from "react";
import { send, useModel } from "../core/engine";
import { Avatar, ParticipantsAvatar } from "../ui/Avatar";
import { Icon, type IconName } from "../ui/Icon";
import { Tooltip, shortcutGlyphs } from "../ui/Tooltip";

type Action = {
  icon: IconName;
  label: string;
  shortcut: string;
  count?: number;
  active: boolean;
  onClick: () => void;
};

function useWindowActive(): boolean {
  const [active, setActive] = useState(document.hasFocus());
  useEffect(() => {
    const on = () => setActive(true);
    const off = () => setActive(false);
    window.addEventListener("focus", on);
    window.addEventListener("blur", off);
    return () => {
      window.removeEventListener("focus", on);
      window.removeEventListener("blur", off);
    };
  }, []);
  return active;
}

export function ConversationHeader() {
  const conversation = useModel("conversation");
  const extra = useModel("timeline_extra");
  const navigation = useModel("navigation") as { right_pane?: unknown } | undefined;
  const find = useModel("find_in_chat") as { open?: boolean } | undefined;
  const windowActive = useWindowActive();
  if (!conversation || !extra) return <div className="tl-header" />;
  const header = extra.header;
  const pane = navigation?.right_pane;
  const isChannel = header.kind === "channel";
  const isDm = header.kind === "dm";
  const togglePane = (name: string) => send("right_pane", "toggle_pane", { pane: name });

  const actions: Action[] = [
    {
      icon: "search",
      label: "Find in conversation",
      shortcut: "⌘F",
      active: !!find?.open,
      onClick: () => send("overlays", "toggle_find_in_chat"),
    },
  ];
  if (!isDm) {
    actions.push({
      icon: "members",
      label: "Members",
      shortcut: "⇧⌘M",
      count: header.member_count > 0 ? header.member_count : undefined,
      active: pane === "Members",
      onClick: () => togglePane("members"),
    });
  }
  actions.push({
    icon: "files",
    label: "Files",
    shortcut: "⇧⌘F",
    active: pane === "Files",
    onClick: () => togglePane("files"),
  });
  if (isChannel) {
    actions.push({
      icon: "info",
      label: "Conversation details",
      shortcut: "⇧⌘D",
      active: pane === "Details",
      onClick: () => togglePane("details"),
    });
  }

  const badge = isChannel ? (
    <Avatar name={header.team_name} asset={conversation.avatar_asset} size={32} team />
  ) : (
    <ParticipantsAvatar
      participants={conversation.participant_avatars}
      fallbackName={conversation.summary.title}
      size={32}
      surface="var(--main-bg)"
    />
  );
  const titleBlock = (
    <div className={`tl-header-title${isChannel || isDm ? " clickable" : ""}`}>
      {badge}
      <div className="tl-header-text">
        <div className="tl-header-name-row">
          <span className="tl-header-name">{header.title}</span>
          {(isChannel || isDm) && <Icon name="chevron-down" size={14} />}
        </div>
        {header.subtitle && <div className="tl-header-subtitle">{header.subtitle}</div>}
      </div>
    </div>
  );
  const title = isChannel ? (
    <Tooltip text="Conversation details" shortcut={shortcutGlyphs("⇧⌘D")}>
      <div style={{ flex: 1, minWidth: 0, display: "flex" }} onClick={() => togglePane("details")}>
        {titleBlock}
      </div>
    </Tooltip>
  ) : isDm ? (
    <Tooltip text="View profile" singleLine>
      <div style={{ flex: 1, minWidth: 0, display: "flex" }} onClick={() => send("right_pane", "open_dm_header_profile")}>
        {titleBlock}
      </div>
    </Tooltip>
  ) : (
    <div style={{ flex: 1, minWidth: 0, display: "flex" }}>{titleBlock}</div>
  );

  return (
    <div className={`tl-header${windowActive ? "" : " tl-window-inactive"}`}>
      {title}
      <div className="tl-header-actions">
        {actions.map((action) => (
          <Tooltip key={action.label} text={action.label} shortcut={shortcutGlyphs(action.shortcut)}>
            <div
              className={`tl-header-action${action.active ? " active" : ""}${action.count !== undefined ? " with-count" : ""}`}
              onClick={action.onClick}
            >
              <Icon name={action.icon} size={16} />
              {action.count !== undefined && <span className="tl-header-count">{action.count}</span>}
            </div>
          </Tooltip>
        ))}
      </div>
    </div>
  );
}

/** Amber strip while Keybase is unreachable (`connection_banner`). */
export function ConnectionBanner() {
  const extra = useModel("timeline_extra");
  const banner = extra?.connection_banner;
  if (!banner) return null;
  return (
    <div className="tl-connection-banner">
      <Icon name={banner.offline ? "cloud-off" : "info"} size={14} />
      <strong>{banner.title}</strong>
      <span style={{ flex: 1, minWidth: 0 }}>{banner.detail}</span>
    </div>
  );
}

/** `render_pinned_banner`: jump to the pinned message, or dismiss it. */
export function PinnedBanner() {
  const extra = useModel("timeline_extra");
  const pinned = extra?.pinned;
  if (!pinned) return null;
  return (
    <div className="tl-pinned-banner">
      <div className="tl-pinned-open" onClick={() => send("timeline", "jump_to_pinned")}>
        <Icon name="pin" size={18} />
        <span className="tl-pinned-text">{pinned.text}</span>
        {pinned.unavailable && <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>Unavailable</span>}
      </div>
      <div className="tl-round-button" onClick={() => send("timeline", "dismiss_pinned_banner")}>
        <Icon name="x" size={14} />
      </div>
    </div>
  );
}
