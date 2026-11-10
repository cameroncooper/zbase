// views/avatar: rounded-square avatars, team marks, group pairs and counts,
// and presence dots with a distinct shape per state.

import { useState, type CSSProperties } from "react";
import { assetUrl } from "../core/engine";
import type { Availability, AvatarParticipant } from "./types";

/** `square_radius`: 4px at sidebar size, 6px at header size. */
const squareRadius = (size: number) => Math.min(12, Math.max(3, Math.round(size * 0.22)));

/** `team_colors`: a stable per-team hue from an FNV-1a hash of the name. */
const TEAM_FILLS = ["#3f73d9", "#7a55d6", "#0f8a7e", "#c0522a", "#b8437c", "#3d8a3f", "#5a6b85", "#a86512"];
export function teamFill(name: string): string {
  let hash = 0x811c9dc5;
  for (const byte of new TextEncoder().encode(name.trim().toLowerCase())) {
    hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  }
  return TEAM_FILLS[hash % TEAM_FILLS.length];
}

/** `default_avatar_background`. */
const defaultBackground = (name: string) => (name === "You" ? "var(--accent)" : "var(--avatar-bg)");

function initials(name: string): string {
  const parts = name
    .split(/\s+/)
    .filter(Boolean)
    .map((part) => [...part][0].toUpperCase())
    .slice(0, 2)
    .join("");
  return parts || "?";
}

type Kind = "person" | "team";

function Fallback({ name, size, background, foreground, kind }: { name: string; size: number; background: string; foreground: string; kind: Kind }) {
  const team = kind === "team";
  const text = size < 22 || team ? [...initials(name)][0] : initials(name);
  return (
    <div
      style={{
        flexShrink: 0,
        width: size,
        height: size,
        borderRadius: squareRadius(size),
        background,
        color: foreground,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontSize: Math.min(15, Math.max(7, size * (team ? 0.55 : 0.42))),
        fontWeight: 600,
        lineHeight: `${size}px`,
      }}
    >
      {text}
    </div>
  );
}

function AvatarImage(props: { name: string; asset: string | null | undefined; size: number; background: string; foreground: string; kind: Kind }) {
  const [failed, setFailed] = useState<string | null>(null);
  const url = assetUrl(props.asset);
  if (!url || failed === url) return <Fallback {...props} />;
  return (
    <img
      src={url}
      alt=""
      draggable={false}
      onError={() => setFailed(url)}
      style={{
        flexShrink: 0,
        width: props.size,
        height: props.size,
        borderRadius: squareRadius(props.size),
        objectFit: "cover",
        background: "var(--panel-bg)",
        display: "block",
      }}
    />
  );
}

/** `Avatar::render`: a person. */
export function Avatar({ name, asset, size, background, foreground }: { name: string; asset?: string | null; size: number; background?: string; foreground?: string }) {
  return (
    <AvatarImage
      name={name}
      asset={asset}
      size={size}
      background={background ?? defaultBackground(name)}
      foreground={foreground ?? "var(--text-primary)"}
      kind="person"
    />
  );
}

/** `Avatar::render_team`: the team image, or its initial on the team's hue. */
export function TeamAvatar({ name, asset, size }: { name: string; asset?: string | null; size: number }) {
  return <AvatarImage name={name} asset={asset} size={size} background={teamFill(name)} foreground="#ffffff" kind="team" />;
}

const presenceColor: Record<Availability, string | null> = {
  Active: "var(--success)",
  Away: "#f2a516",
  DoNotDisturb: "var(--danger)",
  Offline: "var(--text-secondary)",
  Unknown: null,
};

export const presenceLabel: Record<Availability, string | null> = {
  Active: "Active",
  Away: "Away",
  DoNotDisturb: "Do not disturb",
  Offline: "Offline",
  Unknown: null,
};

const dotInner = (size: number) => (size <= 20 ? 7 : size <= 28 ? 8 : 10);

/** `presence_dot`: solid, clock hands, a minus bar or a hollow ring. */
export function PresenceDot({ availability, avatarSize, surface }: { availability: Availability; avatarSize: number; surface: string }) {
  const color = presenceColor[availability];
  if (!color) return null;
  const inner = dotInner(avatarSize);
  const outer = inner + 4;
  const stroke = Math.max(1.5, inner * 0.2);
  const center = inner / 2;
  const hand = Math.max(2, inner * 0.32);
  const bar: CSSProperties = { position: "absolute", borderRadius: stroke / 2, background: "#ffffff" };
  return (
    <div style={{ width: outer, height: outer, borderRadius: "50%", background: surface, display: "flex", alignItems: "center", justifyContent: "center" }}>
      <div
        style={{
          position: "relative",
          width: inner,
          height: inner,
          borderRadius: "50%",
          boxSizing: "border-box",
          ...(availability === "Offline" ? { border: `1.5px solid ${color}` } : { background: color }),
        }}
      >
        {availability === "DoNotDisturb" && (
          <div style={{ ...bar, left: (inner - Math.round(inner * 0.6)) / 2, top: (inner - stroke) / 2, width: Math.round(inner * 0.6), height: stroke }} />
        )}
        {availability === "Away" && (
          <>
            <div style={{ ...bar, left: center - stroke / 2, top: center - hand, width: stroke, height: hand + stroke / 2 }} />
            <div style={{ ...bar, left: center - stroke / 2, top: center - stroke / 2, width: hand + stroke / 2, height: stroke }} />
          </>
        )}
      </div>
    </div>
  );
}

/** `Avatar::with_presence`. */
function WithPresence({ children, size, availability, surface }: { children: React.ReactNode; size: number; availability: Availability | undefined; surface: string }) {
  if (!availability || !presenceColor[availability]) return <>{children}</>;
  const outer = dotInner(size) + 4;
  return (
    <div style={{ position: "relative", flexShrink: 0, width: size, height: size }} title={presenceLabel[availability] ?? undefined}>
      {children}
      <div style={{ position: "absolute", right: -outer * 0.2, bottom: -outer * 0.2 }}>
        <PresenceDot availability={availability} avatarSize={size} surface={surface} />
      </div>
    </div>
  );
}

/**
 * `Avatar::render_sidebar_participants_on`: one face with presence, two
 * overlapping faces, or a count for larger groups.
 */
export function ParticipantsAvatar({ participants, fallbackName, size, surface }: { participants: AvatarParticipant[]; fallbackName: string; size: number; surface: string }) {
  if (participants.length === 1) {
    const [person] = participants;
    return (
      <WithPresence size={size} availability={person.presence?.availability} surface={surface}>
        <Avatar name={person.display_name} asset={person.asset} size={size} />
      </WithPresence>
    );
  }
  if (participants.length === 0) return <Avatar name={fallbackName} size={size} />;
  if (participants.length > 2) {
    return (
      <div
        style={{
          flexShrink: 0,
          width: size,
          height: size,
          borderRadius: squareRadius(size),
          background: "var(--avatar-bg)",
          color: "var(--text-primary)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: Math.min(14, Math.max(11, Math.round(size * 0.4))),
          fontWeight: 600,
          lineHeight: `${size}px`,
        }}
      >
        {participants.length > 99 ? "99+" : participants.length}
      </div>
    );
  }
  const mini = Math.round(size * 0.7);
  const offset = size - mini;
  const ring = Math.min(2.5, Math.max(1.5, size / 12));
  const [first, second] = participants;
  return (
    <div style={{ width: size, height: size, flexShrink: 0, position: "relative" }}>
      <div style={{ position: "absolute", top: 0, left: 0 }}>
        <Avatar name={first.display_name} asset={first.asset} size={mini} />
      </div>
      <div
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
      </div>
    </div>
  );
}
