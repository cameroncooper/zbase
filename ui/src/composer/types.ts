// Shapes of the composer's model sections (models/composer_model.rs,
// models/file_upload_model.rs, engine/composer.rs).

export type ComposerMode =
  | "Compose"
  | { Edit: { message_id: string } }
  | { ReplyInThread: { root_id: string } };

export type AutocompleteCandidate =
  | { MentionUser: { username: string; display_name: string; avatar_asset: string | null } }
  | { MentionBroadcast: { keyword: string; description: string } }
  | { Emoji: { label: string; insert_text: string; glyph: string | null } };

export type AutocompleteState = {
  trigger: string;
  query: string;
  trigger_offset: number;
  selected_index: number;
  candidates: AutocompleteCandidate[];
};

export type AttachmentKind = "Image" | "Video" | "Audio" | "File";

/** `AttachmentSummary`; the composer only reads a few fields. */
export type AttachmentSummary = {
  name: string;
  kind: AttachmentKind;
  size_bytes: number;
  [field: string]: unknown;
};

export type ComposerModel = {
  conversation_id: string;
  mode: ComposerMode;
  draft_text: string;
  attachments: AttachmentSummary[];
  autocomplete: AutocompleteState | null;
};

export type FileUploadCandidate = {
  path: string;
  filename: string;
  kind: AttachmentKind;
  size_bytes: number;
  width: number | null;
  height: number | null;
  caption: string;
};

export type UploadTarget = "Composer" | "Thread";

export type FileUploadLightboxModel = {
  candidates: FileUploadCandidate[];
  current_index: number;
  target: UploadTarget;
};

export type Target = "conversation" | "thread";

/** One composer field as the engine last set it; adopt when `rev` goes up. */
export type FieldState = {
  rev: number;
  text: string;
  /** UTF-16 offsets. */
  selection_start: number;
  selection_end: number;
};

export type ComposerState = {
  conversation: FieldState;
  thread: FieldState;
  format_menu: Target | null;
  placeholder: string;
  /** `target: null` is the upload lightbox's caption. */
  focus_request: { seq: number; target: Target | null } | null;
};
