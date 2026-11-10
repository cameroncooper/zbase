// views/sidebar: toolbar, Recent and Grouped lists in both row styles, the
// order pop-up, and the identity footer.

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { send, useModel } from "../core/engine";
import { IS_MAC, shortcutLabel } from "../core/shortcuts";
import { Avatar, ParticipantsAvatar, TeamAvatar } from "./Avatar";
import { Icon } from "./Icon";
import { PopupMenu } from "./PopupMenu";
import { conversationKey, isRoute, routeLabel, sameRoute, sidebarTimeLabel, sidebarTimeLabelCompact } from "./routes";
import type {
  Connectivity,
  PreviewLive,
  RecentSidebarRow,
  Route,
  SidebarAvatars,
  SidebarModel,
  SidebarRow,
  SidebarSection,
} from "./types";

const ROW_AVATAR_SIZE_PX = 20;
const PREVIEW_AVATAR_PX = 32;
const TOOLBAR_BUTTON_PX = 28;
const TOOLBAR_ICON_PX = 16;
const SECTION_GAP_PX = 16;
const FIRST_SECTION_GAP_PX = 8;
const PREVIEWS_SIDEBAR_WIDTH_PX = 280;

type Menu = "order" | "identity";

/** `RowActivity`: mentions earn "@", DMs a count, other unread a quiet dot. */
function activityOf(row: SidebarRow) {
  const direct = isRoute(row.route, "DirectMessage");
  const badge = direct ? Math.max(row.unread_count, row.mention_count) : row.mention_count;
  return {
    unread: row.unread_count,
    badge,
    mention: !direct,
    muted: row.muted,
    isUnread: badge > 0 || (row.unread_count > 0 && !row.muted),
  };
}
type Activity = ReturnType<typeof activityOf>;

/** `badge_label`: "@" for one mention, "@3" for more; a plain count otherwise. */
function badgeLabel(count: number, mention: boolean): string {
  const text = count > 99 ? "99+" : String(count);
  return mention ? (text === "1" ? "@" : `@${text}`) : text;
}

function Indicator({ activity, selected }: { activity: Activity; selected: boolean }) {
  if (selected) return null;
  if (activity.badge > 0) return <span className="row-badge">{badgeLabel(activity.badge, activity.mention)}</span>;
  if (activity.unread > 0 && !activity.muted) return <span className="row-dot" />;
  return null;
}

/** `RowColors`: read rows recede, muted further; hover and selection restore contrast. */
function rowColors(selected: boolean, hovered: boolean, activity: Activity) {
  if (selected) {
    return { primary: "var(--text-primary)", secondary: "var(--text-secondary)", surface: "var(--sidebar-card-bg)" };
  }
  const surface = hovered ? "var(--sidebar-hover-bg)" : "var(--sidebar-bg)";
  if (hovered || activity.isUnread) return { primary: "var(--text-primary)", secondary: "var(--text-secondary)", surface };
  if (activity.muted) return { primary: "var(--text-secondary-a60)", secondary: "var(--text-secondary-a60)", surface };
  return { primary: "var(--text-secondary)", secondary: "var(--text-secondary)", surface };
}
type Colors = ReturnType<typeof rowColors>;

type Selection = "none" | "full" | "marker";
const selectionOf = (row: SidebarRow, current: Route | undefined, owns: boolean): Selection =>
  !sameRoute(row.route, current) ? "none" : owns ? "full" : "marker";

/** `title_segments`: the name (semibold while unread), then a suffix that truncates first. */
function Title({ label, suffix, colors, emphasized }: { label: string; suffix?: string | null; colors: Colors; emphasized: boolean }) {
  return (
    // One line cut once at its end, like `TruncatedLine`: the suffix goes first.
    <div className="row-title" style={{ color: suffix ? colors.secondary : colors.primary }}>
      <span style={{ color: colors.primary, fontWeight: emphasized ? 600 : 400 }}>{label}</span>
      {suffix && <span style={{ color: colors.secondary }}>{`  ${suffix}`}</span>}
    </div>
  );
}

function teamAsset(avatars: SidebarAvatars | undefined, teamId: string): string | null {
  const avatar = avatars?.[`sidebar-team:${teamId}`];
  return avatar && "Team" in avatar ? avatar.Team : null;
}

/** `Sidebar::leading_element`: a simple team's mark, `#` under a team, faces for DMs. */
function Leading({ row, avatars, colors, size }: { row: SidebarRow; avatars: SidebarAvatars | undefined; colors: Colors; size: number }) {
  const avatar = avatars?.[routeLabel(row.route)];
  if (isRoute(row.route, "Channel")) {
    if (!row.team) {
      return <TeamAvatar name={row.label} asset={avatar && "Team" in avatar ? avatar.Team : null} size={size} />;
    }
    return (
      <div className="row-lead" style={{ width: size, color: colors.secondary }}>
        <Icon name="hash" size={16} />
      </div>
    );
  }
  if (isRoute(row.route, "DirectMessage")) {
    const participants = avatar && "Participants" in avatar ? avatar.Participants : [];
    return <ParticipantsAvatar participants={participants} fallbackName={row.label} size={size} surface={colors.surface} />;
  }
  return (
    <div className="row-lead" style={{ width: size, color: "var(--text-secondary)" }}>
      <Icon name="chevron-right" size={14} />
    </div>
  );
}

type RowProps = {
  row: SidebarRow;
  label: string;
  suffix?: string | null;
  lead: (colors: Colors, size: number) => ReactNode;
  selection: Selection;
  /** Keyboard highlight (Alt+Up/Down), drawn like hover. */
  highlighted?: boolean;
  look: "compact" | { compactTime: boolean };
  live: PreviewLive | undefined;
};

/** `CompactRow` and `PreviewRow`. */
function Row({ row, label, suffix, lead, selection, highlighted, look, live }: RowProps) {
  const [hovered, setHovered] = useState(false);
  const activity = activityOf(row);
  const selected = selection === "full";
  const colors = rowColors(selected, hovered || !!highlighted, activity);
  const emphasized = !selected && activity.isUnread;
  const className = `sidebar-row ${look === "compact" ? "compact" : "preview"}${selected ? " selected" : hovered || highlighted ? " hovered" : ""}`;
  const fullLabel = row.team ? `${row.team.display_name} #${row.label}` : row.label;
  const common = {
    className,
    title: fullLabel,
    "data-route": routeLabel(row.route),
    "data-selected": selected || undefined,
    onMouseEnter: () => setHovered(true),
    onMouseLeave: () => setHovered(false),
    onClick: () => send("shell", "navigate", { route: row.route }),
  };
  if (look === "compact") {
    return (
      <div {...common}>
        {selection === "marker" && <div className="marker" />}
        <div className="row-lead" style={{ width: ROW_AVATAR_SIZE_PX }}>
          {lead(colors, ROW_AVATAR_SIZE_PX)}
        </div>
        <Title label={label} suffix={suffix} colors={colors} emphasized={emphasized} />
        <Indicator activity={activity} selected={selection !== "none"} />
      </div>
    );
  }
  const timestamp = row.preview?.timestamp_ms;
  const time = timestamp ? (look.compactTime ? sidebarTimeLabelCompact(timestamp) : sidebarTimeLabel(timestamp)) : null;
  const unread = activity.isUnread && !selected;
  return (
    <div {...common}>
      <div style={{ width: PREVIEW_AVATAR_PX, height: PREVIEW_AVATAR_PX, flexShrink: 0 }}>{lead(colors, PREVIEW_AVATAR_PX)}</div>
      <div className="preview-body">
        <div className="preview-title-line">
          <Title label={label} suffix={suffix} colors={colors} emphasized={emphasized} />
          {time && (
            <span className="preview-time" style={{ color: colors.secondary }}>
              {time}
            </span>
          )}
        </div>
        <div className="preview-line">
          <div className="preview-text" style={{ fontStyle: live && typeof live === "object" && "Typing" in live ? "italic" : undefined }}>
            {live === "NotSent" && <Icon name="alert-triangle" size={12} color="var(--danger)" />}
            <PreviewText row={row} live={live} colors={colors} unread={unread} />
          </div>
          <Indicator activity={activity} selected={selected} />
        </div>
      </div>
    </div>
  );
}

/** `preview_line`: what is happening now, or the newest message. */
function PreviewText({ row, live, colors, unread }: { row: SidebarRow; live: PreviewLive | undefined; colors: Colors; unread: boolean }) {
  if (live === "NotSent") {
    return <span style={{ color: "var(--danger)", fontWeight: 500 }}>Not sent</span>;
  }
  if (live && "Typing" in live) return <span style={{ color: colors.secondary }}>{live.Typing}</span>;
  if (live && "Draft" in live) {
    return (
      <span>
        <span style={{ color: "var(--accent)", fontWeight: 500 }}>Draft: </span>
        <span style={{ color: colors.secondary }}>{live.Draft}</span>
      </span>
    );
  }
  if (!row.preview) return <span />;
  const color = unread ? colors.primary : colors.secondary;
  return (
    <span style={{ color }}>
      {row.preview.prefix && <span style={{ fontWeight: 500 }}>{row.preview.prefix}</span>}
      {row.preview.text}
    </span>
  );
}

/** `SidebarModel::visible_recent_rows`. */
function visibleRecentRows(sidebar: SidebarModel): RecentSidebarRow[] {
  const filter = sidebar.filter.trim().toLowerCase();
  if (!filter) return sidebar.recent_rows;
  return sidebar.recent_rows.filter(({ row, secondary }) => {
    const full = row.team ? `${row.team.display_name} #${row.label}` : row.label;
    return (
      full.toLowerCase().includes(filter) ||
      secondary.toLowerCase().includes(filter) ||
      routeLabel(row.route).toLowerCase().includes(filter)
    );
  });
}

/** `ambiguous_channel_labels`. */
function ambiguousChannelLabels(rows: RecentSidebarRow[]): Set<string> {
  const seen = new Set<string>();
  const ambiguous = new Set<string>();
  for (const recent of rows) {
    if (!recent.is_channel) continue;
    const label = recent.row.label.toLowerCase();
    if (seen.has(label)) ambiguous.add(label);
    seen.add(label);
  }
  return ambiguous;
}

/** `SidebarModel::selection_section`. */
function selectionSection(sidebar: SidebarModel, route: Route | undefined): string | null {
  const listed = (section: SidebarSection) => section.rows.some((row) => sameRoute(row.route, route));
  const canonical = sidebar.sections.find((section) => section.id !== "unread" && !section.collapsed && listed(section));
  const unread = sidebar.sections.find((section) => section.id === "unread" && listed(section));
  return (canonical ?? unread)?.id ?? null;
}

type Look = RowProps["look"];

function RecentList({ sidebar, avatars, current, look }: { sidebar: SidebarModel; avatars: SidebarAvatars | undefined; current: Route | undefined; look: Look }) {
  const rows = useMemo(() => visibleRecentRows(sidebar), [sidebar]);
  const ambiguous = useMemo(() => ambiguousChannelLabels(rows), [rows]);
  if (rows.length === 0) {
    return <div className="sidebar-empty">{sidebar.filter.trim() ? "No matching conversations" : "No conversations yet"}</div>;
  }
  return (
    <div className="sidebar-recent" data-sidebar-scroll>
      {rows.map((recent) => {
        const { row } = recent;
        const teamLabel =
          recent.is_channel && recent.secondary && ambiguous.has(row.label.toLowerCase()) ? recent.secondary : null;
        const label = look === "compact" && recent.is_channel ? `#${row.label}` : row.label;
        const size = look === "compact" ? ROW_AVATAR_SIZE_PX : PREVIEW_AVATAR_PX;
        return (
          <Row
            key={routeLabel(row.route)}
            row={row}
            label={label}
            suffix={teamLabel}
            selection={selectionOf(row, current, true)}
            highlighted={sameRoute(sidebar.highlighted_route, row.route)}
            look={look}
            live={liveFor(sidebar, row)}
            lead={(colors) =>
              row.team ? (
                <TeamAvatar name={row.team.display_name} asset={teamAsset(avatars, row.team.id)} size={size} />
              ) : (
                <Leading row={row} avatars={avatars} colors={colors} size={size} />
              )
            }
          />
        );
      })}
    </div>
  );
}

function liveFor(sidebar: SidebarModel, row: SidebarRow): PreviewLive | undefined {
  const key = conversationKey(row.route);
  return key ? sidebar.live_previews[key] : undefined;
}

/** Drag-and-drop of section headers (`DraggedSection`). */
let draggedSection: string | null = null;

function useSectionDrag(sectionId: string) {
  const [over, setOver] = useState(false);
  return {
    over,
    props: {
      draggable: true,
      onDragStart: (event: React.DragEvent) => {
        draggedSection = sectionId;
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", sectionId);
      },
      onDragEnd: () => {
        draggedSection = null;
        setOver(false);
      },
      onDragOver: (event: React.DragEvent) => {
        if (!draggedSection) return;
        event.preventDefault();
        setOver(true);
      },
      onDragLeave: () => setOver(false),
      onDrop: (event: React.DragEvent) => {
        event.preventDefault();
        setOver(false);
        if (draggedSection) send("shell", "reorder_section", { dragged_id: draggedSection, target_id: sectionId });
        draggedSection = null;
      },
    },
  };
}

function Disclosure({ collapsed }: { collapsed: boolean }) {
  return (
    <span className={`disclosure${collapsed ? "" : " expanded"}`}>
      <Icon name={collapsed ? "chevron-right" : "chevron-down"} size={14} />
    </span>
  );
}

function SectionHeader({ section }: { section: SidebarSection }) {
  const collapsible = section.id !== "unread";
  const drag = useSectionDrag(section.id);
  return (
    <div
      className={`sidebar-section-header${drag.over ? " drop-target" : ""}`}
      onClick={collapsible ? () => send("shell", "toggle_section", { section_id: section.id }) : undefined}
      {...(collapsible ? drag.props : {})}
    >
      <span>{section.title}</span>
      {collapsible && <Disclosure collapsed={section.collapsed} />}
    </div>
  );
}

type GroupContext = {
  sidebar: SidebarModel;
  avatars: SidebarAvatars | undefined;
  current: Route | undefined;
  look: Look;
  selected: string | null;
};

/** `Sidebar::render_row`: a row in a team or Conversations, in the chosen style. */
function GroupedRow({ sectionId, row, context }: { sectionId: string; row: SidebarRow; context: GroupContext }) {
  const { look, avatars } = context;
  const channelUnderTeam = !!row.team && isRoute(row.route, "Channel");
  return (
    <Row
      row={row}
      label={row.label}
      selection={selectionOf(row, context.current, context.selected === sectionId)}
      highlighted={sameRoute(context.sidebar.highlighted_route, row.route)}
      look={look}
      live={look === "compact" ? undefined : liveFor(context.sidebar, row)}
      lead={(colors, size) =>
        look !== "compact" && channelUnderTeam && row.team ? (
          <TeamAvatar name={row.team.display_name} asset={teamAsset(avatars, row.team.id)} size={size} />
        ) : (
          <Leading row={row} avatars={avatars} colors={colors} size={size} />
        )
      }
    />
  );
}

/** `render_unread_rows`: one compact line each, whatever the row style. */
function UnreadRows({ section, context }: { section: SidebarSection; context: GroupContext }) {
  const seen = new Set<string>();
  const ambiguous = new Set<string>();
  for (const row of section.rows) {
    if (!row.team) continue;
    const label = row.label.toLowerCase();
    if (seen.has(label)) ambiguous.add(label);
    seen.add(label);
  }
  return (
    <>
      {section.rows.map((row) => (
        <Row
          key={routeLabel(row.route)}
          row={row}
          label={row.team ? `#${row.label}` : row.label}
          suffix={row.team && ambiguous.has(row.label.toLowerCase()) ? row.team.display_name : null}
          selection={selectionOf(row, context.current, context.selected === section.id)}
          highlighted={sameRoute(context.sidebar.highlighted_route, row.route)}
          look="compact"
          live={undefined}
          lead={(colors) =>
            row.team ? (
              <TeamAvatar name={row.team.display_name} asset={teamAsset(context.avatars, row.team.id)} size={ROW_AVATAR_SIZE_PX} />
            ) : (
              <Leading row={row} avatars={context.avatars} colors={colors} size={ROW_AVATAR_SIZE_PX} />
            )
          }
        />
      ))}
    </>
  );
}

/** views/sidebar/team_group.rs. */
function TeamGroup({ section, context }: { section: SidebarSection; context: GroupContext }) {
  const team = section.rows.find((row) => row.team)?.team!;
  const drag = useSectionDrag(section.id);
  return (
    <div className="sidebar-section">
      <div
        className={`sidebar-team-header${drag.over ? " drop-target" : ""}`}
        onClick={() => send("shell", "toggle_section", { section_id: section.id })}
        {...drag.props}
      >
        <TeamAvatar name={team.display_name} asset={teamAsset(context.avatars, team.id)} size={16} />
        <span className="team-name">{team.display_name}</span>
        <Disclosure collapsed={section.collapsed} />
      </div>
      {!section.collapsed && (
        <div className={`sidebar-team-rows${context.look === "compact" ? " nested" : ""}`}>
          {section.rows.map((row) => (
            <GroupedRow key={routeLabel(row.route)} sectionId={section.id} row={row} context={context} />
          ))}
        </div>
      )}
    </div>
  );
}

function GroupedList({ context, orderHeader }: { context: GroupContext; orderHeader: ReactNode }) {
  const scroller = useRef<HTMLDivElement>(null);
  const elements: ReactNode[] = [];
  let previousIsTeam = false;
  let first = true;
  for (const section of context.sidebar.sections) {
    const isTeam = section.id.startsWith("group_");
    const gap = first ? FIRST_SECTION_GAP_PX : SECTION_GAP_PX;
    if (isTeam && !previousIsTeam) {
      elements.push(
        <div key="order-header" style={{ paddingTop: gap }}>
          {orderHeader}
        </div>,
      );
    }
    const top = isTeam ? (previousIsTeam ? 4 : 0) : gap;
    previousIsTeam = isTeam;
    first = false;
    const teamSection = isTeam && section.rows.some((row) => row.team);
    elements.push(
      <div key={section.id} style={{ paddingTop: top }}>
        {teamSection ? (
          <TeamGroup section={section} context={context} />
        ) : (
          <div className="sidebar-section">
            <SectionHeader section={section} />
            {section.id === "unread" ? (
              <UnreadRows section={section} context={context} />
            ) : (
              !section.collapsed &&
              section.rows.map((row) => <GroupedRow key={routeLabel(row.route)} sectionId={section.id} row={row} context={context} />)
            )}
          </div>
        )}
      </div>,
    );
  }
  // `auto_scroll_during_drag`: up to 8px a frame within 40px of an edge.
  const onDragOver = (event: React.DragEvent) => {
    const element = scroller.current;
    if (!element || !draggedSection) return;
    const bounds = element.getBoundingClientRect();
    const zone = 40;
    if (event.clientY < bounds.top + zone) {
      element.scrollTop -= 8 * Math.min(1, (bounds.top + zone - event.clientY) / zone);
    } else if (event.clientY > bounds.bottom - zone) {
      element.scrollTop += 8 * Math.min(1, (event.clientY - (bounds.bottom - zone)) / zone);
    }
  };
  return (
    <div ref={scroller} className="sidebar-list" data-sidebar-scroll onDragOver={onDragOver}>
      {elements}
    </div>
  );
}

type ToolbarButtonProps = {
  tooltip: string;
  shortcut?: string;
  icon: "chevron-left" | "chevron-right" | "search" | "plus" | "arrow-down-circle";
  on?: boolean;
  enabled?: boolean;
  width: number;
  height: number;
  onClick: () => void;
};

function ToolbarButton({ tooltip, shortcut, icon, on = false, enabled = true, width, height, onClick }: ToolbarButtonProps) {
  return (
    <button
      className={`toolbar-button${on ? " on" : ""}${enabled ? " enabled" : " disabled"}`}
      style={{ width, height }}
      title={shortcut ? `${tooltip}  ${shortcut}` : tooltip}
      disabled={!enabled}
      onClick={onClick}
    >
      <Icon name={icon} size={TOOLBAR_ICON_PX} />
    </button>
  );
}

/** `render_title_bar_row`: history and tools, right of the window buttons on macOS. */
function Toolbar({ width, updateAvailable, canGoBack, canGoForward, searchOpen }: { width: number; updateAvailable: boolean; canGoBack: boolean; canGoForward: boolean; searchOpen: boolean }) {
  const compact = width < (updateAvailable ? 260 : 208);
  const size = compact ? 24 : TOOLBAR_BUTTON_PX;
  const gap = compact ? 0 : 2;
  return (
    <div
      className="sidebar-toolbar"
      style={{ gap: compact ? 4 : 8, justifyContent: IS_MAC ? "flex-end" : "space-between", height: IS_MAC ? 44 : undefined }}
      data-tauri-drag-region
    >
      <div className="sidebar-toolbar-group" style={{ gap }}>
        <ToolbarButton tooltip="Back" shortcut={shortcutLabel("cmd-[")} icon="chevron-left" enabled={canGoBack} width={size - 4} height={size} onClick={() => send("shell", "navigate_back")} />
        <ToolbarButton tooltip="Forward" shortcut={shortcutLabel("cmd-]")} icon="chevron-right" enabled={canGoForward} width={size - 4} height={size} onClick={() => send("shell", "navigate_forward")} />
      </div>
      <div className="sidebar-toolbar-group" style={{ gap }}>
        <ToolbarButton tooltip="Jump to conversation" shortcut={shortcutLabel("cmd-k")} icon="search" on={searchOpen} width={size} height={size} onClick={() => send("overlays", "toggle_quick_switcher")} />
        <ToolbarButton tooltip="New message" shortcut={shortcutLabel("cmd-n")} icon="plus" width={size} height={size} onClick={() => send("overlays", "open_new_chat")} />
        {updateAvailable && (
          <ToolbarButton tooltip="Software update available" icon="arrow-down-circle" on width={size} height={size} onClick={() => send("shell", "show_updates")} />
        )}
      </div>
    </div>
  );
}

/** A menu opened and closed by the same press would reopen; skip that press. */
function useMenus() {
  const [menu, setMenu] = useState<Menu | null>(null);
  const dismissed = useRef<{ menu: Menu; at: number } | null>(null);
  return {
    menu,
    toggle(target: Menu) {
      const last = dismissed.current;
      dismissed.current = null;
      if (last && last.menu === target && performance.now() - last.at < 500) return;
      setMenu((open) => (open === target ? null : target));
    },
    close(byPressOutside: boolean) {
      if (byPressOutside && menu) dismissed.current = { menu, at: performance.now() };
      setMenu(null);
    },
  };
}

/** `order_header`: the list's label as a pop-up button that picks the order. */
function OrderHeader({ title, mode, open, onToggle, onClose }: { title: string; mode: SidebarModel["mode"]; open: boolean; onToggle: () => void; onClose: (outside: boolean) => void }) {
  return (
    <div className="sidebar-order">
      <div style={{ position: "relative" }}>
        <button className={`sidebar-order-trigger${open ? " open" : ""}`} title="Sidebar order" onClick={onToggle}>
          {title}
          <Icon name="chevrons-up-down" size={10} />
        </button>
        {open && (
          <PopupMenu
            style={{ top: 24, left: 0 }}
            onDismiss={() => onClose(true)}
            entries={[
              {
                label: "Sort by Recent",
                checked: mode === "Recent",
                shortcut: shortcutLabel("alt-cmd-1"),
                onClick: () => {
                  onClose(false);
                  send("shell", "set_sidebar_mode", { mode: "Recent" });
                },
              },
              {
                label: "Group by Team",
                checked: mode === "Grouped",
                shortcut: shortcutLabel("alt-cmd-2"),
                onClick: () => {
                  onClose(false);
                  send("shell", "set_sidebar_mode", { mode: "Grouped" });
                },
              },
            ]}
          />
        )}
      </div>
    </div>
  );
}

/** `render_profile_footer`: your name and the connection, opening the identity menu. */
function ProfileFooter({ connectivity, name, avatar, open, onToggle, onClose }: { connectivity: Connectivity; name: string; avatar: string | null; open: boolean; onToggle: () => void; onClose: (outside: boolean) => void }) {
  const status =
    connectivity === "Reconnecting"
      ? { label: "Connecting…", color: "var(--warning)", tooltip: "Connecting to Keybase…" }
      : connectivity === "Offline"
        ? { label: "Offline", color: "var(--text-secondary)", tooltip: "Keybase can't be reached · retrying automatically" }
        : null;
  return (
    <div className="sidebar-footer">
      <div style={{ position: "relative", flex: 1, minWidth: 0 }}>
        {open && (
          <PopupMenu
            style={{ bottom: "calc(100% + 4px)", left: 0 }}
            onDismiss={() => onClose(true)}
            entries={[
              {
                label: "View Profile",
                onClick: () => {
                  onClose(false);
                  send("shell", "open_own_profile");
                },
              },
              "separator",
              {
                label: "Settings…",
                shortcut: shortcutLabel("cmd-,"),
                onClick: () => {
                  onClose(false);
                  send("shell", "open_preferences");
                },
              },
            ]}
          />
        )}
        <button className={`sidebar-identity${open ? " open" : ""}`} onClick={onToggle}>
          <Avatar name={name} asset={avatar} size={24} background="var(--success-soft)" foreground="var(--identity-fg)" />
          <div style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
            <div className="name">{name}</div>
            {status && (
              <div className="sidebar-connection" title={status.tooltip}>
                <Icon name="cloud-off" size={12} color={status.color} />
                {status.label}
              </div>
            )}
          </div>
        </button>
      </div>
    </div>
  );
}

export function Sidebar({ widthPx }: { widthPx: number }) {
  const sidebar = useModel("sidebar");
  const app = useModel("app");
  const navigation = useModel("navigation");
  const shell = useModel("shell");
  const avatars = useModel("sidebar_avatars");
  const menus = useMenus();
  useMinuteTick();
  useRevealSelection(sidebar, navigation?.current);
  if (!sidebar || !app || !navigation) {
    return <nav className="sidebar" style={{ width: widthPx }} />;
  }
  const current = navigation.current;
  const look: Look =
    sidebar.style === "Compact" ? "compact" : { compactTime: widthPx < PREVIEWS_SIDEBAR_WIDTH_PX - 8 };
  const orderHeader = (title: string) => (
    <OrderHeader
      title={title}
      mode={sidebar.mode}
      open={menus.menu === "order"}
      onToggle={() => menus.toggle("order")}
      onClose={(outside) => menus.close(outside)}
    />
  );
  return (
    <nav className="sidebar" style={{ width: widthPx }}>
      <Toolbar
        width={widthPx}
        updateAvailable={!!shell?.update.has_reminder}
        canGoBack={navigation.back_stack.length > 0}
        canGoForward={navigation.forward_stack.length > 0}
        searchOpen={isRoute(current, "Search")}
      />
      {sidebar.mode === "Recent" ? (
        <>
          <div style={{ paddingTop: FIRST_SECTION_GAP_PX }}>{orderHeader("Recent")}</div>
          <RecentList sidebar={sidebar} avatars={avatars} current={current} look={look} />
        </>
      ) : (
        <GroupedList
          context={{ sidebar, avatars, current, look, selected: selectionSection(sidebar, current) }}
          orderHeader={orderHeader("Teams")}
        />
      )}
      <ProfileFooter
        connectivity={app.connectivity}
        name={app.current_user_display_name}
        avatar={app.current_user_avatar_asset}
        open={menus.menu === "identity"}
        onToggle={() => menus.toggle("identity")}
        onClose={(outside) => menus.close(outside)}
      />
    </nav>
  );
}

/**
 * `reveal_recent_sidebar_route`: centers the selection when the order or row
 * style changes and when the keyboard moves it; keeps it in view otherwise.
 */
function useRevealSelection(sidebar: SidebarModel | undefined, current: Route | undefined) {
  const layoutKey = sidebar ? `${sidebar.mode}/${sidebar.style}` : "";
  const highlighted = sidebar?.highlighted_route ? routeLabel(sidebar.highlighted_route) : "";
  const currentKey = current ? routeLabel(current) : "";
  const previous = useRef({ layoutKey, highlighted, currentKey });
  useLayoutEffect(() => {
    const before = previous.current;
    previous.current = { layoutKey, highlighted, currentKey };
    const centered = before.layoutKey !== layoutKey || before.highlighted !== highlighted;
    if (!centered && before.currentKey === currentKey) return;
    const target = centered && highlighted && before.highlighted !== highlighted ? highlighted : currentKey;
    const element =
      document.querySelector<HTMLElement>(`.sidebar [data-route="${CSS.escape(target)}"][data-selected]`) ??
      document.querySelector<HTMLElement>(`.sidebar [data-route="${CSS.escape(target)}"]`);
    element?.scrollIntoView({ block: centered ? "center" : "nearest" });
  }, [layoutKey, highlighted, currentKey]);
}

/** Re-renders relative times ("Yesterday") as days pass. */
export function useMinuteTick() {
  const [, setTick] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => setTick((tick) => tick + 1), 60_000);
    return () => window.clearInterval(timer);
  }, []);
}
