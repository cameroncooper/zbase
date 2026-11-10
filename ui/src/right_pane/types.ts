// Shapes of the model sections the right pane renders (Rust field names).
// Sections owned by other areas (navigation, conversation, search, overlay)
// are read through the local views at the bottom, which list only the fields
// this area uses.

export type Affinity = "None" | "Positive" | "Broken";
export type Availability = "Active" | "Away" | "DoNotDisturb" | "Offline" | "Unknown";
export type Presence = { availability: Availability; status_text: string | null };

export type AttachmentKind = "Image" | "Video" | "Audio" | "File";
export type AttachmentSource = { Url: string } | { LocalPath: string };
export type AttachmentSummary = {
  name: string;
  kind: AttachmentKind;
  mime_type: string | null;
  size_bytes: number;
  width: number | null;
  height: number | null;
  preview: { source: AttachmentSource; width: number | null; height: number | null } | null;
  duration_ms: number | null;
  waveform: number[] | null;
  source: AttachmentSource | null;
};

/** `MessageRecord`; fields past the ones listed here are rendered by the timeline's row. */
export type MessageRecord = {
  id: string;
  conversation_id: string;
  author_id: string;
  timestamp_ms: number | null;
  fragments: unknown[];
  attachments: AttachmentSummary[];
  thread_reply_count: number;
  [field: string]: unknown;
};

export type UserSummary = {
  id: string;
  display_name: string;
  title: string;
  avatar_asset: string | null;
  presence: Presence;
  affinity: Affinity;
};

export type MessageRow = { author: UserSummary; message: MessageRecord; show_header: boolean };

export type ThreadPaneModel = {
  open: boolean;
  root_message_id: string | null;
  width_px: number;
  following: boolean;
  replies: MessageRecord[];
  reply_draft: string;
  reply_attachments: AttachmentSummary[];
  reply_autocomplete: unknown;
  loading: boolean;
};

export type SocialTab = "Followers" | "Following";
export type SocialListLoadState = "Loading" | "Failed";
export type ProofState = "Verified" | "Broken" | "Pending" | "Unknown";
export type IdentityProof = {
  service_name: string;
  service_username: string;
  proof_url: string | null;
  site_url: string | null;
  icon_asset: string | null;
  state: ProofState;
};
export type SocialGraphEntry = {
  user_id: string;
  display_name: string;
  avatar_asset: string | null;
  affinity: Affinity;
};
export type SocialGraph = {
  followers_count: number | null;
  following_count: number | null;
  is_following_you: boolean;
  you_are_following: boolean;
  followers: SocialGraphEntry[] | null;
  following: SocialGraphEntry[] | null;
};
export type TeamShowcaseEntry = { name: string; description: string; is_open: boolean; members_count: number };
export type CustomField = { label: string; value: string };
export type ProfileSection =
  | { IdentityProofs: IdentityProof[] }
  | { SocialGraph: SocialGraph }
  | { TeamShowcase: TeamShowcaseEntry[] }
  | { CustomFields: CustomField[] };
export type UserProfile = {
  user_id: string;
  username: string;
  display_name: string;
  avatar_asset: string | null;
  presence: Presence;
  affinity: Affinity;
  bio: string | null;
  location: string | null;
  title: string | null;
  sections: ProfileSection[];
};

export type ProfilePanelModel = {
  user_id: string | null;
  profile: UserProfile | null;
  loading: boolean;
  active_social_tab: SocialTab;
  social_list_status: SocialListLoadState | null;
};

export type FilesTab = "Media" | "Documents";
export type FileItem = {
  message_id: string;
  index: number;
  author: string;
  timestamp_ms: number | null;
  attachment: AttachmentSummary;
  thumbnail: string | null;
  is_media: boolean;
};
export type SearchHitContext = { index: number; author_name: string; author_avatar: string | null };

/** `engine::right_pane::RightPaneState`. */
export type RightPaneState = {
  thread_rows: MessageRow[];
  thread_reply_count: number;
  location: string | null;
  files_tab: FilesTab;
  files: FileItem[];
  files_has_older: boolean;
  files_loading_older: boolean;
  search_contexts: SearchHitContext[];
  window: { expanding: boolean; closing: boolean };
};

// Views of other areas' sections.

export type RightPaneMode = "Hidden" | "Thread" | "Details" | "Members" | "Files" | "Search" | { Profile: string };
export type NavigationView = { right_pane: RightPaneMode };

export type ChannelMemberPreview = {
  user_id: string;
  display_name: string;
  avatar_asset: string | null;
  affinity: Affinity;
  is_team_admin_or_owner: boolean;
  is_team_owner: boolean;
  availability: Availability | null;
};
export type ConversationKind = "Channel" | "DirectMessage" | "GroupDirectMessage";
export type AvatarParticipant = { display_name: string; asset: string | null; presence: Presence | null };
export type ConversationView = {
  summary: {
    id: string;
    title: string;
    kind: ConversationKind;
    muted: boolean;
    group: { display_name: string } | null;
  };
  details: {
    topic: string;
    member_count: number;
    members: ChannelMemberPreview[];
    group: { display_name: string } | null;
    can_leave: boolean;
    is_leaving: boolean;
    leave_error: string | null;
  } | null;
  avatar_asset: string | null;
  participant_avatars: AvatarParticipant[];
  member_count: number;
};

export type SearchResultView = {
  conversation_id: string;
  snippet: string;
  snippet_highlight_ranges: [number, number][];
  message: MessageRecord;
};
export type SearchView = {
  results: SearchResultView[];
  highlighted_index: number | null;
  is_loading: boolean;
};

export type OverlayView = { profile_card_user_id: string | null };
export type SettingsView = { show_right_pane: boolean };
