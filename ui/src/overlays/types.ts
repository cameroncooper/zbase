// Shapes of the model sections the overlays render (field names are the Rust ones).

import type { AvatarParticipant, Availability } from "../ui/Avatar";

/** `domain::route::Route`, externally tagged. */
export type Route =
  | { WorkspaceHome: { workspace_id: string } }
  | { Channel: { workspace_id: string; channel_id: string } }
  | { DirectMessage: { workspace_id: string; dm_id: string } }
  | { Search: { workspace_id: string; query: string } }
  | { Activity: { workspace_id: string } }
  | "Preferences"
  | { ActiveCall: { workspace_id: string; call_id: string } };

export type QuickSwitcherResultKind = "UnreadChannel" | "Channel" | "DirectMessage" | "Message";

export type QuickSwitcherResult = {
  label: string;
  sublabel: string | null;
  kind: QuickSwitcherResultKind;
  route: Route;
  conversation_id: string;
  message_id: string | null;
  match_ranges: [number, number][];
  author_id: string | null;
  timestamp_ms: number | null;
};

export type QuickSwitcherModel = {
  query: string;
  results: QuickSwitcherResult[];
  selected_index: number;
  loading_messages: boolean;
};

export type StockEmoji = { emoji: string; name: string; shortcode: string | null };
export type EmojiPickerItem =
  | { Stock: StockEmoji }
  | { Custom: { alias: string; unicode: string | null; asset_path: string | null } };

export type EmojiPickerModel = {
  query: string;
  selected_skin_tone: string | null;
  active_group: string | null;
  hovered: EmojiPickerItem | null;
  recent_aliases: string[];
  skin_tone_expanded: boolean;
};

export type AttachmentKind = "Image" | "Video" | "Audio" | "File" | string;
export type AttachmentSource = { Url: string } | { LocalPath: string };

export type AttachmentSummary = {
  name: string;
  kind: AttachmentKind;
  mime_type: string | null;
  size_bytes: number;
  width: number | null;
  height: number | null;
  source: AttachmentSource | null;
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
};

/** The parts of `MessageRecord` search rows read. */
export type SearchMessage = {
  id: string;
  conversation_id: string;
  author_id: string;
  timestamp_ms: number | null;
  attachments: AttachmentSummary[];
  link_previews: LinkPreview[];
  thread_reply_count: number;
};

export type SearchResult = {
  conversation_id: string;
  route: Route;
  snippet: string;
  snippet_highlight_ranges: [number, number][];
  message: SearchMessage;
};

export type FindInChatModel = {
  open: boolean;
  query: string;
  loading: boolean;
  conversation_id: string | null;
  anchor_timestamp_ms: number | null;
  query_seq: number;
  results: SearchResult[];
  selected_index: number | null;
};

export type SearchFilter = { FromUser: string } | { InChannel: string } | "HasFile" | "HasLink" | "MentionsMe";

export type SearchModel = {
  query: string;
  filters: SearchFilter[];
  results: SearchResult[];
  submitted_query: string | null;
  highlighted_index: number | null;
  is_loading: boolean;
};

export type Affinity = "None" | "Positive" | "Broken" | string;

export type UserSummary = {
  id: string;
  display_name: string;
  title: string;
  avatar_asset: string | null;
  presence: { availability: Availability; status_text: string | null };
  affinity: Affinity;
};

export type NewChatModel = {
  open: boolean;
  is_searching: boolean;
  search_query: string;
  search_results: UserSummary[];
  selected_participants: UserSummary[];
  creating: boolean;
  error: string | null;
  highlighted_index: number;
};

export type FullscreenImageOverlay = {
  source: AttachmentSource;
  caption: string | null;
  width: number | null;
  height: number | null;
};

export type OverlayModel = {
  new_chat_open: boolean;
  quick_switcher_open: boolean;
  command_palette_open: boolean;
  emoji_picker_open: boolean;
  reaction_target_message_id: string | null;
  fullscreen_image: FullscreenImageOverlay | null;
  /** Rendered by the composer area (`FileUploadLightboxModel`). */
  file_upload_lightbox: unknown | null;
  active_modal: string | null;
  profile_card_user_id: string | null;
  profile_card_position: [number, number] | null;
  sidebar_hover_tooltip: { text: string; anchor_x: number; anchor_y: number; width_px: number } | null;
  reaction_hover_tooltip: { text: string; anchor_x: number; anchor_y: number } | null;
};

// --- overlays_extra (engine/overlays.rs `OverlaysState`) ---

export type Segment = { text: string; matched: boolean };
export type SnippetRun = { text: string; matched: boolean; mention: "user" | "broadcast" | "channel" | null };

export type HitView = {
  author_name: string;
  author_avatar: string | null;
  location: string | null;
  time_label: string;
  full_time: string | null;
  snippet: SnippetRun[];
};

export type SwitcherRowView = {
  is_dm: boolean;
  primary: Segment[];
  team: Segment[] | null;
  simple_team: boolean;
  team_name: string | null;
  team_avatar: string | null;
  participants: AvatarParticipant[];
  hit: HitView | null;
};

export type EmojiCell = { key: string; glyph: string | null; asset: string | null; alias: string };
export type EmojiSection = { title: string; items: EmojiCell[] };
export type EmojiBookmark = { label: string; selected: boolean; group: string | null };
export type EmojiTone = { tone: string; color: string; selected: boolean };
export type EmojiFooter = { cell: EmojiCell; title: string; subtitle: string };

export type EmojiPickerView = {
  tones: EmojiTone[];
  bookmarks: EmojiBookmark[];
  sections: EmojiSection[];
  search_results: EmojiCell[] | null;
  footer: EmojiFooter | null;
};

/** The emoji the picker asks a composer to insert at its caret. */
export type EmojiInsert = { seq: number; target: "composer" | "thread"; text: string };

export type OverlaysState = {
  quick_switcher_query_seq: number;
  quick_switcher_indexing_status: string | null;
  quick_switcher_rows: SwitcherRowView[];
  search_rows: HitView[];
  emoji_picker: EmojiPickerView | null;
  emoji_insert: EmojiInsert | null;
};
