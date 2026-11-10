/** Threads, members, details, files, search and profiles beside the conversation (views/right_pane, views/profile). */

import { useEffect, useRef, useState } from "react";
import { readModel, send, useModel } from "../core/engine";
import { useShortcuts } from "../core/shortcuts";
import { DetailsPane } from "./DetailsPane";
import { FilesPane } from "./FilesPane";
import { MembersPane } from "./MembersPane";
import { ProfileCard, ProfilePanel } from "./ProfilePanel";
import { SearchPane } from "./SearchPane";
import { ThreadPane } from "./ThreadPane";
import type {
  ConversationView,
  NavigationView,
  OverlayView,
  ProfilePanelModel,
  RightPaneMode,
  RightPaneState,
  SearchView,
  ThreadPaneModel,
} from "./types";
import "./right_pane.css";

/** views/mod.rs layout constants used by `shell_layout`. */
const RESIZE_HANDLE_WIDTH = 4;
const MAIN_PANEL_MIN_WIDTH = 560;

type Snapshot = {
  mode: RightPaneMode;
  thread: ThreadPaneModel;
  conversation: ConversationView;
  profile: ProfilePanelModel;
  state: RightPaneState;
};

const togglePane = (pane: string) => () => {
  send("right_pane", "toggle_pane", { pane });
};

/** Overlays that make Escape dismiss them instead of closing the pane (`dismiss_overlays_action`). */
function overlayOpen(): boolean {
  const overlay = readModel("overlay") as Record<string, unknown> | undefined;
  if (!overlay) return false;
  return Boolean(
    overlay.new_chat_open ||
      overlay.quick_switcher_open ||
      overlay.command_palette_open ||
      overlay.emoji_picker_open ||
      overlay.fullscreen_image ||
      overlay.file_upload_lightbox ||
      overlay.profile_card_user_id,
  );
}

export function RightPane() {
  const navigation = useModel("navigation") as NavigationView | undefined;
  const thread = useModel("thread_pane") as ThreadPaneModel | undefined;
  const conversation = useModel("conversation") as ConversationView | undefined;
  const profile = useModel("profile_panel") as ProfilePanelModel | undefined;
  const search = useModel("search") as SearchView | undefined;
  const state = useModel("right_pane_extra") as RightPaneState | undefined;
  const overlay = useModel("overlay") as OverlayView | undefined;
  const sidebar = useModel("sidebar") as { width_px?: number } | undefined;
  const [viewport, setViewport] = useState(window.innerWidth);
  const [dragWidth, setDragWidth] = useState<number | null>(null);
  const snapshot = useRef<Snapshot | null>(null);

  useEffect(() => {
    const onResize = () => setViewport(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useShortcuts([
    { keys: "cmd-shift-\\", handler: togglePane("thread") },
    // Shift turns "\" into "|" in KeyboardEvent.key.
    { keys: "cmd-shift-|", handler: togglePane("thread") },
    { keys: "cmd-shift-m", handler: togglePane("members") },
    { keys: "cmd-shift-d", handler: togglePane("details") },
    { keys: "cmd-shift-f", handler: togglePane("files") },
    {
      // `dismiss_overlays_action`: with nothing else to dismiss, Escape closes
      // the thread, profile and details panes; the profile card goes first.
      keys: "escape",
      handler: () => {
        const card = (readModel("overlay") as OverlayView | undefined)?.profile_card_user_id;
        if (card) {
          send("right_pane", "hide_profile_card");
          return true;
        }
        if (overlayOpen()) return false;
        const composer = readModel("composer") as { mode?: unknown; autocomplete?: unknown } | undefined;
        const threadModel = readModel("thread_pane") as ThreadPaneModel | undefined;
        if (composer?.autocomplete || threadModel?.reply_autocomplete) return false;
        if (composer?.mode && typeof composer.mode === "object" && "Edit" in composer.mode) return false;
        const mode = (readModel("navigation") as NavigationView | undefined)?.right_pane;
        if (mode === "Thread" || mode === "Details" || (typeof mode === "object" && mode && "Profile" in mode)) {
          send("right_pane", "close");
          return true;
        }
        return false;
      },
    },
  ]);

  const card =
    overlay?.profile_card_user_id && profile ? (
      <div className="rp-card-layer">
        <ProfileCard panel={profile} userId={overlay.profile_card_user_id} />
      </div>
    ) : null;

  if (!navigation || !thread || !conversation || !profile || !state) return card;

  let view: Snapshot = { mode: navigation.right_pane, thread, conversation, profile, state };
  if (view.mode !== "Hidden") {
    snapshot.current = view;
  } else if (state.window.closing && snapshot.current) {
    // The window is shrinking back: keep showing the closed pane until it has.
    view = snapshot.current;
  } else {
    snapshot.current = null;
    return card;
  }
  // The window is growing for the pane: keep the chat layout until it has.
  if (state.window.expanding) return card;

  const width = dragWidth ?? view.thread.width_px;
  const sidebarWidth = sidebar?.width_px ?? 0;
  const minShellWidth = sidebarWidth + RESIZE_HANDLE_WIDTH + MAIN_PANEL_MIN_WIDTH + width;
  const overlayLayout = viewport < minShellWidth && !state.window.closing;

  const beginResize = (event: React.MouseEvent) => {
    event.preventDefault();
    const anchor = event.clientX;
    const start = view.thread.width_px;
    let frame = 0;
    const move = (moveEvent: MouseEvent) => {
      const requested = start + (anchor - moveEvent.clientX);
      // `constrained_right_pane_width`, mirrored so the drag follows the pointer.
      const available = window.innerWidth - sidebarWidth - RESIZE_HANDLE_WIDTH - MAIN_PANEL_MIN_WIDTH;
      const next = Math.min(Math.max(requested, 280), Math.min(Math.max(available, 280), 520));
      setDragWidth(next);
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() =>
        send("right_pane", "set_width", { width_px: requested, viewport_width: window.innerWidth }),
      );
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.style.cursor = "";
      setDragWidth(null);
    };
    document.body.style.cursor = "col-resize";
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  let content = null;
  const mode = view.mode;
  if (mode === "Thread") content = <ThreadPane thread={view.thread} state={view.state} />;
  else if (mode === "Details") content = <DetailsPane conversation={view.conversation} state={view.state} />;
  else if (mode === "Members") content = <MembersPane conversation={view.conversation} state={view.state} />;
  else if (mode === "Files") content = <FilesPane state={view.state} width={width} />;
  else if (mode === "Search") content = search ? <SearchPane search={search} state={view.state} /> : null;
  else if (typeof mode === "object") content = <ProfilePanel panel={view.profile} />;

  return (
    <>
      <aside className={`rp-host${overlayLayout ? " rp-overlay" : ""}`} style={{ width }}>
        <div className="rp-resize" onMouseDown={beginResize}>
          <span />
        </div>
        <div className="rp-content">{content}</div>
      </aside>
      {card}
    </>
  );
}
