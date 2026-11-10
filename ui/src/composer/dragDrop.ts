// Files dragged from Finder onto the conversation or the thread pane, as the
// `drag_over::<ExternalPaths>` / `on_drop` handlers on views/conversation and
// views/right_pane. Tauri reports drags for the whole window with real paths,
// so each mounted composer registers the pane it sits in and the drop goes to
// the pane under the pointer.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { send } from "../core/engine";
import type { Target } from "./types";

type Zone = { target: Target; anchor: HTMLElement };

const zones = new Set<Zone>();
let highlighted: { element: HTMLElement; background: string; boxShadow: string } | null = null;
let listening = false;

/** The pane a composer belongs to: its nearest ancestor that spans most of the window's height. */
function paneOf(anchor: HTMLElement): HTMLElement {
  let element: HTMLElement = anchor;
  while (element.parentElement && element.parentElement !== document.body) {
    element = element.parentElement;
    if (element.getBoundingClientRect().height >= window.innerHeight * 0.6) return element;
  }
  return element;
}

function zoneAt(x: number, y: number): { zone: Zone; pane: HTMLElement } | null {
  // The thread pane first: it is the narrower, more specific target.
  const rank = (zone: Zone) => (zone.target === "thread" ? 0 : 1);
  const ordered = [...zones].sort((a, b) => rank(a) - rank(b));
  for (const zone of ordered) {
    if (!zone.anchor.isConnected) continue;
    const pane = paneOf(zone.anchor);
    const rect = pane.getBoundingClientRect();
    if (x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom) return { zone, pane };
  }
  return null;
}

function highlight(pane: HTMLElement | null) {
  if (highlighted?.element === pane) return;
  if (highlighted) {
    highlighted.element.style.background = highlighted.background;
    highlighted.element.style.boxShadow = highlighted.boxShadow;
    highlighted = null;
  }
  if (!pane) return;
  highlighted = { element: pane, background: pane.style.background, boxShadow: pane.style.boxShadow };
  const dark = document.documentElement.dataset.theme === "dark";
  // tint(main_bg, 0.72) in dark, tint(0xe8eef5, 0.70) in light; 2px accent border.
  pane.style.background = dark
    ? "color-mix(in srgb, var(--main-bg) 72%, transparent)"
    : "rgba(232, 238, 245, 0.70)";
  pane.style.boxShadow = "inset 0 0 0 2px var(--accent)";
}

function startListening() {
  if (listening) return;
  listening = true;
  getCurrentWebview()
    .onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "leave") {
        highlight(null);
        return;
      }
      const scale = window.devicePixelRatio || 1;
      const x = payload.position.x / scale;
      const y = payload.position.y / scale;
      const hit = zoneAt(x, y);
      if (payload.type === "enter" || payload.type === "over") {
        highlight(hit?.pane ?? null);
        return;
      }
      highlight(null);
      if (payload.type === "drop" && hit && payload.paths.length > 0) {
        send("composer", "open_files", { target: hit.zone.target, paths: payload.paths });
      }
    })
    .catch((error) => {
      listening = false;
      console.error("could not listen for file drops", error);
    });
}

/** Makes the pane around `anchor` accept dropped files for `target`. */
export function registerDropZone(target: Target, anchor: HTMLElement): () => void {
  const zone = { target, anchor };
  zones.add(zone);
  startListening();
  return () => {
    zones.delete(zone);
    if (highlighted && !zones.size) highlight(null);
  };
}
