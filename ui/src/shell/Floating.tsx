// Window-level layers: splash (views/splash), toasts, the software update
// panel (views/app_window/updates.rs) and the Keybase inspector.

import { useEffect } from "react";
import { send } from "../core/engine";
import { IS_MAC } from "../core/shortcuts";
import { Badge } from "./Pages";
import appIcon from "./app-icon.png";
import type { NotificationsModel, ShellState, ToastAction } from "./types";

function Keycap({ label }: { label: string }) {
  return (
    <div
      style={{
        minWidth: 38,
        height: 38,
        padding: "0 10px",
        borderRadius: 8,
        background: "var(--splash-kbd-bg)",
        border: "1px solid var(--splash-kbd-border)",
        boxShadow: "0 2px 4px var(--splash-kbd-shadow)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        color: "var(--text-primary)",
        fontSize: 16,
        fontWeight: 500,
      }}
    >
      {label}
    </div>
  );
}

/** `SplashView`: swallows every press and scroll until the app is ready. */
export function Splash({ status }: { status: string }) {
  const swallow = (event: React.SyntheticEvent) => event.stopPropagation();
  return (
    <div
      style={{ position: "absolute", inset: 0, zIndex: 100, display: "flex", alignItems: "center", justifyContent: "center", background: "var(--app-bg)" }}
      onMouseDown={swallow}
      onWheel={swallow}
      data-tauri-drag-region
    >
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 32 }}>
        <img src={appIcon} width={192} height={192} alt="" draggable={false} />
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 12 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <Keycap label={IS_MAC ? "⌘" : "Ctrl"} />
            <Keycap label="K" />
          </div>
          <div style={{ fontSize: 14, color: "var(--splash-subtitle)" }}>conversations · people · messages</div>
          <div style={{ height: 18, fontSize: 12, color: "var(--splash-status)" }}>{status}</div>
        </div>
      </div>
    </div>
  );
}

const TOAST_LABEL: Record<ToastAction, string> = {
  OpenPreferences: "Open Settings",
  FocusComposer: "Jump to composer",
  OpenThread: "Open thread",
  FocusThreadReply: "Reply in thread",
  OpenCurrentConversation: "View conversation",
  OpenActiveCall: "Open call",
};
const TOAST_DURATION_MS = 5000;

/**
 * `NotificationsModel::toasts` with `activate_toast`. gpui keeps toasts in the
 * model (at most three) but never draws them; here they show bottom-right and
 * leave after five seconds.
 */
export function Toasts({ notifications }: { notifications: NotificationsModel }) {
  const count = notifications.toasts.length;
  useEffect(() => {
    if (count === 0) return;
    const timer = window.setTimeout(() => send("shell", "dismiss_toast", { index: 0 }), TOAST_DURATION_MS);
    return () => window.clearTimeout(timer);
  }, [count, notifications.toasts[0]?.title]);
  if (count === 0) return null;
  return (
    <div className="toast-stack">
      {notifications.toasts.map((toast, index) => (
        <div key={`${index}-${toast.title}`} className="floating-panel toast" role="status">
          <span style={{ flex: 1 }}>{toast.title}</span>
          {toast.action && (
            <button className="text-button" onClick={() => send("shell", "activate_toast", { index })}>
              {TOAST_LABEL[toast.action]}
            </button>
          )}
          <button style={{ color: "var(--text-secondary)" }} title="Dismiss" onClick={() => send("shell", "dismiss_toast", { index })}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}

type PanelCopy = [title: string, description: string, primary: string, action: string | null];

function panelCopy(state: string, message: string): PanelCopy {
  switch (state) {
    case "available":
      return ["A new zBase is available", "Keep chatting while the update downloads.", "Download update", "download"];
    case "downloaded":
      return ["Ready to prepare the update", "macOS may ask your permission before preparing the update.", "Prepare update", "download"];
    case "downloading":
      return ["Downloading update", "You can keep using zBase.", "Cancel download", "cancel"];
    case "preparing":
      return ["Preparing update…", "macOS may ask permission to update this application.", "", null];
    case "ready":
      return ["Ready when you are", "Restart now, or finish updating when you quit. Your drafts will be restored.", "Restart and update", "restart"];
    case "installing":
      return ["Installing update…", "zBase will reopen shortly.", "", null];
    case "checking":
      return ["Checking for updates…", "", "Cancel", "cancel"];
    case "current":
      return ["No update available", message, "Check again", "check"];
    case "error":
      return ["Couldn’t finish the update", message, "Try again", "check"];
    case "disabled":
      return ["Updates", message, "View releases", null];
    default:
      return ["Updates", "Check for a new version of zBase.", "Check now", "check"];
  }
}

/** `render_update_panel`, anchored under the sidebar toolbar. */
export function UpdatePanel({ shell, sidebarWidth }: { shell: ShellState; sidebarWidth: number }) {
  const { snapshot, notes_open: notesOpen, error } = shell.update;
  const [title, description, primary, action] = panelCopy(snapshot.state, snapshot.message);
  const progress = snapshot.total > 0 ? Math.min(1, Math.max(0, snapshot.received / snapshot.total)) : null;
  const busy = ["downloading", "preparing", "checking", "installing"].includes(snapshot.state);
  const command = (name: string) => send("shell", "update_command", { action: name });
  const openUrl = (url: string) => send("shell", "open_url", { url });
  return (
    <div style={{ position: "absolute", inset: 0, zIndex: 60 }} onClick={() => send("shell", "close_updates")}>
      <div style={{ position: "absolute", top: 48, left: 8, maxWidth: Math.max(280, sidebarWidth + 160) }}>
        <div
          className="floating-panel"
          style={{ width: 360, maxWidth: "100%", padding: 16, display: "flex", flexDirection: "column", gap: 12, fontSize: 14 }}
          onClick={(event) => event.stopPropagation()}
        >
          <div style={{ fontWeight: 600 }}>{title}</div>
          <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>
            Installed: {shell.version}
            {snapshot.version && ` · Available: ${snapshot.version}`}
          </div>
          <div>{description}</div>
          {snapshot.state === "downloading" && progress !== null && (
            <>
              <div style={{ height: 4, width: "100%", borderRadius: 999, background: "var(--subtle-surface)" }}>
                <div style={{ height: "100%", width: `${progress * 100}%`, background: "var(--accent)" }} />
              </div>
              <div>
                {(snapshot.received / 1_000_000).toFixed(1)} / {(snapshot.total / 1_000_000).toFixed(1)} MB
              </div>
            </>
          )}
          {snapshot.notes && (
            <>
              <button className="text-button" style={{ textAlign: "left" }} onClick={() => send("shell", "toggle_update_notes")}>
                {notesOpen ? "▾ What’s new" : "▸ What’s new"}
              </button>
              {notesOpen && (
                <div style={{ maxHeight: 220, overflowY: "auto", whiteSpace: "pre-wrap", userSelect: "text", fontSize: 13 }}>
                  {snapshot.notes}
                </div>
              )}
            </>
          )}
          {snapshot.version && (
            <button
              className="text-button"
              style={{ textAlign: "left" }}
              onClick={() => openUrl(`https://github.com/cameroncooper/zbase/releases/tag/v${snapshot.version}`)}
            >
              View release on GitHub ↗
            </button>
          )}
          {notesOpen && ["available", "downloaded"].includes(snapshot.state) && !snapshot.informational && (
            <button style={{ textAlign: "left", fontSize: 12, color: "var(--text-secondary)" }} onClick={() => command("skip")}>
              Skip this version
            </button>
          )}
          {error && <div style={{ color: "var(--danger)" }}>{error}</div>}
          <div style={{ display: "flex", flexWrap: "wrap", justifyContent: "flex-end", gap: 8 }}>
            <button className="update-button" onClick={() => command("later")}>
              {busy ? "Hide" : "Later"}
            </button>
            {action ? (
              <button className="update-button primary" onClick={() => command(action)}>
                {snapshot.informational ? "View release" : primary}
              </button>
            ) : (
              snapshot.state === "disabled" && (
                <button onClick={() => openUrl("https://github.com/cameroncooper/zbase/releases")}>{primary}</button>
              )
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/** `format_keybase_inspector_export`. */
function inspectorExport(shell: ShellState): string {
  const { inspector } = shell;
  const lines = [
    `generated_unix_ms=${Date.now()}`,
    `paused=${inspector.paused}`,
    `unknown_only=${inspector.unknown_only}`,
    `seq=${inspector.seq}`,
    `rows=${inspector.rows.length}`,
    "method\tcount\tlast_seen_seq\tpayload_preview",
  ];
  for (const row of inspector.rows) {
    const payload = (row.payload_preview ?? "—").replace(/\n/g, "\\n").replace(/\t/g, "  ");
    lines.push(`${row.method}\t${row.count}\t${row.last_seen_seq}\t${payload}`);
  }
  return lines.join("\n");
}

/** `render_keybase_inspector_modal` (Cmd+Shift+I). */
export function KeybaseInspector({ shell }: { shell: ShellState }) {
  const { inspector } = shell;
  const button = (label: string, onClick: () => void, foreground = "var(--text-primary)") => (
    <button onClick={onClick}>
      <Badge label={label} background="var(--panel-alt-bg)" foreground={foreground} />
    </button>
  );
  const copy = () =>
    navigator.clipboard
      .writeText(inspectorExport(shell))
      .then(() => send("shell", "inspector_copied", { count: inspector.rows.length }));
  const column = { method: 380, count: 80, seen: 110 };
  return (
    <div
      style={{ position: "absolute", inset: 0, zIndex: 70, display: "flex", alignItems: "center", justifyContent: "center", background: "rgba(0,0,0,0.45)" }}
      onClick={() => send("shell", "close_keybase_inspector")}
    >
      <div
        style={{
          width: 860,
          height: 560,
          borderRadius: 12,
          border: "1px solid var(--shell-border-strong)",
          background: "var(--panel-bg)",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
        }}
        onClick={(event) => event.stopPropagation()}
      >
        <div style={{ padding: "12px 16px", borderBottom: "1px solid var(--shell-border-strong)", display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div style={{ display: "flex", flexDirection: "column" }}>
            <div style={{ fontSize: 14 }}>Keybase Event Inspector</div>
            <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>{inspector.method_count} methods tracked</div>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            {button(inspector.paused ? "Resume" : "Pause", () => send("shell", "inspector_toggle_pause"))}
            {button(inspector.unknown_only ? "Showing Unknown" : "Show Unknown Only", () => send("shell", "inspector_toggle_unknown_only"))}
            {button("Clear", () => send("shell", "inspector_clear"))}
            {button("Copy", copy)}
            {button("Close", () => send("shell", "close_keybase_inspector"), "var(--text-secondary)")}
          </div>
        </div>
        <div style={{ display: "flex", padding: "8px 16px", gap: 16, borderBottom: "1px solid var(--shell-border-strong)", fontSize: 12, color: "var(--text-secondary)" }}>
          <div style={{ width: column.method }}>Method</div>
          <div style={{ width: column.count }}>Count</div>
          <div style={{ width: column.seen }}>Last Seen</div>
          <div style={{ flex: 1 }}>Payload Preview</div>
        </div>
        <div style={{ flex: 1, overflowY: "auto", padding: "8px 16px", display: "flex", flexDirection: "column", gap: 4, userSelect: "text" }}>
          {inspector.rows.map((entry) => (
            <div key={entry.method} className="inspector-row" style={{ width: "100%", padding: 8, borderRadius: 6, display: "flex", gap: 16, fontSize: 12 }}>
              <div style={{ width: column.method, flexShrink: 0, overflowWrap: "anywhere" }}>{entry.method}</div>
              <div style={{ width: column.count, flexShrink: 0 }}>{entry.count}</div>
              <div style={{ width: column.seen, flexShrink: 0, color: "var(--text-secondary)" }}>#{entry.last_seen_seq}</div>
              <div style={{ flex: 1, minWidth: 0, color: "var(--text-secondary)", overflowWrap: "anywhere" }}>{entry.payload_preview ?? "—"}</div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
