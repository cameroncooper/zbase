// Every overlay and modal layered over the window (`OverlayHost::render`,
// src/views/overlays/mod.rs), plus the window-level keys AppWindow routes to
// them: Cmd+K, Cmd+Shift+P, Cmd+N, Cmd+Return, Cmd+F, Cmd+J, Escape, Up/Down,
// Tab/Shift+Tab in the switcher, Return and Cmd+1..5 over the switcher.

import { readModel, send, useModel } from "../core/engine";
import { useShortcuts } from "../core/shortcuts";
import { Badge } from "../ui/common";
import { CommandPalette } from "./CommandPalette";
import { EmojiPicker } from "./EmojiPicker";
import { Lightbox } from "./Lightbox";
import { restoreChatFocus } from "./MessageHit";
import { NewChat } from "./NewChat";
import { QuickSwitcher } from "./QuickSwitcher";
import "./overlays.css";
import type { OverlayModel } from "./types";

export { FindBar } from "./FindBar";

function activeInput(): string | null {
  const element = document.activeElement as HTMLElement | null;
  return element?.dataset?.overlayInput ?? null;
}

const findInput = () => document.querySelector('[data-overlay-input="find-in-chat"]');
const switcherInput = () => document.querySelector('[data-overlay-input="quick-switcher"]');

function inTextArea(): boolean {
  const element = document.activeElement;
  return element instanceof HTMLTextAreaElement || (element as HTMLElement | null)?.isContentEditable === true;
}

function overlay(): OverlayModel | undefined {
  return readModel("overlay") as OverlayModel | undefined;
}

function onSearchRoute(): boolean {
  const route = readModel("navigation")?.current;
  return typeof route === "object" && route !== null && "Search" in route;
}

function anyOverlayOpen(model = overlay()): boolean {
  return (
    !!model &&
    (model.new_chat_open ||
      model.quick_switcher_open ||
      model.command_palette_open ||
      model.emoji_picker_open ||
      model.fullscreen_image !== null ||
      model.file_upload_lightbox !== null ||
      model.profile_card_user_id !== null)
  );
}

/**
 * Escape, `dismiss_overlays_action`. Popups that take Escape first in gpui
 * (update panel, composer autocomplete, format menu) mark themselves with
 * `data-escape-layer`; while one is in the DOM this binding steps aside.
 */
function dismissOverlaysKey(): boolean {
  if (document.querySelector("[data-escape-layer]")) return false;
  const composer = readModel("composer") as { autocomplete?: unknown } | undefined;
  const thread = readModel("thread_pane") as { autocomplete?: unknown } | undefined;
  if ((composer?.autocomplete || thread?.autocomplete) && inTextArea()) return false;
  const hadOverlay = anyOverlayOpen();
  send("overlays", "dismiss_overlays_action");
  if (hadOverlay) restoreChatFocus();
  else (document.activeElement as HTMLElement | null)?.blur?.();
  return true;
}

export function Overlays() {
  const model = useModel("overlay") as OverlayModel | undefined;

  useShortcuts([
    {
      keys: "cmd-k",
      handler: () => {
        const opening = !overlay()?.quick_switcher_open;
        send("overlays", "toggle_quick_switcher");
        if (!opening) restoreChatFocus();
      },
    },
    { keys: "cmd-shift-p", handler: () => void send("overlays", "toggle_command_palette") },
    { keys: "cmd-n", handler: () => void send("overlays", "open_new_chat") },
    {
      keys: "cmd-enter",
      when: () => !!overlay()?.new_chat_open,
      handler: () => void send("overlays", "start_new_chat"),
    },
    { keys: "cmd-f", handler: () => void send("overlays", "toggle_find_in_chat") },
    { keys: "cmd-j", handler: () => void send("overlays", "submit_search") },
    { keys: "escape", handler: dismissOverlaysKey },
    // FindInChatTextField: its own Return, Shift+Return and Escape.
    { keys: "enter", within: findInput, handler: () => void send("overlays", "find_next") },
    { keys: "shift-enter", within: findInput, handler: () => void send("overlays", "find_previous") },
    {
      keys: "escape",
      within: findInput,
      handler: () => {
        send("overlays", "close_find_in_chat");
        restoreChatFocus();
      },
    },
    // SelectPrevious / SelectNext for the switcher, new chat and Search.
    {
      keys: "up",
      when: () => !!overlay()?.quick_switcher_open || !!overlay()?.new_chat_open || (onSearchRoute() && !inTextArea()),
      handler: () => void send("overlays", "select", { direction: -1 }),
    },
    {
      keys: "down",
      when: () => !!overlay()?.quick_switcher_open || !!overlay()?.new_chat_open || (onSearchRoute() && !inTextArea()),
      handler: () => void send("overlays", "select", { direction: 1 }),
    },
    {
      keys: "tab",
      within: switcherInput,
      handler: () => void send("overlays", "select", { direction: 1 }),
    },
    {
      keys: "shift-tab",
      within: switcherInput,
      handler: () => void send("overlays", "select", { direction: -1 }),
    },
    // ConfirmPrimary for the switcher, new chat and Search.
    {
      keys: "enter",
      when: () =>
        overlay()?.file_upload_lightbox == null &&
        (!!overlay()?.quick_switcher_open ||
          !!overlay()?.new_chat_open ||
          activeInput() === "search" ||
          (onSearchRoute() && !inTextArea() && activeInput() === null)),
      handler: () => {
        if (activeInput() === "search" && !overlay()?.quick_switcher_open && !overlay()?.new_chat_open) {
          send("overlays", "confirm_search");
        } else {
          send("overlays", "confirm_primary");
        }
      },
    },
    // Cmd+1..5 open the switcher's recents while it is open with no query.
    ...[1, 2, 3, 4, 5].map((digit) => ({
      keys: `cmd-${digit}`,
      when: () => !!overlay()?.quick_switcher_open && (readModel("quick_switcher")?.query ?? "").trim() === "",
      handler: () => void send("overlays", "open_quick_switcher_recent", { recent_index: digit - 1 }),
    })),
  ]);

  if (!model) return null;
  const hasBackdrop =
    model.new_chat_open ||
    model.quick_switcher_open ||
    model.command_palette_open ||
    model.fullscreen_image !== null ||
    model.file_upload_lightbox !== null;
  const hasCorner = model.active_modal !== null || model.emoji_picker_open;
  if (!hasBackdrop && !hasCorner) return null;

  if (hasBackdrop) {
    // The file upload lightbox shares this backdrop but is the composer area's.
    return (
      <div
        className="ov-root ov-backdrop"
        onClick={() => send("overlays", "dismiss_overlays")}
        onWheel={(event) => event.stopPropagation()}
      >
        {model.new_chat_open && <NewChat />}
        {model.quick_switcher_open && <QuickSwitcher />}
        {model.command_palette_open && <CommandPalette />}
        {model.fullscreen_image && <Lightbox image={model.fullscreen_image} />}
      </div>
    );
  }

  return (
    <div className="ov-root ov-corner">
      <div style={{ display: "flex", gap: 8 }}>
        {model.active_modal !== null && (
          <div className="ov-modal-chip">
            {model.active_modal}
            <Badge
              background="var(--panel-alt-bg)"
              foreground="var(--text-primary)"
              onClick={() => send("overlays", "dismiss_overlays")}
            >
              Close
            </Badge>
          </div>
        )}
        {model.emoji_picker_open && <EmojiPicker />}
      </div>
    </div>
  );
}
