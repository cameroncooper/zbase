// Shapes of the shell's model sections, as serde serializes the Rust models
// (src/models/*.rs, tauri/src-tauri/src/engine/shell.rs). Field names are the
// Rust ones; ids are plain strings; unit enum variants are strings.

export type Connectivity = "Online" | "Reconnecting" | "Offline";

/** `domain::route::Route`. */
export type Route =
  | { WorkspaceHome: { workspace_id: string } }
  | { Channel: { workspace_id: string; channel_id: string } }
  | { DirectMessage: { workspace_id: string; dm_id: string } }
  | { Search: { workspace_id: string; query: string } }
  | { Activity: { workspace_id: string } }
  | "Preferences"
  | { ActiveCall: { workspace_id: string; call_id: string } };

export type AppModel = {
  open_workspaces: string[];
  active_workspace_id: string;
  connectivity: Connectivity;
  global_unread_count: number;
  current_user_id: string | null;
  current_user_display_name: string;
  current_user_avatar_asset: string | null;
};

export type ConversationKind = "Channel" | "DirectMessage" | "GroupDirectMessage";

export type ConversationGroup = {
  id: string;
  display_name: string;
  team_type: "Unknown" | "Simple" | "Complex";
};

export type ConversationSummary = {
  id: string;
  title: string;
  kind: ConversationKind;
  topic: string;
  group: ConversationGroup | null;
  unread_count: number;
  mention_count: number;
  muted: boolean;
  last_activity_ms: number;
  last_message: unknown | null;
};

export type WorkspaceModel = {
  workspace_id: string;
  workspace_name: string;
  channels: ConversationSummary[];
  direct_messages: ConversationSummary[];
};

export type RightPaneMode =
  | "Hidden"
  | "Thread"
  | "Details"
  | "Members"
  | "Files"
  | "Search"
  | { Profile: string };

export type NavigationModel = {
  current: Route;
  back_stack: Route[];
  forward_stack: Route[];
  right_pane: RightPaneMode;
};

export type SidebarMode = "Recent" | "Grouped";
export type SidebarStyle = "Compact" | "Previews";

export type SidebarPreview = {
  prefix: string | null;
  text: string;
  timestamp_ms: number | null;
};

export type PreviewLive = { Typing: string } | { Draft: string } | "NotSent";

export type SidebarRow = {
  label: string;
  team: ConversationGroup | null;
  unread_count: number;
  mention_count: number;
  route: Route;
  preview: SidebarPreview | null;
  muted: boolean;
};

export type SidebarSection = {
  id: string;
  title: string;
  rows: SidebarRow[];
  collapsed: boolean;
};

export type RecentSidebarRow = {
  row: SidebarRow;
  secondary: string;
  is_channel: boolean;
};

export type SidebarModel = {
  sections: SidebarSection[];
  recent_rows: RecentSidebarRow[];
  mode: SidebarMode;
  style: SidebarStyle;
  filter: string;
  highlighted_route: Route | null;
  width_px: number;
  live_previews: Record<string, PreviewLive>;
};

export type ToastAction =
  | "OpenPreferences"
  | "FocusComposer"
  | "OpenThread"
  | "FocusThreadReply"
  | "OpenCurrentConversation"
  | "OpenActiveCall";

export type ToastNotification = { title: string; action: ToastAction | null };

export type ActivityKind = "Mention" | "ThreadReply" | "Reaction" | "Reminder";

export type ActivityItem = {
  kind: ActivityKind;
  title: string;
  detail: string;
  route: Route;
  message_id: string | null;
  unread: boolean;
};

export type NotificationsModel = {
  toasts: ToastNotification[];
  notification_center_count: number;
  activity_items: ActivityItem[];
  highlighted_index: number | null;
};

export type ThemeMode = "Light" | "Dark" | "System";
export type Density = "Comfortable" | "Compact";

export type SettingsModel = {
  theme_mode: ThemeMode;
  density: Density;
  reduced_motion: boolean;
  show_right_pane: boolean;
  sidebar_style: SidebarStyle;
  emoji_skin_tone: string | null;
  emoji_recents: string[];
  sidebar_modes: Record<string, SidebarMode>;
  sidebar_section_order: Record<string, string[]>;
  sidebar_collapsed_sections: Record<string, string[]>;
  dismissed_pinned_items: Record<string, string>;
  quick_switcher_affinity: Record<string, [number, number]>;
};

export type Availability = "Active" | "Away" | "DoNotDisturb" | "Offline" | "Unknown";
export type Presence = { availability: Availability; status_text: string | null };

export type CallStatus =
  | "Idle"
  | "Ringing"
  | "Connecting"
  | "ActiveAudio"
  | "ActiveVideo"
  | "SharingScreen"
  | "Reconnecting";

export type CallModel = {
  active_call: {
    call_id: string;
    status: CallStatus;
    participants: { id: string; display_name: string; title: string; avatar_asset: string | null }[];
  } | null;
  is_muted: boolean;
  is_sharing_screen: boolean;
};

/** `services::updates::UpdateSnapshot`. */
export type UpdateSnapshot = {
  state: string;
  version: string;
  notes: string;
  release_url: string;
  message: string;
  received: number;
  total: number;
  attention: boolean;
  informational: boolean;
  automatic_checks: boolean;
  automatic_downloads: boolean;
  last_check: number;
};

export type InspectorEntry = {
  method: string;
  count: number;
  last_seen_seq: number;
  payload_preview: string | null;
};

/** `engine::shell::ShellState`. */
export type ShellState = {
  splash_open: boolean;
  splash_status: string;
  version: string;
  update: {
    snapshot: UpdateSnapshot;
    open: boolean;
    notes_open: boolean;
    error: string | null;
    has_reminder: boolean;
  };
  inspector: {
    open: boolean;
    paused: boolean;
    unknown_only: boolean;
    seq: number;
    method_count: number;
    rows: InspectorEntry[];
  };
  navigation_seq: number;
  focus_request: { seq: number; target: "composer" | "thread" } | null;
};

export type AvatarParticipant = {
  display_name: string;
  asset: string | null;
  presence: Presence | null;
};

/** `models::sidebar_model::SidebarAvatar`. */
export type SidebarAvatar = { Participants: AvatarParticipant[] } | { Team: string };

/** `StoreSync::sidebar_conversation_avatars`, keyed by `Route::label()` or `sidebar-team:<id>`. */
export type SidebarAvatars = Record<string, SidebarAvatar>;
