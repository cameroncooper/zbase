// Members pane: a filter, then owners and admins, members and bots (views/right_pane/members.rs).

import { useRef, useState } from "react";
import { send } from "../core/engine";
import { bareInputStyle } from "../ui/common";
import { Avatar, EmptyState, IdentityMarker, PaneHeader, PaneWithHeader, SearchFieldShell, SecondaryButton, SectionLabel, useFocused, WithPresence } from "./parts";
import type { ChannelMemberPreview, ConversationView, RightPaneState } from "./types";

const SECTION_TITLES = ["Owners & admins", "Members", "Bots"] as const;

/** `member_sections`: matches by name or username, owners first, then by name. */
export function memberSections(members: ChannelMemberPreview[], query: string) {
  const needle = query.trim().toLowerCase();
  const sections: ChannelMemberPreview[][] = [[], [], []];
  for (const member of members) {
    if (
      needle &&
      !member.display_name.toLowerCase().includes(needle) &&
      !member.user_id.toLowerCase().includes(needle)
    )
      continue;
    const section = member.user_id.toLowerCase().endsWith("bot") ? 2 : member.is_team_admin_or_owner ? 0 : 1;
    sections[section].push(member);
  }
  const key = (member: ChannelMemberPreview) => [
    member.is_team_admin_or_owner && member.is_team_owner ? 0 : 1,
    member.display_name.toLowerCase(),
  ] as const;
  for (const people of sections) {
    people.sort((a, b) => {
      const [ownerA, nameA] = key(a);
      const [ownerB, nameB] = key(b);
      return ownerA - ownerB || (nameA < nameB ? -1 : nameA > nameB ? 1 : 0);
    });
  }
  return sections
    .map((people, index) => ({ title: SECTION_TITLES[index], people }))
    .filter((section) => section.people.length > 0);
}

const peopleCount = (count: number) => `${count} ${count === 1 ? "member" : "members"}`;

export function MembersPane({ conversation, state }: { conversation: ConversationView; state: RightPaneState }) {
  const input = useRef<HTMLInputElement>(null);
  const details = conversation.details;
  const members = details?.members ?? [];
  const total = details && details.member_count > 0 ? details.member_count : members.length;
  // Each opening mounts the pane afresh, so the filter starts empty (toggle_simple_pane).
  const [query, setQuery] = useState("");
  const [focused, focusHandlers] = useFocused();
  const sections = memberSections(members, query);
  const location = state.location;
  const subtitle = location
    ? total === 0
      ? location
      : `${location} · ${peopleCount(total)}`
    : total === 0
      ? null
      : peopleCount(total);

  let list;
  if (!details) {
    list = <EmptyState icon="members" title="Loading members…" />;
  } else if (members.length === 0) {
    list = (
      <EmptyState
        icon="members"
        title="No members to show"
        body="Keybase hasn't shared this conversation's member list yet."
      />
    );
  } else if (sections.length === 0) {
    // The filter only narrows this list, so the copy says where it looked.
    const where = conversation.summary.kind === "Channel" && location ? location : "this conversation";
    list = (
      <EmptyState icon="search" title={`No one in ${where} matches “${query.trim()}”.`}>
        <div style={{ paddingTop: 12 }}>
          <SecondaryButton
            onClick={() => {
              setQuery("");
              input.current?.focus();
            }}
          >
            Clear filter
          </SecondaryButton>
        </div>
      </EmptyState>
    );
  } else {
    list = (
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        {sections.map((section) => (
          <div key={section.title} style={{ display: "flex", flexDirection: "column" }}>
            <SectionLabel style={{ padding: "0 8px 4px" }}>
              {section.title} · {section.people.length}
            </SectionLabel>
            {section.people.map((member) => (
              <MemberRow key={member.user_id} member={member} />
            ))}
          </div>
        ))}
      </div>
    );
  }

  return (
    <PaneWithHeader header={<PaneHeader title="Members" subtitle={subtitle} />}>
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <div style={{ flexShrink: 0, padding: "12px 16px 8px" }}>
          <SearchFieldShell height={32} focused={focused} onClick={() => input.current?.focus()}>
            <input
              ref={input}
              value={query}
              placeholder="Filter members"
              spellCheck={false}
              style={bareInputStyle}
              onChange={(event) => setQuery(event.target.value)}
              {...focusHandlers}
            />
          </SearchFieldShell>
        </div>
        <div className="rp-scroll" style={{ padding: "4px 8px 16px" }}>
          {list}
        </div>
      </div>
    </PaneWithHeader>
  );
}

/** `member_row`: 40px, avatar with presence, name, @username, identity marker, role. */
function MemberRow({ member }: { member: ChannelMemberPreview }) {
  const username = member.user_id;
  const showUsername = member.display_name.trim().toLowerCase() !== username.toLowerCase();
  const role = member.is_team_admin_or_owner ? (member.is_team_owner ? "Owner" : "Admin") : null;
  return (
    <div className="rp-member-row" onClick={() => send("right_pane", "open_profile", { user_id: member.user_id })}>
      <WithPresence tooltip size={28} availability={member.availability} surface="var(--rp-surface)">
        <Avatar name={member.display_name} asset={member.avatar_asset} size={28} />
      </WithPresence>
      <div style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 6 }}>
        <div className="rp-member-name">{member.display_name}</div>
        <IdentityMarker affinity={member.affinity} name={member.display_name} />
        {showUsername && <div className="rp-member-username">@{username}</div>}
      </div>
      {role && <div className="rp-role">{role}</div>}
    </div>
  );
}
