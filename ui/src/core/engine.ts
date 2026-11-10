// The bridge to the Rust engine (tauri/src-tauri/src/engine): model sections
// arrive as `model:<section>` events, and user actions go back as intents.

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";
import type { Sections } from "./sections";

type SectionName = keyof Sections;

const values: Partial<Sections> = {};
const listeners = new Map<SectionName, Set<() => void>>();

const SECTION_NAMES: SectionName[] = [
  "app",
  "workspace",
  "navigation",
  "sidebar",
  "conversation",
  "timeline",
  "composer",
  "quick_switcher",
  "emoji_picker",
  "find_in_chat",
  "search",
  "new_chat",
  "thread_pane",
  "profile_panel",
  "overlay",
  "call",
  "notifications",
  "settings",
  "shell",
  "timeline_extra",
  "composer_extra",
  "right_pane_extra",
  "overlays_extra",
  "sidebar_avatars",
];

let started: Promise<void> | null = null;

/** Subscribes to every section once, then asks the engine to send them all. */
export function startEngine(): Promise<void> {
  started ??= (async () => {
    await Promise.all(
      SECTION_NAMES.map((name) =>
        listen(`model:${name}`, (event) => {
          (values as Record<string, unknown>)[name] = event.payload;
          listeners.get(name)?.forEach((notify) => notify());
        }),
      ),
    );
    await sendIntent({ area: "republish" });
  })();
  return started;
}

/** The latest value of a model section; `undefined` until the engine sends it. */
export function useModel<K extends SectionName>(name: K): Sections[K] | undefined {
  return useSyncExternalStore(
    (notify) => {
      let set = listeners.get(name);
      if (!set) listeners.set(name, (set = new Set()));
      set.add(notify);
      return () => set!.delete(notify);
    },
    () => values[name],
  );
}

/** Reads a section outside React, e.g. in a key handler. */
export function readModel<K extends SectionName>(name: K): Sections[K] | undefined {
  return values[name];
}

/** Mirrors `engine::Intent`: `{ area, intent }`, with `intent` tagged by `type`. */
export type Intent =
  | { area: "republish" }
  | { area: "view"; intent: ViewReport }
  | { area: "shell" | "timeline" | "composer" | "right_pane" | "overlays"; intent: { type: string } & Record<string, unknown> };

export type ViewReport = {
  window_focused: boolean;
  timeline_near_bottom: boolean;
  first_visible_message_id: string | null;
};

export function sendIntent(intent: Intent): Promise<void> {
  return invoke("intent", { intent }).then(
    () => undefined,
    (error) => console.error("intent failed", intent, error),
  );
}

/** Shorthand for an area's intent: `send("timeline", "react", { message_id, emoji })`. */
export function send(
  area: "shell" | "timeline" | "composer" | "right_pane" | "overlays",
  type: string,
  fields: Record<string, unknown> = {},
): Promise<void> {
  return sendIntent({ area, intent: { type, ...fields } });
}

const view: ViewReport = {
  window_focused: document.hasFocus(),
  timeline_near_bottom: true,
  first_visible_message_id: null,
};

/** Reports layout facts the engine needs (it reads these from gpui in the old app). */
export function reportView(patch: Partial<ViewReport>) {
  const next = { ...view, ...patch };
  if (
    next.window_focused === view.window_focused &&
    next.timeline_near_bottom === view.timeline_near_bottom &&
    next.first_visible_message_id === view.first_visible_message_id
  ) {
    return;
  }
  Object.assign(view, next);
  sendIntent({ area: "view", intent: { ...view } });
}

window.addEventListener("focus", () => reportView({ window_focused: true }));
window.addEventListener("blur", () => reportView({ window_focused: false }));

/** Cached avatars, emoji and attachments are local paths; URLs pass through. */
export function assetUrl(source: string | null | undefined): string | undefined {
  if (!source) return undefined;
  if (/^(https?|data|blob|asset):/.test(source)) return source;
  // Repo-relative assets (demo avatars and files), which gpui embedded in the binary.
  if (source.startsWith("assets/")) return BUNDLED_ASSETS[`../../../${source}`] ?? undefined;
  return convertFileSrc(source);
}

const BUNDLED_ASSETS = import.meta.glob<string>(
  ["../../../assets/avatars/*", "../../../assets/demo/**/*"],
  { query: "?url", import: "default", eager: true },
);
