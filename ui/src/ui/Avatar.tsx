// Avatars, the counterpart of src/views/avatar/mod.rs: rounded squares with an
// image or initials, team marks on a per-team hue, overlapping pairs for group
// conversations, and presence dots whose shape differs per state.

import { useState, type ReactNode } from "react";
import { assetUrl } from "../core/engine";
import { Tooltip } from "./Tooltip";

export type Availability = "Active" | "Away" | "DoNotDisturb" | "Offline" | "Unknown";

export type AvatarParticipant = {
  display_name: string;
  asset: string | null;
  presence: { availability: Availability; status_text: string | null } | null;
};

function isDark(): boolean {
  return document.documentElement.dataset.theme === "dark";
}

/** `default_avatar_background`. */
export function defaultAvatarBackground(name: string): string {
  if (name === "You") return "var(--accent)";
  return isDark() ? "#484848" : "#d8dde3";
}

const TEAM_FILLS = ["#3f73d9", "#7a55d6", "#0f8a7e", "#c0522a", "#b8437c", "#3d8a3f", "#5a6b85", "#a86512"];

/** `team_colors`: FNV-1a over the lowercase name picks one of eight fills. */
export function teamColors(name: string): [string, string] {
  let hash = 0x811c9dc5;
  for (const byte of new TextEncoder().encode(name.trim().toLowerCase())) {
    hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  }
  return [TEAM_FILLS[hash % TEAM_FILLS.length], "#ffffff"];
}

function squareRadius(size: number): number {
  return Math.min(12, Math.max(3, Math.round(size * 0.22)));
}

function initials(name: string): string {
  const letters = name
    .split(/\s+/)
    .filter(Boolean)
    .map((part) => [...part][0]!.toUpperCase())
    .slice(0, 2)
    .join("");
  return letters || "?";
}

type Props = {
  name: string;
  asset?: string | null;
  size: number;
  background?: string;
  foreground?: string;
  team?: boolean;
};

/** `Avatar::render` / `Avatar::render_team`. */
export function Avatar({ name, asset, size, background, foreground, team = false }: Props) {
  const [failed, setFailed] = useState(false);
  const [teamBackground, teamForeground] = teamColors(name);
  const bg = background ?? (team ? teamBackground : defaultAvatarBackground(name));
  const fg = foreground ?? (team ? teamForeground : "var(--text-primary)");
  const radius = squareRadius(size);
  const url = assetUrl(asset);
  const box = {
    width: size,
    height: size,
    minWidth: size,
    borderRadius: radius,
    flexShrink: 0,
  } as const;
  if (url && !failed) {
    return (
      <span style={{ ...box, overflow: "hidden", display: "inline-flex", background: "var(--panel-bg)" }}>
        <img
          src={url}
          alt=""
          draggable={false}
          onError={() => setFailed(true)}
          style={{ width: size, height: size, objectFit: "cover", borderRadius: radius }}
        />
      </span>
    );
  }
  const text = size < 22 || team ? [...initials(name)][0] : initials(name);
  return (
    <span
      style={{
        ...box,
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        background: bg,
        color: fg,
        fontSize: Math.min(15, Math.max(7, size * (team ? 0.55 : 0.42))),
        fontWeight: 600,
        lineHeight: `${size}px`,
      }}
    >
      {text}
    </span>
  );
}

/** `presence_color`. */
export function presenceColor(availability: Availability): string | null {
  switch (availability) {
    case "Active":
      return "var(--success)";
    case "Away":
      return "#f2a516";
    case "DoNotDisturb":
      return "var(--danger)";
    case "Offline":
      return "var(--text-secondary)";
    default:
      return null;
  }
}

/** `presence_label`. */
export function presenceLabel(availability: Availability): string | null {
  switch (availability) {
    case "Active":
      return "Active";
    case "Away":
      return "Away";
    case "DoNotDisturb":
      return "Do not disturb";
    case "Offline":
      return "Offline";
    default:
      return null;
  }
}

/** `presence_dot`: solid, clock hands, minus bar or hollow ring, on a `surface` ring. */
export function PresenceDot({
  availability,
  avatarSize,
  surface,
}: {
  availability: Availability;
  avatarSize: number;
  surface: string;
}) {
  const color = presenceColor(availability);
  if (!color) return null;
  const inner = avatarSize <= 20 ? 7 : avatarSize <= 28 ? 8 : 10;
  const outer = inner + 4;
  const stroke = Math.max(1.5, inner * 0.2);
  const ring = availability === "Offline";
  const center = inner / 2;
  const hand = Math.max(2, inner * 0.32);
  const bar = Math.round(inner * 0.6);
  const white = "#ffffff";
  return (
    <span
      style={{
        width: outer,
        height: outer,
        borderRadius: "50%",
        background: surface,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <span
        style={{
          position: "relative",
          width: inner,
          height: inner,
          borderRadius: "50%",
          boxSizing: "border-box",
          border: ring ? `1.5px solid ${color}` : undefined,
          background: ring ? undefined : color,
        }}
      >
        {availability === "DoNotDisturb" && (
          <span
            style={{
              position: "absolute",
              left: (inner - bar) / 2,
              top: (inner - stroke) / 2,
              width: bar,
              height: stroke,
              borderRadius: stroke / 2,
              background: white,
            }}
          />
        )}
        {availability === "Away" && (
          <>
            <span
              style={{
                position: "absolute",
                left: center - stroke / 2,
                top: center - hand,
                width: stroke,
                height: hand + stroke / 2,
                borderRadius: stroke / 2,
                background: white,
              }}
            />
            <span
              style={{
                position: "absolute",
                left: center - stroke / 2,
                top: center - stroke / 2,
                width: hand + stroke / 2,
                height: stroke,
                borderRadius: stroke / 2,
                background: white,
              }}
            />
          </>
        )}
      </span>
    </span>
  );
}

/** `Avatar::with_presence` (and `with_presence_tooltip` when `tooltip` is set). */
export function WithPresence({
  children,
  size,
  availability,
  surface,
  tooltip = false,
}: {
  children: ReactNode;
  size: number;
  availability: Availability | null | undefined;
  surface: string;
  tooltip?: boolean;
}) {
  if (!availability || !presenceColor(availability)) return <>{children}</>;
  const dotSize = (size <= 20 ? 7 : size <= 28 ? 8 : 10) + 4;
  const body = (
    <span style={{ position: "relative", display: "inline-flex", flexShrink: 0, width: size, height: size }}>
      {children}
      <span style={{ position: "absolute", right: -dotSize * 0.2, bottom: -dotSize * 0.2 }}>
        <PresenceDot availability={availability} avatarSize={size} surface={surface} />
      </span>
    </span>
  );
  const label = presenceLabel(availability);
  return tooltip && label ? <Tooltip text={label}>{body}</Tooltip> : body;
}

/** `Avatar::render_participants_on`: one face with presence, or an overlapping pair. */
export function ParticipantsAvatar({
  participants,
  fallbackName,
  size,
  surface,
}: {
  participants: AvatarParticipant[];
  fallbackName: string;
  size: number;
  surface: string;
}) {
  if (participants.length === 1) {
    const [person] = participants;
    return (
      <WithPresence size={size} availability={person.presence?.availability} surface={surface}>
        <Avatar name={person.display_name} asset={person.asset} size={size} />
      </WithPresence>
    );
  }
  if (participants.length === 0) return <Avatar name={fallbackName} size={size} />;
  const mini = Math.round(size * 0.7);
  const offset = size - mini;
  const ring = Math.min(2.5, Math.max(1.5, size / 12));
  const [first, second] = participants;
  return (
    <span style={{ position: "relative", width: size, height: size, flexShrink: 0, display: "inline-block" }}>
      <span style={{ position: "absolute", top: 0, left: 0, display: "flex" }}>
        <Avatar name={first.display_name} asset={first.asset} size={mini} />
      </span>
      <span
        style={{
          position: "absolute",
          top: offset - ring,
          left: offset - ring,
          width: mini + ring * 2,
          height: mini + ring * 2,
          borderRadius: squareRadius(mini) + ring,
          background: surface,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Avatar name={second.display_name} asset={second.asset} size={mini} />
      </span>
    </span>
  );
}
