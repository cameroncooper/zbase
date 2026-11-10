// Details pane: identity card, about, members, bots, shared files, leave (views/right_pane/details.rs).

import type { ReactNode } from "react";
import { send } from "../core/engine";
import { Avatar, Badge, IdentityMarker, PaneHeader, PaneWithHeader, RoleCrown, WithPresence } from "./parts";
import type { ChannelMemberPreview, ConversationView, RightPaneState } from "./types";

/** `truncate_with_ellipsis`. */
export function truncateWithEllipsis(value: string, maxChars: number): string {
  const chars = [...value];
  return chars.length <= maxChars ? value : `${chars.slice(0, maxChars).join("")}…`;
}

function Card({ children, center }: { children: ReactNode; center?: boolean }) {
  return (
    <div className="rp-card" style={center ? { alignItems: "center", gap: 12 } : undefined}>
      {children}
    </div>
  );
}

function MemberSection({ title, members }: { title: string; members: ChannelMemberPreview[] }) {
  return (
    <Card>
      <div className="rp-card-heading">{title}</div>
      {members.map((member) => (
        <div
          key={member.user_id}
          className="rp-details-member"
          onClick={() => send("right_pane", "open_profile", { user_id: member.user_id })}
        >
          <WithPresence tooltip size={32} availability={member.availability} surface="var(--rp-panel-surface)">
            <Avatar name={member.display_name} asset={member.avatar_asset} size={32} />
          </WithPresence>
          <div className="rp-truncate" style={{ flex: 1, fontSize: 14 }}>
            {member.display_name}
          </div>
          <IdentityMarker affinity={member.affinity} name={member.display_name} />
          {member.is_team_admin_or_owner && <RoleCrown label={member.is_team_owner ? "Team owner" : "Team admin"} />}
        </div>
      ))}
    </Card>
  );
}

/** `Avatar::render_participants_on`: one face with presence, or an overlapping pair. */
function ParticipantsAvatar({ conversation, title, size }: { conversation: ConversationView; title: string; size: number }) {
  const participants = conversation.participant_avatars;
  if (participants.length === 1) {
    const [one] = participants;
    return (
      <WithPresence size={size} availability={one.presence?.availability} surface="var(--rp-panel-surface)">
        <Avatar name={one.display_name} asset={one.asset} size={size} />
      </WithPresence>
    );
  }
  if (participants.length >= 2) {
    const small = Math.round(size * 0.72);
    return (
      <span style={{ position: "relative", width: size, height: size, flexShrink: 0 }}>
        <span style={{ position: "absolute", left: 0, top: 0 }}>
          <Avatar name={participants[0].display_name} asset={participants[0].asset} size={small} />
        </span>
        <span
          style={{
            position: "absolute",
            right: 0,
            bottom: 0,
            padding: 2,
            borderRadius: Math.round(small * 0.22) + 2,
            background: "var(--rp-panel-surface)",
            display: "flex",
          }}
        >
          <Avatar name={participants[1].display_name} asset={participants[1].asset} size={small - 4} />
        </span>
      </span>
    );
  }
  return <Avatar name={title} size={size} />;
}

export function DetailsPane({ conversation, state }: { conversation: ConversationView; state: RightPaneState }) {
  const details = conversation.details;
  const shared = state.files;
  const members = details?.members ?? [];
  const bots = members.filter((member) => member.user_id.toLowerCase().endsWith("bot"));
  const people = members.filter((member) => !member.user_id.toLowerCase().endsWith("bot"));
  let memberCount = details?.member_count ?? conversation.member_count;
  if (memberCount === 0) memberCount = people.length + bots.length;
  const groupName = details?.group?.display_name ?? conversation.summary.group?.display_name ?? null;
  const isChannel = conversation.summary.kind === "Channel";
  const title = isChannel ? `#${conversation.summary.title}` : conversation.summary.title;
  const rawTopic = details?.topic.trim() ?? "";
  const topic = rawTopic && rawTopic !== conversation.summary.title && rawTopic !== groupName ? rawTopic : null;
  const kindLabel = { Channel: "Channel", DirectMessage: "Direct message", GroupDirectMessage: "Group conversation" }[
    conversation.summary.kind
  ];
  const leaving = details?.is_leaving ?? false;

  return (
    <PaneWithHeader header={<PaneHeader title="Details" />}>
      <div className="rp-scroll" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 12 }}>
        <Card center>
          {isChannel ? (
            <Avatar name={groupName ?? title} asset={conversation.avatar_asset} size={64} team />
          ) : (
            <ParticipantsAvatar conversation={conversation} title={title} size={64} />
          )}
          <div style={{ fontSize: 18, fontWeight: 600, textAlign: "center" }}>{title}</div>
          {groupName && (
            <div style={{ fontSize: 14, textAlign: "center", color: "var(--text-secondary)" }}>{groupName}</div>
          )}
          <div style={{ display: "flex", flexWrap: "wrap", justifyContent: "center", gap: 8 }}>
            <Badge background="var(--panel-alt-bg)" foreground="var(--text-secondary)">{kindLabel}</Badge>
            {memberCount > 0 && <Badge background="var(--panel-alt-bg)" foreground="var(--text-secondary)">{`${memberCount} members`}</Badge>}
            {conversation.summary.muted && <Badge background="var(--panel-alt-bg)" foreground="var(--text-secondary)">Muted</Badge>}
          </div>
        </Card>
        {topic && (
          <Card>
            <div className="rp-card-heading">About</div>
            <div style={{ fontSize: 14 }}>{topic}</div>
          </Card>
        )}
        {people.length > 0 && <MemberSection title={`Members (${people.length})`} members={people} />}
        {bots.length > 0 && <MemberSection title={`Bots (${bots.length})`} members={bots} />}
        {people.length === 0 && bots.length === 0 && (
          <Card>
            <div className="rp-card-heading">Members</div>
            <div style={{ fontSize: 14, color: "var(--text-secondary)" }}>Member list is not available yet.</div>
          </Card>
        )}
        <Card>
          <div className="rp-card-heading">Shared files ({shared.length})</div>
          {shared.length === 0 && (
            <div
              style={{
                borderRadius: 6,
                background: "var(--panel-alt-bg)",
                padding: "6px 8px",
                fontSize: 12,
                color: "var(--text-secondary)",
              }}
            >
              No files in the loaded messages.
            </div>
          )}
          {shared.map((file) => (
            <div
              key={`${file.message_id}-${file.index}`}
              className="rp-shared-file"
              onClick={() => send("right_pane", "open_attachment", { message_id: file.message_id, index: file.index })}
            >
              <div style={{ flex: 1, minWidth: 0, overflow: "hidden", fontSize: 12, color: "var(--accent)" }}>
                {truncateWithEllipsis(file.attachment.name.trim() ? file.attachment.name : "attachment", 56)}
              </div>
            </div>
          ))}
        </Card>
        {details?.leave_error && <div style={{ fontSize: 14, color: "var(--danger)" }}>{details.leave_error}</div>}
        {details?.can_leave && (
          <div
            className={`rp-leave${leaving ? " rp-leaving" : ""}`}
            onClick={leaving ? undefined : () => send("right_pane", "leave_channel")}
          >
            {leaving ? "Leaving…" : "Leave Channel"}
          </div>
        )}
      </div>
    </PaneWithHeader>
  );
}
