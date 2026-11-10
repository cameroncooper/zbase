// The hovered message and its quick-react strip: `AppWindow::set_hovered_message_with_cursor_anchor`,
// `hover_message_without_toolbar`, `show_toolbar_immediately`, the 200ms settle
// and the 300ms delayed clear, kept in the web view since they only drive drawing.
// Also `quick_react_placement.rs`, which places the strip beside the pointer.

import { useSyncExternalStore } from "react";
import { readModel } from "../core/engine";

export type Hover = {
  id: string;
  thread: boolean;
  /** Pointer in window coordinates, frozen once the strip settled. */
  x: number;
  y: number;
  settled: boolean;
};

const HOVER_SETTLE_DELAY_MS = 200;
const HOVER_CLEAR_DELAY_MS = 300;

let hover: Hover | null = null;
const listeners = new Set<() => void>();
let settleTimer: number | undefined;
let clearTimer: number | undefined;

function set(next: Hover | null) {
  hover = next;
  listeners.forEach((notify) => notify());
}

/** `timeline_interactions_blocked_by_overlay`. */
function blocked(): boolean {
  const overlay = readModel("overlay") as Record<string, unknown> | undefined;
  if (!overlay) return false;
  return Boolean(
    overlay.quick_switcher_open ||
      overlay.command_palette_open ||
      overlay.emoji_picker_open ||
      overlay.fullscreen_image ||
      overlay.file_upload_lightbox ||
      overlay.profile_card_user_id,
  );
}

function scheduleSettle() {
  window.clearTimeout(settleTimer);
  settleTimer = window.setTimeout(() => {
    if (hover && !hover.settled) set({ ...hover, settled: true });
  }, HOVER_SETTLE_DELAY_MS);
}

export function cancelHoverClear() {
  window.clearTimeout(clearTimer);
  clearTimer = undefined;
}

/** Pointer moved over a message body. */
export function hoverMove(id: string, thread: boolean, x: number, y: number) {
  if (blocked()) return clearHoverNow();
  cancelHoverClear();
  if (!hover || hover.id !== id || hover.thread !== thread) {
    set({ id, thread, x, y, settled: false });
    scheduleSettle();
  } else if (!hover.settled) {
    // Until the strip appears it follows the pointer; then it stays put.
    set({ ...hover, x, y });
    scheduleSettle();
  }
}

/** Over a name, quote or reaction with its own tooltip: highlighted, no strip. */
export function hoverWithoutToolbar(id: string, thread: boolean) {
  if (blocked()) return clearHoverNow();
  cancelHoverClear();
  window.clearTimeout(settleTimer);
  const changed = !hover || hover.id !== id || hover.thread !== thread;
  if (!changed && !hover!.settled) return;
  set({ id, thread, x: changed ? 0 : hover!.x, y: changed ? 0 : hover!.y, settled: false });
}

/** Right click: the strip at once. */
export function showToolbarNow(id: string, thread: boolean, x: number, y: number) {
  if (blocked()) return;
  cancelHoverClear();
  window.clearTimeout(settleTimer);
  set({ id, thread, x, y, settled: true });
}

/** Pointer left the row: hide after a moment, so it can reach the strip. */
export function clearHoverSoon() {
  if (!hover || clearTimer !== undefined) return;
  clearTimer = window.setTimeout(() => {
    clearTimer = undefined;
    clearHoverNow();
  }, HOVER_CLEAR_DELAY_MS);
}

export function clearHoverNow() {
  cancelHoverClear();
  window.clearTimeout(settleTimer);
  if (hover) set(null);
}

export function useHover(): Hover | null {
  return useSyncExternalStore(
    (notify) => {
      listeners.add(notify);
      return () => listeners.delete(notify);
    },
    () => hover,
  );
}

// quick_react_placement.rs

export const BUTTON_SIZE = 26;
const GAP = 14;

/** `toolbar_size`: `ownButtons` counts edit and delete. */
export function toolbarSize(stockButtons: number, recent: boolean, ownButtons: number) {
  const buttons = stockButtons + 4 + (recent ? 1 : 0) + ownButtons;
  const separators = 1 + (recent ? 1 : 0);
  const gaps = buttons + separators - 1;
  return {
    width: buttons * BUTTON_SIZE + separators + gaps * 2 + 6,
    height: BUTTON_SIZE + 6,
  };
}

type Size = { width: number; height: number };

/** `HoverGeometry::local_placement`, returning window coordinates. */
export function placeToolbar(
  pointer: { x: number; y: number },
  container: { left: number; top: number; width: number },
  viewport: Size,
  toolbar: Size,
): { x: number; y: number } | null {
  const right = Math.min(container.left + container.width + 16, viewport.width - 8);
  const left = Math.min(Math.max(container.left - 48, 8), Math.max(right - toolbar.width, 8));
  const area = {
    left,
    top: 56,
    width: Math.max(right - left, 0),
    height: Math.max(viewport.height - 128, 0),
  };
  if (area.width < toolbar.width || area.height < toolbar.height) return null;
  const areaRight = area.left + area.width;
  const areaBottom = area.top + area.height;
  const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max);
  const centeredX = clamp(pointer.x - toolbar.width / 2, area.left, areaRight - toolbar.width);
  const centeredY = clamp(pointer.y - toolbar.height / 2, area.top, areaBottom - toolbar.height);
  const candidates = [
    { x: pointer.x + GAP, y: centeredY },
    { x: centeredX, y: pointer.y - GAP - toolbar.height },
    { x: centeredX, y: pointer.y + GAP },
  ];
  return (
    candidates.find(
      ({ x, y }) =>
        x >= area.left &&
        x + toolbar.width <= areaRight &&
        y >= area.top &&
        y + toolbar.height <= areaBottom &&
        (x >= pointer.x + GAP ||
          x + toolbar.width <= pointer.x - GAP ||
          y >= pointer.y + GAP ||
          y + toolbar.height <= pointer.y - GAP),
    ) ?? null
  );
}
