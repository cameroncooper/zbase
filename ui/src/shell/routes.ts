// Route helpers mirroring `domain::route::Route` and the time labels of
// util/formatting.rs that the sidebar shows.

import type { ConversationSummary, Route } from "./types";

/** `Route::label()`: the key the sidebar avatars and row ids use. */
export function routeLabel(route: Route): string {
  if (route === "Preferences") return "preferences";
  if ("WorkspaceHome" in route) return "workspace-home";
  if ("Channel" in route) return `channel:${route.Channel.channel_id}`;
  if ("DirectMessage" in route) return `dm:${route.DirectMessage.dm_id}`;
  if ("Search" in route) return `search:${route.Search.query}`;
  if ("Activity" in route) return "activity";
  return `call:${route.ActiveCall.call_id}`;
}

export function sameRoute(left: Route | null | undefined, right: Route | null | undefined): boolean {
  if (!left || !right) return false;
  return JSON.stringify(left) === JSON.stringify(right);
}

/** `Route::conversation_key()`. */
export function conversationKey(route: Route): string | null {
  if (route === "Preferences") return null;
  if ("Channel" in route) return route.Channel.channel_id;
  if ("DirectMessage" in route) return route.DirectMessage.dm_id;
  return null;
}

export function isConversation(route: Route | undefined): boolean {
  return !!route && route !== "Preferences" && ("Channel" in route || "DirectMessage" in route);
}

export function isRoute(route: Route | undefined, kind: string): boolean {
  return !!route && route !== "Preferences" && kind in route;
}

/** `quick_switcher_route_for_summary`. */
export function routeForSummary(summary: ConversationSummary, workspaceId: string): Route {
  return summary.kind === "Channel"
    ? { Channel: { workspace_id: workspaceId, channel_id: summary.id } }
    : { DirectMessage: { workspace_id: workspaceId, dm_id: summary.id } };
}

const startOfDay = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate());
const DAY_MS = 86_400_000;

function dayLabel(day: Date, today: Date, time: string): string {
  const days = Math.round((startOfDay(today).getTime() - startOfDay(day).getTime()) / DAY_MS);
  if (days <= 0) return time;
  if (days === 1) return "Yesterday";
  if (days < 7) return day.toLocaleDateString("en-US", { weekday: "short" });
  if (day.getFullYear() === today.getFullYear()) {
    return day.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  }
  return `${day.getMonth() + 1}/${day.getDate()}/${String(day.getFullYear()).slice(-2)}`;
}

/** `sidebar_time_label`: "4:34 PM", "Yesterday", "Tue", "Oct 30", "12/31/25". */
export function sidebarTimeLabel(timestampMs: number): string {
  const day = new Date(timestampMs);
  const hours = day.getHours() % 12 || 12;
  const time = `${hours}:${String(day.getMinutes()).padStart(2, "0")} ${day.getHours() < 12 ? "AM" : "PM"}`;
  return dayLabel(day, new Date(), time);
}

/** `sidebar_time_label_compact`: "10:30" today, the weekday from yesterday on. */
export function sidebarTimeLabelCompact(timestampMs: number): string {
  const day = new Date(timestampMs);
  const today = new Date();
  const days = Math.round((startOfDay(today).getTime() - startOfDay(day).getTime()) / DAY_MS);
  if (days === 1) return day.toLocaleDateString("en-US", { weekday: "short" });
  const clock = `${day.getHours() % 12 || 12}:${String(day.getMinutes()).padStart(2, "0")}`;
  return dayLabel(day, today, clock);
}
