// Profile panel and profile card (views/profile).

import { useEffect, useRef, useState, type ReactNode } from "react";
import { assetUrl, send } from "../core/engine";
import { Tooltip } from "../ui/Tooltip";
import type { IconName } from "../ui/Icon";
import {
  Avatar,
  EmptyState,
  Icon,
  IdentityMarker,
  PaneHeader,
  PaneWithHeader,
  PrimaryButton,
  SecondaryButton,
  SectionLabel,
  WithPresence,
} from "./parts";
import type {
  CustomField,
  IdentityProof,
  Presence,
  ProfilePanelModel,
  SocialGraph,
  SocialGraphEntry,
  TeamShowcaseEntry,
  UserProfile,
} from "./types";

const SOCIAL_ENTRY_HEIGHT = 36;
const SOCIAL_VIRTUALIZATION_MIN_ENTRIES = 200;
const SOCIAL_INITIAL_ROWS = 180;
const SOCIAL_CHUNK_ROWS = 72;
const SOCIAL_MAX_ROWS = 260;

function section<K extends string, T>(profile: UserProfile | null | undefined, key: K): T | undefined {
  for (const entry of profile?.sections ?? []) {
    if (key in entry) return (entry as Record<K, T>)[key];
  }
  return undefined;
}

const trimmed = (value: string | null | undefined) => {
  const text = value?.trim();
  return text ? text : null;
};

/** `presence_label` in views/profile: "Away · In a meeting". */
function presenceText(presence: Presence): string {
  const base = { Active: "Active", Away: "Away", DoNotDisturb: "Do not disturb", Offline: "Offline", Unknown: "" }[
    presence.availability
  ];
  const status = presence.status_text?.trim() ?? "";
  if (!base) return status;
  return status ? `${base} · ${status}` : base;
}

/** `proof_service`: a glyph and a readable name for a Keybase service id. */
function proofService(serviceName: string): { icon: IconName; label: string } {
  const known: Record<string, [IconName, string]> = {
    github: ["service-github", "GitHub"],
    twitter: ["service-twitter", "Twitter"],
    reddit: ["service-reddit", "Reddit"],
    hackernews: ["service-hackernews", "Hacker News"],
    facebook: ["service-facebook", "Facebook"],
    dns: ["service-globe", "Website"],
    web: ["service-globe", "Website"],
    http: ["service-globe", "Website"],
    https: ["service-globe", "Website"],
    pgp: ["service-key", "PGP key"],
  };
  const match = known[serviceName.trim().toLowerCase()];
  return match ? { icon: match[0], label: match[1] } : { icon: "link", label: serviceName.trim() };
}

const follow = (userId: string, following: boolean) =>
  send("right_pane", following ? "unfollow_user" : "follow_user", { user_id: userId });

function identity(panel: ProfilePanelModel, userId: string) {
  const profile = panel.profile;
  const displayName = trimmed(profile?.display_name) ?? userId;
  const username = trimmed(profile?.username) ?? userId;
  const presence: Presence = profile?.presence ?? { availability: "Unknown", status_text: null };
  const graph = section<"SocialGraph", SocialGraph>(profile, "SocialGraph");
  return { profile, displayName, username, presence, graph, following: graph?.you_are_following ?? false };
}

export function ProfilePanel({ panel }: { panel: ProfilePanelModel }) {
  const scroll = useRef<HTMLDivElement>(null);
  const [scrollState, setScrollState] = useState({ top: 0, max: 0 });
  const userId = panel.user_id;

  const measure = () => {
    const node = scroll.current;
    if (node) setScrollState({ top: node.scrollTop, max: Math.max(0, node.scrollHeight - node.clientHeight) });
  };
  useEffect(measure);
  useEffect(() => {
    scroll.current?.scrollTo({ top: 0 });
  }, [userId]);

  if (!userId) {
    return (
      <PaneWithHeader header={<PaneHeader title="Profile" />}>
        <div className="rp-scroll" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 12 }}>
          <EmptyState icon="members" title="No one selected" body="Click a name in a conversation to see their profile." />
        </div>
      </PaneWithHeader>
    );
  }

  const { profile, displayName, username, presence, graph, following } = identity(panel, userId);
  const title = trimmed(profile?.title);
  const bio = trimmed(profile?.bio);
  const location = trimmed(profile?.location);
  const proofs = section<"IdentityProofs", IdentityProof[]>(profile, "IdentityProofs") ?? [];
  const teams = section<"TeamShowcase", TeamShowcaseEntry[]>(profile, "TeamShowcase") ?? [];
  const fields = section<"CustomFields", CustomField[]>(profile, "CustomFields") ?? [];
  const status = presenceText(presence);

  const sections: [string, ReactNode][] = [];
  if (bio || location)
    sections.push([
      "About",
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        {bio && <div style={{ fontSize: 13, lineHeight: "19px", whiteSpace: "pre-wrap", userSelect: "text" }}>{bio}</div>}
        {location && <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>{location}</div>}
      </div>,
    ]);
  if (proofs.length) sections.push(["Identity proofs", <Proofs proofs={proofs} />]);
  if (teams.length) sections.push(["Teams", <Teams teams={teams} />]);
  if (fields.length) sections.push(["Details", <Fields fields={fields} />]);

  const moreBelow = scrollState.max > 1 && scrollState.top < scrollState.max - 1;

  return (
    <PaneWithHeader header={<PaneHeader title="Profile" />}>
      <div style={{ flex: 1, minHeight: 0, position: "relative", display: "flex", flexDirection: "column" }}>
        <div ref={scroll} className="rp-scroll" onScroll={measure} style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "16px 16px 12px", display: "flex", flexDirection: "column", alignItems: "center", gap: 12 }}>
            <WithPresence tooltip size={64} availability={presence.availability} surface="var(--rp-surface)">
              <Avatar name={displayName} asset={profile?.avatar_asset} size={64} />
            </WithPresence>
            <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 2, textAlign: "center" }}>
              <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
                <div style={{ fontSize: 17, lineHeight: "22px", fontWeight: 600, userSelect: "text" }}>{displayName}</div>
                <IdentityMarker affinity={profile?.affinity ?? "None"} name={displayName} />
              </div>
              <div style={{ fontSize: 13, color: "var(--text-secondary)", userSelect: "text" }}>@{username}</div>
              {title && <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>{title}</div>}
              {status && <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>{status}</div>}
            </div>
            <div style={{ display: "flex", gap: 8 }}>
              <PrimaryButton onClick={() => send("right_pane", "profile_open_message", { user_id: userId })}>Message</PrimaryButton>
              <SecondaryButton onClick={() => follow(userId, following)}>{following ? "Unfollow" : "Follow"}</SecondaryButton>
            </div>
          </div>
          {panel.loading && !profile && (
            <>
              <Skeleton widths={[180, 120]} />
              <Skeleton widths={[140, 100, 160]} />
            </>
          )}
          {sections.map(([label, content]) => (
            <div key={label} className="rp-profile-section">
              <SectionLabel>{label}</SectionLabel>
              {content}
            </div>
          ))}
          {graph && (
            <div className="rp-profile-section">
              <Social panel={panel} graph={graph} scrollTop={scrollState.top} scrollMax={scrollState.max} />
            </div>
          )}
          {/* 16px after the last row; a spacer, since bottom padding is outside the scroll extent. */}
          <div style={{ flexShrink: 0, height: 16 }} />
        </div>
        {moreBelow && <div className="rp-fade-bottom" />}
      </div>
    </PaneWithHeader>
  );
}

function Skeleton({ widths }: { widths: number[] }) {
  return (
    <div className="rp-profile-section" style={{ gap: 10 }}>
      {widths.map((width, index) => (
        <div key={index} className="rp-skeleton" style={{ width }} />
      ))}
    </div>
  );
}

/** `social_entry_window`: reveal long lists in chunks as the profile scrolls. */
function socialWindow(total: number, top: number, max: number): [number, number] {
  if (total <= SOCIAL_INITIAL_ROWS) return [0, total];
  const progress = max <= 0 ? 0 : Math.min(1, Math.max(0, Math.min(top, max) / max));
  const revealed = Math.ceil((total - SOCIAL_INITIAL_ROWS) * progress);
  const rows = Math.ceil(revealed / SOCIAL_CHUNK_ROWS) * SOCIAL_CHUNK_ROWS;
  const end = Math.min(total, SOCIAL_INITIAL_ROWS + rows);
  return [Math.max(0, end - SOCIAL_MAX_ROWS), end];
}

function Social({ panel, graph, scrollTop, scrollMax }: { panel: ProfilePanelModel; graph: SocialGraph; scrollTop: number; scrollMax: number }) {
  const tab = panel.active_social_tab;
  const list = tab === "Followers" ? graph.followers : graph.following;
  const entries = list ?? [];
  const loaded = list != null;
  const loading = !loaded && panel.social_list_status === "Loading";
  const failed = !loaded && panel.social_list_status === "Failed";
  const empty = (loaded || panel.social_list_status == null) && entries.length === 0;
  const virtualize = entries.length >= SOCIAL_VIRTUALIZATION_MIN_ENTRIES;
  const [start, end] = virtualize ? socialWindow(entries.length, scrollTop, scrollMax) : [0, entries.length];
  const select = (list: "followers" | "following") => send("right_pane", "profile_select_social_tab", { list });
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <div style={{ display: "flex", gap: 16, borderBottom: "1px solid var(--rp-shell-border)" }}>
        <div className={`rp-social-tab${tab === "Followers" ? " rp-selected" : ""}`} onClick={() => select("followers")}>
          Followers · {graph.followers_count ?? 0}
        </div>
        <div className={`rp-social-tab${tab === "Following" ? " rp-selected" : ""}`} onClick={() => select("following")}>
          Following · {graph.following_count ?? 0}
        </div>
      </div>
      {loading && <div className="rp-quiet">Loading people…</div>}
      {empty && <div className="rp-quiet">{tab === "Followers" ? "No followers yet." : "Not following anyone yet."}</div>}
      {failed && (
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <div className="rp-quiet">Couldn't load this list from Keybase.</div>
          <div
            style={{ cursor: "pointer", fontSize: 12, fontWeight: 500, color: "var(--link)" }}
            onClick={() => select(tab === "Followers" ? "followers" : "following")}
          >
            Retry
          </div>
        </div>
      )}
      <div style={{ display: "flex", flexDirection: "column" }}>
        {start > 0 && <div style={{ height: start * SOCIAL_ENTRY_HEIGHT }} />}
        {entries.slice(start, end).map((entry) => (
          <SocialRow key={entry.user_id} entry={entry} />
        ))}
        {end < entries.length && <div style={{ height: (entries.length - end) * SOCIAL_ENTRY_HEIGHT }} />}
      </div>
    </div>
  );
}

function SocialRow({ entry }: { entry: SocialGraphEntry }) {
  const name = entry.display_name.trim() ? entry.display_name : entry.user_id;
  return (
    <div className="rp-social-row" onClick={() => send("right_pane", "open_profile", { user_id: entry.user_id })}>
      <Avatar name={name} asset={entry.avatar_asset} size={24} />
      <div className="rp-truncate" style={{ fontSize: 13 }}>
        {name}
      </div>
      <IdentityMarker affinity={entry.affinity} name={name} />
    </div>
  );
}

/** `render_proofs_section`: service icon, handle, service, verification state. */
function Proofs({ proofs }: { proofs: IdentityProof[] }) {
  return (
    <div style={{ display: "flex", flexDirection: "column" }}>
      {proofs.map((proof, index) => {
        const target = proof.proof_url ?? proof.site_url;
        const service = proofService(proof.service_name);
        const icon = proof.icon_asset && !proof.icon_asset.startsWith("assets/") ? assetUrl(proof.icon_asset) : undefined;
        return (
          <div
            key={index}
            className={`rp-proof-row${target ? " rp-link" : ""}`}
            onClick={target ? () => send("right_pane", "open_url", { url: target }) : undefined}
          >
            {icon ? (
              <img src={icon} alt="" style={{ width: 16, height: 16, flexShrink: 0, objectFit: "contain" }} />
            ) : (
              <Icon name={service.icon} size={16} color="var(--text-primary)" />
            )}
            <div style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "baseline", gap: 6 }}>
              <div className="rp-truncate" style={{ fontSize: 13 }}>
                {proof.service_username}
              </div>
              <div style={{ flexShrink: 0, fontSize: 12, color: "var(--text-secondary)" }}>{service.label}</div>
            </div>
            {proof.state === "Verified" && (
              <Tooltip text="Verified" singleLine>
                <span style={{ display: "flex" }}>
                  <Icon name="check-circle" size={14} color="var(--affinity-positive)" />
                </span>
              </Tooltip>
            )}
            {proof.state === "Broken" && (
              <Tooltip text="Proof is broken" singleLine>
                <span style={{ display: "flex" }}>
                  <Icon name="alert-triangle" size={14} color="var(--affinity-broken)" />
                </span>
              </Tooltip>
            )}
            {proof.state === "Pending" && <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>Pending</div>}
          </div>
        );
      })}
    </div>
  );
}

function Teams({ teams }: { teams: TeamShowcaseEntry[] }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      {teams.map((team) => (
        <div key={team.name} style={{ display: "flex", alignItems: "flex-start", gap: 10 }}>
          <Avatar name={team.name} size={24} team />
          <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
            <div style={{ display: "flex", alignItems: "baseline", gap: 6 }}>
              <div className="rp-truncate" style={{ fontSize: 13, fontWeight: 500 }}>
                {team.name}
              </div>
              <div style={{ flexShrink: 0, fontSize: 12, color: "var(--text-secondary)" }}>
                {team.members_count} {team.members_count === 1 ? "member" : "members"}
              </div>
            </div>
            {team.description.trim() && (
              <div style={{ fontSize: 12, lineHeight: "16px", color: "var(--text-secondary)" }}>{team.description}</div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

function Fields({ fields }: { fields: CustomField[] }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      {fields.map((field, index) => (
        <div key={index} style={{ display: "flex", alignItems: "baseline", gap: 12 }}>
          <div style={{ width: 96, flexShrink: 0, fontSize: 12, color: "var(--text-secondary)" }}>{field.label}</div>
          <div style={{ flex: 1, minWidth: 0, fontSize: 13, userSelect: "text" }}>{field.value}</div>
        </div>
      ))}
    </div>
  );
}

/** `render_profile_card`: the popover for `user_id`, from the profile panel model. */
export function ProfileCard({ panel, userId }: { panel: ProfilePanelModel; userId: string }) {
  const profile = panel.profile?.user_id === userId ? panel.profile : null;
  const { displayName, username, presence, graph, following } = identity({ ...panel, profile }, userId);
  const bio = trimmed(profile?.bio);
  return (
    <div className="rp-profile-card" onClick={(event) => event.stopPropagation()}>
      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <WithPresence tooltip size={48} availability={presence.availability} surface="var(--rp-panel-surface)">
          <Avatar name={displayName} asset={profile?.avatar_asset} size={48} />
        </WithPresence>
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
            <div style={{ fontWeight: 600 }}>{displayName}</div>
            <IdentityMarker affinity={profile?.affinity ?? "None"} name={displayName} />
          </div>
          <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>@{username}</div>
        </div>
      </div>
      {bio && <div style={{ fontSize: 14, color: "var(--text-secondary)" }}>{bio}</div>}
      {graph && (
        <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>
          {graph.followers_count ?? 0} followers {"·"} {graph.following_count ?? 0} following
        </div>
      )}
      <div style={{ paddingTop: 4, display: "flex", gap: 8 }}>
        <PrimaryButton onClick={() => send("right_pane", "open_profile", { user_id: userId })}>View profile</PrimaryButton>
        <SecondaryButton onClick={() => follow(userId, following)}>{following ? "Unfollow" : "Follow"}</SecondaryButton>
        <div style={{ flex: 1 }} />
        <SecondaryButton onClick={() => send("right_pane", "hide_profile_card")}>Close</SecondaryButton>
      </div>
    </div>
  );
}
