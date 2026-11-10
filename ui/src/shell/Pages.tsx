// Main-panel pages the shell owns: workspace home (views/home), activity
// (views/inbox) and calls (views/calls).

import { send, useModel } from "../core/engine";
import { routeForSummary } from "./routes";
import type { ActivityItem, CallModel, ConversationSummary } from "./types";

/** `views::badge`. */
export function Badge({ label, background, foreground }: { label: string; background: string; foreground: string }) {
  return (
    <span className="badge" style={{ background, color: foreground }}>
      {label}
    </span>
  );
}

/** `WorkspaceHomeView`. */
export function HomeView() {
  const app = useModel("app");
  const workspace = useModel("workspace");
  const notifications = useModel("notifications");
  if (!app || !workspace) return null;
  const card = (conversation: ConversationSummary, key: string) => (
    <button
      key={key}
      className="home-card"
      onClick={() => send("shell", "navigate", { route: routeForSummary(conversation, workspace.workspace_id) })}
    >
      <div style={{ display: "flex", flexDirection: "column", gap: 4, minWidth: 0 }}>
        <div style={{ fontWeight: 500, color: "var(--text-primary)" }}>{conversation.title}</div>
        <div className="secondary-text">{conversation.topic}</div>
      </div>
      <div style={{ display: "flex", gap: 8 }}>
        <Badge label={`${conversation.unread_count} unread`} background="var(--panel-bg)" foreground="var(--text-primary)" />
        <Badge label={`${conversation.mention_count} mentions`} background="var(--panel-bg)" foreground="var(--text-primary)" />
      </div>
    </button>
  );
  return (
    <div className="page-card">
      <div style={{ flex: 1, overflowY: "auto", padding: "24px 16px 24px 24px", display: "flex", flexDirection: "column", gap: 24 }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            <div className="page-card-title">{workspace.workspace_name}</div>
            <div className="secondary-text">Slack-like shell status and quick routes</div>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <Badge label={app.connectivity} background="var(--accent-soft)" foreground="var(--text-primary)" />
            <Badge label={`${app.global_unread_count} unread`} background="var(--panel-alt-bg)" foreground="var(--text-primary)" />
            <Badge
              label={`${notifications?.notification_center_count ?? 0} inbox`}
              background="var(--panel-alt-bg)"
              foreground="var(--text-primary)"
            />
          </div>
        </div>
        <div style={{ display: "flex", gap: 12 }}>
          <button onClick={() => send("overlays", "submit_search")}>
            <Badge label="Search" background="var(--accent)" foreground="var(--panel-bg)" />
          </button>
          <button onClick={() => send("shell", "show_activity")}>
            <Badge label="Inbox" background="var(--panel-alt-bg)" foreground="var(--text-primary)" />
          </button>
          <button onClick={() => send("shell", "open_preferences")}>
            <Badge label="Settings" background="var(--panel-alt-bg)" foreground="var(--text-primary)" />
          </button>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <div className="page-card-title">Channels</div>
          {workspace.channels.map((channel, index) => card(channel, `home-channel-${index}`))}
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <div className="page-card-title">Direct messages</div>
          {workspace.direct_messages.map((conversation, index) => card(conversation, `home-dm-${index}`))}
        </div>
      </div>
    </div>
  );
}

const KIND_BADGE: Record<ActivityItem["kind"], [string, string, string]> = {
  Mention: ["Mention", "var(--warning-soft)", "var(--warning)"],
  ThreadReply: ["Thread", "var(--success-soft)", "var(--success)"],
  Reaction: ["Reaction", "var(--accent-soft)", "var(--accent)"],
  Reminder: ["Reminder", "var(--panel-alt-bg)", "var(--text-primary)"],
};

/** `InboxView`. */
export function ActivityView() {
  const notifications = useModel("notifications");
  if (!notifications) return null;
  return (
    <div className="page-card" style={{ padding: 24, gap: 16 }}>
      <div style={{ fontWeight: 600 }}>Inbox</div>
      <div>
        <Badge
          label={`${notifications.notification_center_count} unread items`}
          background="var(--panel-alt-bg)"
          foreground="var(--warning)"
        />
      </div>
      <div className="secondary-text" style={{ borderRadius: 8, background: "var(--panel-alt-bg)", padding: 16 }}>
        Mentions, thread replies, reactions, and reminders will land here.
      </div>
      <div style={{ flex: 1, overflowY: "auto", paddingRight: 8, display: "flex", flexDirection: "column" }}>
        {notifications.activity_items.map((item, index) => {
          const highlighted = notifications.highlighted_index === index;
          const [label, background, foreground] = KIND_BADGE[item.kind];
          return (
            <button
              key={index}
              className="inbox-item"
              style={{
                borderColor: highlighted ? "var(--accent)" : "var(--border)",
                background: highlighted ? "color-mix(in srgb, var(--accent-soft) 80%, transparent)" : "var(--panel-bg)",
              }}
              onClick={() => send("shell", "open_activity_item", { index })}
            >
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <Badge label={label} background={background} foreground={foreground} />
                  <span style={{ fontWeight: 500, color: "var(--text-primary)" }}>{item.title}</span>
                </div>
                <div className="secondary-text">{item.detail}</div>
              </div>
              {item.unread ? (
                <Badge label="Unread" background="var(--success-soft)" foreground="var(--success)" />
              ) : (
                <Badge label="Viewed" background="var(--panel-alt-bg)" foreground="var(--text-secondary)" />
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** `CallWindow`. */
export function CallWindow() {
  const call = useModel("call");
  const status = call?.active_call?.status ?? "Idle";
  const participants = call?.active_call?.participants ?? [];
  const description =
    status === "SharingScreen"
      ? "Screen share is active."
      : status === "Reconnecting"
        ? "Reconnecting media session…"
        : status === "ActiveVideo"
          ? "Video call is active."
          : "Audio call is active.";
  return (
    <div className="page-card" style={{ padding: 24, gap: 16 }}>
      <div style={{ fontWeight: 600 }}>Call window</div>
      <div>
        <Badge label={status} background="var(--panel-alt-bg)" foreground="var(--text-primary)" />
      </div>
      <div className="secondary-text" style={{ borderRadius: 8, background: "var(--panel-alt-bg)", padding: 16 }}>
        {description}
      </div>
      <div style={{ display: "flex", gap: 12 }}>
        {participants.map((participant) => (
          <div key={participant.id} style={{ borderRadius: 8, background: "var(--panel-alt-bg)", padding: "8px 12px", fontSize: 14 }}>
            {participant.display_name}
          </div>
        ))}
      </div>
    </div>
  );
}

/** `MiniCallDock`, shown while a call is active. */
export function MiniCallDock({ call }: { call: CallModel }) {
  const button = (label: string, type: string, primary = false) => (
    <button onClick={() => send("shell", type)}>
      <Badge
        label={label}
        background={primary ? "var(--accent)" : "var(--panel-bg)"}
        foreground={primary ? "var(--panel-bg)" : "var(--text-primary)"}
      />
    </button>
  );
  return (
    <div
      style={{
        margin: "0 16px 16px",
        borderRadius: 8,
        background: "var(--panel-alt-bg)",
        padding: "12px 16px",
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        Mini call dock
        <Badge label={`muted=${call.is_muted}`} background="var(--panel-bg)" foreground="var(--text-primary)" />
        <Badge label={`sharing=${call.is_sharing_screen}`} background="var(--panel-bg)" foreground="var(--text-primary)" />
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        {button("Open", "start_or_open_call")}
        {button("Mute", "toggle_call_mute")}
        {button("Share", "toggle_call_screen_share")}
        {button("Leave", "leave_call", true)}
      </div>
    </div>
  );
}
