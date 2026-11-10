// Right-pane pieces on top of the shared primitives in ../ui: identity and
// role markers (views/mod.rs), the pane header and timestamps.

import type { ReactNode } from "react";
import { send } from "../core/engine";
import { Icon } from "../ui/Icon";
import { Tooltip } from "../ui/Tooltip";
import type { Affinity } from "./types";

export { Icon } from "../ui/Icon";
export { Avatar, ParticipantsAvatar, WithPresence } from "../ui/Avatar";
export { Badge, EmptyState, PrimaryButton, SearchFieldShell, SecondaryButton, SectionLabel, useFocused } from "../ui/common";

/** `identity_marker`: a person-check for someone you follow, a warning when broken. */
export function IdentityMarker({ affinity, name }: { affinity: Affinity; name: string }) {
  if (affinity === "Positive")
    return (
      <Tooltip text={`You follow ${name}`} singleLine>
        <span className="rp-marker">
          <Icon name="user-check" size={13} color="var(--affinity-positive)" />
        </span>
      </Tooltip>
    );
  if (affinity === "Broken")
    return (
      <Tooltip text={`Identity broken: ${name}'s proofs changed since you followed them`} singleLine>
        <span className="rp-marker">
          <Icon name="alert-triangle" size={13} color="var(--affinity-broken)" />
        </span>
      </Tooltip>
    );
  return null;
}

/** `role_crown_badge`. */
export function RoleCrown({ label }: { label: string }) {
  return (
    <Tooltip text={label} singleLine>
      <span className="rp-marker">
        <Icon name="crown" size={13} color="#b7891d" />
      </span>
    </Tooltip>
  );
}

/** `pane_header`: 56px, title and optional one-line subtitle, close button. */
export function PaneHeader({ title, subtitle }: { title: string; subtitle?: string | null }) {
  return (
    <div className="rp-header">
      <div className="rp-header-text">
        <div className="rp-header-title">{title}</div>
        {subtitle && <div className="rp-header-subtitle">{subtitle}</div>}
      </div>
      <Tooltip text="Close" singleLine>
        <button className="rp-close" onClick={() => send("right_pane", "close")}>
          <Icon name="x" size={14} color="var(--text-secondary)" />
        </button>
      </Tooltip>
    </div>
  );
}

/** `pane_with_header`: the fixed header above a body that fills. */
export function PaneWithHeader({ header, children }: { header: ReactNode; children: ReactNode }) {
  return (
    <div className="rp-pane">
      {header}
      {children}
    </div>
  );
}

/** `message_timestamp_label` from util/formatting.rs: "3:04 PM • Yesterday", non-breaking. */
export function messageTimestampLabel(timestampMs: number | null): string {
  if (timestampMs == null) return "Now";
  const date = new Date(timestampMs);
  const now = new Date();
  const hours = date.getHours() % 12 || 12;
  const time = `${hours}:${String(date.getMinutes()).padStart(2, "0")} ${date.getHours() < 12 ? "AM" : "PM"}`;
  const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();
  const age = now.getTime() - date.getTime();
  const month = date.toLocaleDateString("en-US", { month: "short" });
  let label: string;
  if (sameDay(date, now)) label = time;
  else if (sameDay(date, new Date(now.getTime() - 86_400_000))) label = `${time} \u2022 Yesterday`;
  else if (age > 0 && age < 7 * 86_400_000)
    label = `${time} \u2022 ${date.toLocaleDateString("en-US", { weekday: "short" })}`;
  else if (age > 0 && age < 30 * 86_400_000) label = `${time} \u2022 ${date.getDate()} ${month}`;
  else label = `${time} \u2022 ${date.getDate()} ${month} ${String(date.getFullYear() % 100).padStart(2, "0")}`;
  return label.replace(/ /g, "\u00a0");
}
