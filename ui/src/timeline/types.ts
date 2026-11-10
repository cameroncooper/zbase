// Shapes of the timeline's model sections (models/conversation_model.rs,
// models/timeline_model.rs, domain/message.rs) and of `timeline_extra`
// (engine/timeline.rs, engine/timeline/present.rs). Field names are the Rust ones.

export type Availability = "Active" | "Away" | "DoNotDisturb" | "Offline" | "Unknown";
export type Presence = { availability: Availability; status_text: string | null };
export type Affinity = "None" | "Positive" | "Broken";

export type UserSummary = {
  id: string;
  display_name: string;
  title: string;
  avatar_asset: string | null;
  presence: Presence;
  affinity: Affinity;
};

export type ConversationKind = "Channel" | "DirectMessage" | "GroupDirectMessage";

export type ConversationSummary = {
  id: string;
  title: string;
  topic: string;
  kind: ConversationKind;
  group: { id: string; display_name: string; team_type: "Unknown" | "Simple" | "Complex" } | null;
  [field: string]: unknown;
};

export type AvatarParticipant = {
  display_name: string;
  asset: string | null;
  presence: Presence | null;
};

export type PinnedItem = {
  id: string;
  target: { Message: { message_id: string } };
  pinned_by: string | null;
  pinned_at_ms: number | null;
  preview: { author_label: string | null; text: string | null } | null;
};

export type ConversationModel = {
  summary: ConversationSummary;
  pinned_message: PinnedItem | null;
  details: { member_count: number; [field: string]: unknown } | null;
  avatar_asset: string | null;
  participant_avatars: AvatarParticipant[];
  member_count: number;
  can_post: boolean;
  is_archived: boolean;
};

export type AttachmentKind = "Image" | "Video" | "Audio" | "File";
export type AttachmentSource = { Url: string } | { LocalPath: string };

export type AttachmentSummary = {
  name: string;
  kind: AttachmentKind;
  mime_type: string | null;
  size_bytes: number;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  source: AttachmentSource | null;
};

export type GithubPrState = "Open" | "Draft" | "Merged" | "Closed";
export type GithubReviewState = "Approved" | "ChangesRequested" | "ReviewRequired" | "NoReviews";

export type GithubPrStatus = {
  title: string;
  state: GithubPrState;
  merged_by: string | null;
  additions: number;
  deletions: number;
  checks: { passed: number; failed: number; running: number };
  review: GithubReviewState;
  unresolved_threads: number;
  total_threads: number;
  threads_complete: boolean;
};

export type GithubPrPreview = {
  pr: { owner: string; repo: string; number: number };
  status: GithubPrStatus | null;
  needs_auth: boolean;
  refreshing: boolean;
};

export type LinkPreview = {
  url: string;
  video_url: string | null;
  title: string | null;
  site: string | null;
  description: string | null;
  thumbnail_asset: string | null;
  is_media: boolean;
  media_width: number | null;
  media_height: number | null;
  is_video: boolean;
  github_pr: GithubPrPreview | null;
};

export type MessageSendState = "Sent" | "Pending" | "Failed";

export type MessageRecord = {
  id: string;
  conversation_id: string;
  author_id: string;
  reply_to: string | null;
  thread_root_id: string | null;
  timestamp_ms: number | null;
  event: unknown;
  link_previews: LinkPreview[];
  permalink: string;
  fragments: unknown[];
  source_text: string | null;
  attachments: AttachmentSummary[];
  thread_reply_count: number;
  send_state: MessageSendState;
  edited: { edit_id: string; edited_at_ms: number | null } | null;
};

export type MessageRow = { author: UserSummary; message: MessageRecord; show_header: boolean };

export type SystemEventIcon =
  | "Join"
  | "Leave"
  | "Add"
  | "Remove"
  | "Pin"
  | "Description"
  | "Settings"
  | "Info";

export type EventSpan =
  | { Actor: string }
  | { Text: string }
  | { ChannelLink: { channel_name: string; team_name: string | null; conv_id: string | null } }
  | { UserLink: string };

export type TimelineRow =
  | { DateDivider: string }
  | { UnreadDivider: string }
  | { SystemEvent: { icon: SystemEventIcon; spans: EventSpan[] } }
  | { Message: MessageRow }
  | { LoadingIndicator: string };

export type ThreadSummary = { repliers: UserSummary[]; last_reply_ms: number | null };

export type QuickReactRecent = { alias: string; unicode: string | null; asset_path: string | null };

export type TimelineModel = {
  conversation_id: string;
  current_user_id: string | null;
  rows: TimelineRow[];
  highlighted_message_id: string | null;
  editing_message_id: string | null;
  unread_marker: string | null;
  thread_summaries: Record<string, ThreadSummary>;
  pending_scroll_target: string | null;
  older_cursor: string | null;
  newer_cursor: string | null;
  loading_older: boolean;
  typing_text: string | null;
  quick_react_recent: QuickReactRecent | null;
  open_thread_root_id: string | null;
  [field: string]: unknown;
};

// timeline_extra

export type Role =
  | "text_primary"
  | "text_secondary"
  | "accent"
  | "accent_soft"
  | "panel_alt_bg"
  | "mention"
  | "mention_soft"
  | "mention_you"
  | "mention_you_soft"
  | "affinity_positive"
  | "affinity_positive_soft"
  | "affinity_broken"
  | "affinity_broken_soft"
  | "warning"
  | "warning_soft"
  | "inline_code_foreground"
  | "inline_code_background"
  | "syntax_keyword"
  | "syntax_function"
  | "syntax_string"
  | "syntax_number"
  | "syntax_type"
  | "syntax_comment"
  | "syntax_punctuation";

export type Run = {
  text: string;
  color?: Role;
  bg?: Role;
  bold?: boolean;
  italic?: boolean;
  mono?: boolean;
  strike?: boolean;
  meta?: boolean;
  link?: string;
  image?: { src: string; fallback: string; size: number };
};

export type Block =
  | { kind: "inline"; runs: Run[] }
  | { kind: "code"; runs: Run[] }
  | { kind: "quote"; runs: Run[] };

export type ReactionView = {
  emoji: string;
  unicode: string | null;
  asset: string | null;
  count: number;
  mine: boolean;
  tooltip: string | null;
};

export type AttachmentView = {
  kind: AttachmentKind;
  name: string;
  size_label: string | null;
  display_label: string;
  image: string | null;
  animated: boolean;
  preview_width: number | null;
  preview_height: number | null;
  full_width: number | null;
  full_height: number | null;
  lightbox: boolean;
  video_path: string | null;
  duration_label: string | null;
  download_source: string | null;
  upload: { label: string; fraction: number } | null;
};

export type MessageView = {
  time: string;
  timestamp: string;
  timestamp_tooltip: string | null;
  blocks: Block[];
  emoji_only: boolean;
  hide_body: boolean;
  edited_line: boolean;
  caption: string | null;
  reply: { author: string; excerpt: string } | null;
  reactions: ReactionView[];
  last_reply: string | null;
  own: boolean;
  attachments: AttachmentView[];
  hidden_previews: number;
};

export type ScrollTarget =
  | { kind: "none" }
  | { kind: "bottom" }
  | { kind: "message"; message_id: string; offset: number }
  | { kind: "anchor"; message_id: string | null; row_index: number };

export type DownloadState =
  | { state: "downloading" }
  | { state: "saved"; path: string }
  | { state: "failed"; error: string }
  | { state: "reveal"; path: string };

export type TimelineState = {
  jump_to_message_active: boolean;
  messages: Record<string, MessageView>;
  header: {
    title: string;
    subtitle: string | null;
    kind: "channel" | "dm" | "group";
    team_name: string;
    member_count: number;
  };
  connection_banner: { title: string; detail: string; offline: boolean } | null;
  pinned: { text: string; unavailable: boolean } | null;
  unseen_count: number;
  scroll: { seq: number; target: ScrollTarget };
  older_restore_pending: boolean;
  downloads: Record<string, DownloadState>;
};
