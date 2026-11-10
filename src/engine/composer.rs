//! Message composer: drafts, sending, editing, autocomplete, formatting, attachments and uploads.
//!
//! The text lives in the web view's textareas, which play the part of
//! `AppWindow`'s `composer_input` and `thread_input` `TextField`s. Every edit or
//! caret move arrives as [`Intent::Input`] (= `sync_drafts_from_inputs`), and
//! whenever the models change a draft behind the field's back (sending, editing,
//! switching chats, accepting a completion) the engine publishes a new text with
//! a higher `rev` for the field to adopt (= `sync_inputs_from_models`). Inputs
//! typed against an older `rev` are dropped, so a keystroke in flight cannot
//! overwrite the draft of the chat that replaced it.

use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use super::Engine;
use crate::{
    domain::{
        backend::ProviderMessageRef,
        conversation::{ConversationKind, ConversationSummary},
        ids::MessageId,
        message::MessageFragment,
    },
    models::{
        AppModels,
        composer_model::{AutocompleteState, ComposerMode, MarkdownFormat, apply_markdown_format},
        emoji_picker_model::EmojiPickerItem,
        file_upload_model::UploadTarget,
        navigation_model::RightPaneMode,
        store_sync::conversation_id_from_navigated_route,
        timeline_model::TimelineRow,
    },
    state::{
        ConnectionState, DraftKey, MessageBinding, UiAction, effect::BackendCommand,
        event::BackendEvent,
    },
    util::autocomplete_detect::{
        AutocompleteTriggerKind, TriggerMatch, detect_autocomplete_trigger,
    },
};

const INLINE_AUTOCOMPLETE_MAX_RESULTS: usize = 8;
const AUTOCOMPLETE_NAV_DEBOUNCE: Duration = Duration::from_millis(35);

/// Which field an intent is about: `ComposerTarget` in views/composer.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Conversation,
    Thread,
}

impl Target {
    fn upload_target(self) -> UploadTarget {
        match self {
            Self::Conversation => UploadTarget::Composer,
            Self::Thread => UploadTarget::Thread,
        }
    }

    fn from_upload_target(target: UploadTarget) -> Self {
        match target {
            UploadTarget::Composer => Self::Conversation,
            UploadTarget::Thread => Self::Thread,
        }
    }
}

/// `MarkdownFormat`, which has no `Deserialize`.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Bold,
    Italic,
    Strikethrough,
    Code,
    CodeBlock,
    Quote,
}

impl From<Format> for MarkdownFormat {
    fn from(format: Format) -> Self {
        match format {
            Format::Bold => Self::Bold,
            Format::Italic => Self::Italic,
            Format::Strikethrough => Self::Strikethrough,
            Format::Code => Self::Code,
            Format::CodeBlock => Self::CodeBlock,
            Format::Quote => Self::Quote,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    /// An overlay closed onto a conversation; refocus the chat (`restore_chat_focus`).
    RestoreFocus,
    /// The field's text or selection changed. Offsets are UTF-16, as in the DOM.
    Input {
        target: Target,
        rev: u64,
        text: String,
        selection_start: usize,
        selection_end: usize,
    },
    /// Which composer has keyboard focus, if any.
    Focus {
        target: Option<Target>,
    },
    /// Return in a composer or its send button; carries the field's text.
    Send {
        target: Target,
        rev: u64,
        text: String,
    },
    /// Up in an empty conversation composer.
    EditLast,
    /// "Edit" on a message (hover toolbar or context menu).
    EditMessage {
        message_id: String,
    },
    CancelEdit,
    /// "Retry" on a message that failed to send.
    RetrySend {
        message_id: String,
    },
    MoveAutocomplete {
        target: Target,
        direction: i32,
    },
    /// Tab or Return, or a click on a row (`index`).
    AcceptAutocomplete {
        target: Target,
        index: Option<usize>,
    },
    /// Escape while the list is open.
    DismissAutocomplete {
        target: Target,
    },
    ToggleFormatMenu {
        target: Target,
    },
    CloseFormatMenu,
    /// The format menu, or Cmd+B / Cmd+I / Cmd+E. Carries the field as the
    /// user sees it, so a toggle always unwraps what the last one wrapped.
    ApplyFormat {
        target: Target,
        format: Format,
        rev: u64,
        text: String,
        selection_start: usize,
        selection_end: usize,
    },
    /// The "@" button.
    InsertMention {
        target: Target,
    },
    /// The emoji button: `AppWindow::toggle_emoji_picker`.
    ToggleEmojiPicker,
    /// The paperclip button: the system file picker.
    OpenFilePicker {
        target: Target,
    },
    /// Files dropped on the conversation or thread pane.
    OpenFiles {
        target: Target,
        paths: Vec<String>,
    },
    /// An image pasted into a composer, base64-encoded PNG/TIFF/JPEG bytes.
    PasteImage {
        target: Target,
        data: String,
    },
    UploadCaption {
        caption: String,
    },
    UploadNext,
    UploadSend,
    UploadSendAll,
    UploadCancel,
    JoinChannel,
}

/// The engine's copy of one field. Only `rev`, `text` and the selection are
/// published; they are what the field adopts when `rev` goes up.
#[derive(Debug, Default, Serialize)]
pub struct FieldState {
    pub rev: u64,
    pub text: String,
    /// UTF-16 offsets to select after adopting `text`.
    pub selection_start: usize,
    pub selection_end: usize,
    /// What the field holds as far as the engine knows.
    #[serde(skip)]
    live_text: String,
    /// Byte offsets into `live_text`.
    #[serde(skip)]
    live_selection: (usize, usize),
    /// The conversation id or thread root the field is showing.
    #[serde(skip)]
    key: Option<String>,
    #[serde(skip)]
    dismissed_autocomplete: Option<DismissSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DismissSignature {
    trigger: char,
    trigger_offset: usize,
    cursor: usize,
    query: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct FocusRequest {
    pub seq: u64,
    /// `None` for the upload lightbox's caption field.
    pub target: Option<Target>,
}

/// Files chosen in the system picker, for the field that opened it.
type PickedFiles = (Target, Vec<PathBuf>);

/// State with no home in `AppModels`.
#[derive(Debug, Default, Serialize)]
pub struct ComposerState {
    pub conversation: FieldState,
    pub thread: FieldState,
    /// `AppWindow::composer_format_menu`.
    pub format_menu: Option<Target>,
    /// "Message #general", from `composer_placeholder`.
    pub placeholder: String,
    /// Asks the web view to focus a field.
    pub focus_request: Option<FocusRequest>,
    /// The focused composer, reported by the web view.
    #[serde(skip)]
    pub focused: Option<Target>,
    #[serde(skip)]
    last_autocomplete_nav: Option<(Target, i32, Instant)>,
    #[serde(skip)]
    picker: Option<(Sender<PickedFiles>, Receiver<PickedFiles>)>,
    #[serde(skip)]
    focus_seq: u64,
    /// The shell's navigation and focus requests already followed.
    #[serde(skip)]
    seen_shell: (u64, u64),
}

impl ComposerState {
    fn field(&mut self, target: Target) -> &mut FieldState {
        match target {
            Target::Conversation => &mut self.conversation,
            Target::Thread => &mut self.thread,
        }
    }

    fn request_focus(&mut self, target: Option<Target>) {
        self.focus_seq += 1;
        self.focus_request = Some(FocusRequest {
            seq: self.focus_seq,
            target,
        });
    }
}

pub fn handle(engine: &mut Engine, intent: Intent) {
    match intent {
        Intent::RestoreFocus => restore_chat_focus(engine),
        Intent::Input {
            target,
            rev,
            text,
            selection_start,
            selection_end,
        } => {
            if rev != engine.composer.field(target).rev {
                return;
            }
            apply_input(engine, target, text, selection_start, selection_end);
            sync_autocomplete(engine, target);
        }
        Intent::Focus { target } => engine.composer.focused = target,
        Intent::Send { target, rev, text } => {
            if rev == engine.composer.field(target).rev {
                let field = engine.composer.field(target);
                let caret = (text.len(), text.len());
                let selection = if field.live_text == text {
                    field.live_selection
                } else {
                    caret
                };
                field.live_text = text;
                field.live_selection = selection;
            }
            match target {
                Target::Conversation => send_composer_message(engine),
                Target::Thread => send_thread_reply(engine),
            }
        }
        Intent::EditLast => edit_last_own_message(engine),
        Intent::EditMessage { message_id } => edit_message(engine, MessageId::new(message_id)),
        Intent::CancelEdit => cancel_edit(engine),
        Intent::RetrySend { message_id } => {
            retry_failed_message_send(engine, MessageId::new(message_id))
        }
        Intent::MoveAutocomplete { target, direction } => {
            move_autocomplete_selection(engine, target, direction);
        }
        Intent::AcceptAutocomplete { target, index } => {
            accept_autocomplete_selection(engine, target, index);
        }
        Intent::DismissAutocomplete { target } => dismiss_autocomplete(engine, target),
        Intent::ToggleFormatMenu { target } => {
            engine.composer.format_menu = if engine.composer.format_menu == Some(target) {
                None
            } else {
                Some(target)
            };
        }
        Intent::CloseFormatMenu => engine.composer.format_menu = None,
        Intent::ApplyFormat {
            target,
            format,
            rev,
            text,
            selection_start,
            selection_end,
        } => {
            if rev == engine.composer.field(target).rev {
                apply_input(engine, target, text, selection_start, selection_end);
            }
            apply_format(engine, target, format.into());
        }
        Intent::InsertMention { target } => insert_mention(engine, target),
        Intent::ToggleEmojiPicker => engine.models.toggle_emoji_picker(),
        Intent::OpenFilePicker { target } => open_file_picker(engine, target),
        Intent::OpenFiles { target, paths } => {
            let paths = paths
                .into_iter()
                .filter(|path| !path.trim().is_empty())
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                open_file_upload_lightbox_with_paths(engine, paths, target);
            }
        }
        Intent::PasteImage { target, data } => handle_pasted_image(engine, target, &data),
        Intent::UploadCaption { caption } => {
            engine.models.file_upload_update_caption(caption);
        }
        Intent::UploadNext => {
            engine.models.file_upload_next();
            engine.composer.request_focus(None);
        }
        Intent::UploadSend => file_upload_send_current(engine),
        Intent::UploadSendAll => file_upload_send_all(engine),
        Intent::UploadCancel => {
            engine.models.cancel_file_upload_lightbox();
        }
        Intent::JoinChannel => join_current_channel(engine),
    }
    sync_inputs_from_models(engine);
    follow_shell_focus(engine);
}

/// Navigation restores chat focus, and the shell's toasts ("Jump to composer",
/// "Reply in thread") ask for a field, as `AppWindow` does with its handles.
fn follow_shell_focus(engine: &mut Engine) {
    let navigation_seq = engine.shell.navigation_seq;
    let request = engine
        .shell
        .focus_request
        .as_ref()
        .map(|request| (request.seq, request.target));
    let (seen_navigation, seen_request) = engine.composer.seen_shell;
    if let Some((seq, target)) = request
        && seq != seen_request
    {
        engine.composer.seen_shell.1 = seq;
        let target = match target {
            "thread" => Target::Thread,
            _ => Target::Conversation,
        };
        engine.composer.request_focus(Some(target));
        engine.mark_dirty();
    }
    if navigation_seq != seen_navigation {
        engine.composer.seen_shell.0 = navigation_seq;
        restore_chat_focus(engine);
        engine.mark_dirty();
    }
}

pub fn tick(engine: &mut Engine) {
    let picked = engine
        .composer
        .picker
        .as_ref()
        .and_then(|(_, receiver)| receiver.try_recv().ok());
    if let Some((target, paths)) = picked {
        open_file_upload_lightbox_with_paths(engine, paths, target);
        engine.mark_dirty();
    }
    sync_inputs_from_models(engine);
}

/// The focused composer, for the emoji picker: `emoji_picker_pick_item`
/// inserts into the thread when its field has focus, or when the thread pane
/// is open and the conversation field does not.
pub fn focused_target(engine: &Engine) -> Option<Target> {
    engine.composer.focused
}

/// `composer_insert_emoji` / `thread_insert_emoji`: types `text` at the caret,
/// closes the autocomplete list and dismisses overlays.
pub fn insert_text(engine: &mut Engine, target: Target, text: &str) {
    let field = engine.composer.field(target);
    let (start, end) = field.live_selection;
    let mut next = field.live_text.clone();
    next.replace_range(start..end, text);
    let caret = start + text.len();
    replace_text(engine, target, next, caret..caret);
    match target {
        Target::Conversation => engine.models.set_composer_autocomplete(None),
        Target::Thread => engine.models.set_thread_autocomplete(None),
    }
    engine.models.dismiss_overlays();
    engine.composer.request_focus(Some(target));
    engine.mark_dirty();
}

// --- Text sync -------------------------------------------------------------

/// `sync_drafts_from_inputs` for one field.
fn apply_input(
    engine: &mut Engine,
    target: Target,
    text: String,
    selection_start: usize,
    selection_end: usize,
) {
    let start = utf16_to_byte(&text, selection_start);
    let end = utf16_to_byte(&text, selection_end).max(start);
    let field = engine.composer.field(target);
    field.live_selection = (start, end);
    if field.live_text == text {
        return;
    }
    field.live_text = text.clone();
    update_draft(engine, target, text);
}

/// Model and store draft for `text`, as `sync_drafts_from_inputs` does.
fn update_draft(engine: &mut Engine, target: Target, text: String) {
    match target {
        Target::Conversation => {
            if engine.models.composer.draft_text == text {
                return;
            }
            let key = DraftKey::Conversation(engine.models.composer.conversation_id.clone());
            engine.models.update_composer_draft_text(text.clone());
            engine.dispatch(UiAction::UpdateDraft { key, text });
        }
        Target::Thread => {
            if engine.models.thread_pane.reply_draft == text {
                return;
            }
            engine.models.update_thread_reply_draft(text.clone());
            if let Some(root_id) = engine.models.thread_pane.root_message_id.clone() {
                engine.dispatch(UiAction::UpdateDraft {
                    key: DraftKey::Thread(root_id),
                    text,
                });
            }
        }
    }
}

/// Replaces the field's text from the engine side and selects `selection`
/// (byte offsets), like `TextField::replace_all_and_select`.
fn replace_text(
    engine: &mut Engine,
    target: Target,
    text: String,
    selection: std::ops::Range<usize>,
) {
    publish_text(engine.composer.field(target), &text, selection);
    update_draft(engine, target, text);
}

fn publish_text(field: &mut FieldState, text: &str, selection: std::ops::Range<usize>) {
    field.rev += 1;
    field.text = text.to_string();
    field.selection_start = byte_to_utf16(text, selection.start);
    field.selection_end = byte_to_utf16(text, selection.end);
    field.live_text = text.to_string();
    field.live_selection = (selection.start, selection.end);
}

/// `sync_inputs_from_models` and `sync_composer_placeholder`: when a draft
/// changed under a field, or the field now shows another chat or thread, hand
/// the field the model's text.
fn sync_inputs_from_models(engine: &mut Engine) {
    let conversation_key = Some(engine.models.composer.conversation_id.0.clone());
    let conversation_text = engine.models.composer.draft_text.clone();
    let switched_conversation = engine.composer.conversation.key != conversation_key;
    sync_field(
        &mut engine.composer.conversation,
        conversation_key,
        &conversation_text,
    );
    let thread_key = engine
        .models
        .thread_pane
        .root_message_id
        .as_ref()
        .map(|id| id.0.clone());
    let thread_text = engine.models.thread_pane.reply_draft.clone();
    sync_field(&mut engine.composer.thread, thread_key, &thread_text);

    let placeholder = composer_placeholder(&engine.models.conversation.summary);
    if engine.composer.placeholder != placeholder {
        engine.composer.placeholder = placeholder;
    }
    if switched_conversation && !engine.models.composer.conversation_id.0.is_empty() {
        engine.composer.format_menu = None;
        restore_chat_focus(engine);
    }
}

fn sync_field(field: &mut FieldState, key: Option<String>, text: &str) {
    if field.key == key && field.live_text == text {
        return;
    }
    field.key = key;
    field.dismissed_autocomplete = None;
    publish_text(field, text, text.len()..text.len());
}

/// `restore_chat_focus`: the conversation composer, unless something else
/// holds the keyboard.
fn restore_chat_focus(engine: &mut Engine) {
    let overlay = &engine.models.overlay;
    if overlay.quick_switcher_open
        || overlay.command_palette_open
        || overlay.emoji_picker_open
        || overlay.new_chat_open
        || overlay.fullscreen_image.is_some()
        || overlay.file_upload_lightbox.is_some()
        || overlay.profile_card_user_id.is_some()
        || engine.models.find_in_chat.open
        || !engine.models.conversation.can_post
    {
        return;
    }
    let target = if engine.models.navigation.right_pane == RightPaneMode::Thread
        && engine.composer.focused == Some(Target::Thread)
    {
        Target::Thread
    } else {
        Target::Conversation
    };
    engine.composer.request_focus(Some(target));
}

fn composer_placeholder(summary: &ConversationSummary) -> String {
    let title = summary.title.trim();
    if title.is_empty() {
        return "Write a message".to_string();
    }
    match summary.kind {
        ConversationKind::Channel => match summary.simple_team_label() {
            Some(team) => format!("Message {team}"),
            None => format!("Message #{title}"),
        },
        ConversationKind::DirectMessage | ConversationKind::GroupDirectMessage => {
            format!("Message {title}")
        }
    }
}

// --- Sending and editing ---------------------------------------------------

fn send_composer_message(engine: &mut Engine) {
    let composer_text = engine.composer.conversation.live_text.clone();
    engine
        .models
        .update_composer_draft_text(composer_text.clone());
    if let ComposerMode::Edit { message_id } = engine.models.composer.mode.clone() {
        let edited_text = engine.models.composer.draft_text.trim().to_string();
        if edited_text.is_empty() {
            delete_message(engine, message_id);
            engine.composer.request_focus(Some(Target::Conversation));
            return;
        }
        ensure_message_binding_for_active_conversation(engine, &message_id);
        engine.dispatch(UiAction::EditMessage {
            conversation_id: engine.models.composer.conversation_id.clone(),
            message_id,
            text: edited_text,
        });
        let (dispatch, _) = engine.models.send_composer_message();
        if matches!(dispatch, crate::models::SendDispatch::NotSent) {
            return;
        }
        engine.composer.request_focus(Some(Target::Conversation));
        return;
    }
    let draft_key = DraftKey::Conversation(engine.models.composer.conversation_id.clone());
    let store_draft_matches = engine
        .store
        .snapshot()
        .drafts
        .get(&draft_key)
        .map(|draft| draft.text.as_str())
        == Some(composer_text.as_str());
    if !store_draft_matches {
        engine.dispatch(UiAction::UpdateDraft {
            key: draft_key.clone(),
            text: composer_text,
        });
    }
    engine.dispatch(UiAction::SendMessage { key: draft_key });
    let (dispatch, _) = engine.models.send_composer_message();
    if matches!(dispatch, crate::models::SendDispatch::NotSent) {
        return;
    }
    engine.composer.request_focus(Some(Target::Conversation));
}

fn send_thread_reply(engine: &mut Engine) {
    let thread_text = engine.composer.thread.live_text.clone();
    engine.models.update_thread_reply_draft(thread_text.clone());
    let Some(root_id) = engine.models.thread_pane.root_message_id.clone() else {
        return;
    };
    let draft_key = DraftKey::Thread(root_id);
    let store_draft_matches = engine
        .store
        .snapshot()
        .drafts
        .get(&draft_key)
        .map(|draft| draft.text.as_str())
        == Some(thread_text.as_str());
    if !store_draft_matches {
        engine.dispatch(UiAction::UpdateDraft {
            key: draft_key.clone(),
            text: thread_text,
        });
    }
    engine.dispatch(UiAction::SendMessage { key: draft_key });
    let (dispatch, _) = engine.models.send_thread_reply();
    if matches!(dispatch, crate::models::SendDispatch::NotSent) {
        return;
    }
    engine.composer.request_focus(Some(Target::Thread));
}

/// `AppWindow::edit_message`, for the timeline's hover toolbar.
pub fn edit_message(engine: &mut Engine, message_id: MessageId) {
    if engine.models.edit_message(&message_id) {
        sync_inputs_from_models(engine);
        engine.composer.request_focus(Some(Target::Conversation));
    }
}

fn edit_last_own_message(engine: &mut Engine) {
    let Some(message_id) = engine.models.find_last_own_message_id() else {
        return;
    };
    engine.models.edit_message(&message_id);
}

fn cancel_edit(engine: &mut Engine) {
    engine.models.cancel_edit();
}

/// `AppWindow::delete_message`: what sending an emptied edit does.
fn delete_message(engine: &mut Engine, message_id: MessageId) {
    ensure_message_binding_for_active_conversation(engine, &message_id);
    if engine.models.delete_message(&message_id)
        && let Some(conversation_id) = engine.store.snapshot().timeline.conversation_id.clone()
    {
        engine.dispatch(UiAction::DeleteMessage {
            conversation_id,
            message_id,
        });
    }
}

fn ensure_message_binding_for_active_conversation(engine: &mut Engine, message_id: &MessageId) {
    let snapshot = engine.store.snapshot();
    if snapshot.backend.message_bindings.contains_key(message_id) {
        return;
    }
    let Some(conversation_id) = snapshot.timeline.conversation_id.clone() else {
        return;
    };
    let Some(conversation_binding) = snapshot
        .backend
        .conversation_bindings
        .get(&conversation_id)
        .cloned()
    else {
        return;
    };
    let binding = MessageBinding {
        message_id: message_id.clone(),
        backend_id: conversation_binding.backend_id,
        account_id: conversation_binding.account_id,
        provider_message_ref: ProviderMessageRef::new(message_id.0.clone()),
    };
    engine.router.register_message_binding(binding.clone());
    engine.store.register_message_binding(binding);
}

/// `AppWindow::retry_failed_message_send`: resend the failed text in place.
pub fn retry_failed_message_send(engine: &mut Engine, message_id: MessageId) {
    let retry_text = engine
        .models
        .timeline
        .rows
        .iter()
        .find_map(|row| match row {
            TimelineRow::Message(message_row) if message_row.message.id == message_id => {
                let text = message_row
                    .message
                    .fragments
                    .iter()
                    .filter_map(|fragment| match fragment {
                        MessageFragment::Text(value)
                        | MessageFragment::InlineCode(value)
                        | MessageFragment::Code { text: value, .. }
                        | MessageFragment::Quote(value) => Some(value.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(text.trim().to_string())
            }
            _ => None,
        });
    let Some(retry_text) = retry_text.filter(|text| !text.is_empty()) else {
        return;
    };
    engine.dispatch(UiAction::DiscardFailedMessage {
        message_id: message_id.clone(),
    });
    let draft_key = DraftKey::Conversation(engine.models.composer.conversation_id.clone());
    engine.models.composer.mode = ComposerMode::Compose;
    engine.models.composer.draft_text = retry_text.clone();
    engine.dispatch(UiAction::UpdateDraft {
        key: draft_key.clone(),
        text: retry_text,
    });
    engine.dispatch(UiAction::SendMessage { key: draft_key });
    engine.models.send_composer_message();
}

// --- Autocomplete ----------------------------------------------------------

fn autocomplete_state(models: &AppModels, target: Target) -> Option<&AutocompleteState> {
    match target {
        Target::Conversation => models.composer.autocomplete.as_ref(),
        Target::Thread => models.thread_pane.reply_autocomplete.as_ref(),
    }
}

fn set_autocomplete(models: &mut AppModels, target: Target, state: Option<AutocompleteState>) {
    match target {
        Target::Conversation => models.set_composer_autocomplete(state),
        Target::Thread => models.set_thread_autocomplete(state),
    }
}

/// `sync_autocomplete_for_target`.
fn sync_autocomplete(engine: &mut Engine, target: Target) {
    let field = engine.composer.field(target);
    let text = field.live_text.clone();
    let cursor = field.live_selection.1;
    let current_state = autocomplete_state(&engine.models, target).cloned();

    let detected = detect_autocomplete_trigger(&text, cursor);
    let detected_signature = detected.as_ref().map(|matched| DismissSignature {
        trigger: trigger_char(matched.kind),
        trigger_offset: matched.trigger_offset,
        cursor,
        query: matched.query.clone(),
    });

    // Escape keeps the list closed until the trigger, query or caret changes.
    let field = engine.composer.field(target);
    if let Some(dismissed) = field.dismissed_autocomplete.clone() {
        if detected_signature.as_ref() == Some(&dismissed) {
            if current_state.is_some() {
                set_autocomplete(&mut engine.models, target, None);
            }
            return;
        }
        field.dismissed_autocomplete = None;
    }

    let mut next_state = detected
        .and_then(|matched| build_autocomplete_state(engine, matched))
        .filter(|state| !state.candidates.is_empty());
    if let (Some(next), Some(current)) = (next_state.as_mut(), current_state.as_ref())
        && next.trigger == current.trigger
        && next.trigger_offset == current.trigger_offset
        && next.query == current.query
        && next.candidates == current.candidates
    {
        let max_ix = next.candidates.len().saturating_sub(1);
        next.selected_index = current.selected_index.min(max_ix);
    }
    if next_state == current_state {
        return;
    }
    set_autocomplete(&mut engine.models, target, next_state);
}

fn trigger_char(kind: AutocompleteTriggerKind) -> char {
    match kind {
        AutocompleteTriggerKind::Mention => '@',
        AutocompleteTriggerKind::Emoji => ':',
    }
}

fn build_autocomplete_state(engine: &Engine, matched: TriggerMatch) -> Option<AutocompleteState> {
    let candidates = match matched.kind {
        AutocompleteTriggerKind::Mention => engine
            .models
            .mention_autocomplete_candidates(&matched.query, INLINE_AUTOCOMPLETE_MAX_RESULTS),
        AutocompleteTriggerKind::Emoji => AppModels::emoji_autocomplete_candidates(
            &matched.query,
            &current_conversation_custom_emoji_items(engine),
            INLINE_AUTOCOMPLETE_MAX_RESULTS,
        ),
    };
    if candidates.is_empty() {
        return None;
    }
    Some(AutocompleteState {
        trigger: trigger_char(matched.kind),
        query: matched.query,
        trigger_offset: matched.trigger_offset,
        selected_index: 0,
        candidates,
    })
}

fn current_conversation_custom_emoji_items(engine: &Engine) -> Vec<EmojiPickerItem> {
    let snapshot = engine.store.snapshot();
    let supports_custom_emoji = snapshot
        .backend
        .accounts
        .values()
        .find(|account| matches!(account.connection_state, ConnectionState::Connected))
        .or_else(|| snapshot.backend.accounts.values().next())
        .map(|account| account.capabilities.supports_custom_emoji)
        .unwrap_or(false);
    if !supports_custom_emoji {
        return Vec::new();
    }
    let Some(conversation_id) = snapshot.timeline.conversation_id.as_ref() else {
        return Vec::new();
    };
    let mut custom_items = snapshot
        .backend
        .conversation_emojis
        .get(conversation_id)
        .into_iter()
        .flat_map(|index| index.values())
        .map(|emoji| EmojiPickerItem::Custom {
            alias: emoji.alias.clone(),
            unicode: emoji.unicode.clone(),
            asset_path: emoji.asset_path.clone(),
        })
        .collect::<Vec<_>>();
    custom_items.sort_by_key(|item| item.key());
    custom_items
}

/// `move_autocomplete_selection`, with its key-repeat debounce.
fn move_autocomplete_selection(engine: &mut Engine, target: Target, direction: i32) -> bool {
    let now = Instant::now();
    if let Some((last_target, last_direction, last_at)) = engine.composer.last_autocomplete_nav
        && last_target == target
        && last_direction == direction
        && now.duration_since(last_at) < AUTOCOMPLETE_NAV_DEBOUNCE
    {
        return false;
    }
    engine.composer.last_autocomplete_nav = Some((target, direction, now));
    let autocomplete = match target {
        Target::Conversation => &mut engine.models.composer.autocomplete,
        Target::Thread => &mut engine.models.thread_pane.reply_autocomplete,
    };
    let Some(state) = autocomplete.as_mut() else {
        return false;
    };
    let len = state.candidates.len();
    if len == 0 {
        return false;
    }
    let current = state.selected_index.min(len - 1);
    let next = (current as i32 + direction).rem_euclid(len as i32) as usize;
    if next == state.selected_index {
        return false;
    }
    state.selected_index = next;
    true
}

/// `accept_autocomplete_selection`: replaces the trigger and query with the
/// completion.
fn accept_autocomplete_selection(
    engine: &mut Engine,
    target: Target,
    selected_index_override: Option<usize>,
) -> bool {
    let Some(state) = autocomplete_state(&engine.models, target).cloned() else {
        return false;
    };
    if state.candidates.is_empty() {
        return false;
    }
    let selected_index = selected_index_override
        .unwrap_or(state.selected_index)
        .min(state.candidates.len() - 1);
    let Some(selected) = state.candidates.get(selected_index) else {
        return false;
    };
    let completion = selected.completion_text();
    let field = engine.composer.field(target);
    let cursor = field.live_selection.1;
    if state.trigger_offset > cursor || cursor > field.live_text.len() {
        return false;
    }
    let mut text = field.live_text.clone();
    text.replace_range(state.trigger_offset..cursor, &completion);
    let caret = state.trigger_offset + completion.len();
    set_autocomplete(&mut engine.models, target, None);
    replace_text(engine, target, text, caret..caret);
    engine.composer.request_focus(Some(target));
    true
}

/// Escape with the list open: close it and remember what it was for.
fn dismiss_autocomplete(engine: &mut Engine, target: Target) {
    let cursor = engine.composer.field(target).live_selection.1;
    let Some(signature) =
        autocomplete_state(&engine.models, target).map(|state| DismissSignature {
            trigger: state.trigger,
            trigger_offset: state.trigger_offset,
            cursor,
            query: state.query.clone(),
        })
    else {
        return;
    };
    set_autocomplete(&mut engine.models, target, None);
    engine.composer.field(target).dismissed_autocomplete = Some(signature);
}

// --- Formatting and the "@" button -----------------------------------------

/// `apply_composer_format`: wraps the selection, keeping it selected.
fn apply_format(engine: &mut Engine, target: Target, format: MarkdownFormat) {
    let field = engine.composer.field(target);
    let (start, end) = field.live_selection;
    let (text, selection) = apply_markdown_format(&field.live_text, start..end, format);
    engine.composer.format_menu = None;
    replace_text(engine, target, text, selection);
    engine.composer.request_focus(Some(target));
    sync_autocomplete(engine, target);
}

/// `composer_insert_mention`: an `@` at the caret, after a space when it
/// would otherwise run into a word, so the mention list opens.
fn insert_mention(engine: &mut Engine, target: Target) {
    let field = engine.composer.field(target);
    let (start, end) = field.live_selection;
    let needs_space = field.live_text[..start]
        .chars()
        .next_back()
        .is_some_and(|previous| !previous.is_whitespace());
    let insert = if needs_space { " @" } else { "@" };
    let mut text = field.live_text.clone();
    text.replace_range(start..end, insert);
    let caret = start + insert.len();
    engine.composer.format_menu = None;
    replace_text(engine, target, text, caret..caret);
    engine.composer.request_focus(Some(target));
    sync_autocomplete(engine, target);
}

// --- Attachments -----------------------------------------------------------

/// `open_file_upload_lightbox_via_picker`, with the system open panel run on
/// the main thread as AppKit requires.
fn open_file_picker(engine: &mut Engine, target: Target) {
    let sender = engine
        .composer
        .picker
        .get_or_insert_with(mpsc::channel)
        .0
        .clone();
    let result = engine.app.run_on_main_thread(move || {
        let Some(paths) = rfd::FileDialog::new().pick_files() else {
            return;
        };
        if !paths.is_empty() {
            let _ = sender.send((target, paths));
        }
    });
    if let Err(error) = result {
        tracing::warn!("could not open the file picker: {error}");
    }
}

/// `handle_pasted_image`: the image goes to a temp file, then the lightbox.
fn handle_pasted_image(engine: &mut Engine, target: Target, data: &str) {
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return;
    };
    if bytes.is_empty() {
        return;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("zbase-paste-{}-{ts}.png", std::process::id()));
    if std::fs::write(&path, &bytes).is_ok() {
        open_file_upload_lightbox_with_paths(engine, vec![path], target);
    }
}

fn open_file_upload_lightbox_with_paths(engine: &mut Engine, paths: Vec<PathBuf>, target: Target) {
    engine.models.timeline.hovered_message_id = None;
    // The lightbox previews images from wherever they are on disk.
    let scope = engine.app.asset_protocol_scope();
    for path in &paths {
        let _ = scope.allow_file(path);
    }
    if !engine
        .models
        .open_file_upload_lightbox(paths, target.upload_target())
    {
        return;
    }
    engine.composer.format_menu = None;
    engine.composer.request_focus(None);
}

fn file_upload_draft_key(engine: &Engine, target: UploadTarget) -> Option<DraftKey> {
    match target {
        UploadTarget::Composer => Some(DraftKey::Conversation(
            engine.models.composer.conversation_id.clone(),
        )),
        UploadTarget::Thread => engine
            .models
            .thread_pane
            .root_message_id
            .clone()
            .map(DraftKey::Thread),
    }
}

fn file_upload_send_current(engine: &mut Engine) {
    let payload = engine
        .models
        .overlay
        .file_upload_lightbox
        .as_ref()
        .and_then(|lightbox| {
            lightbox.current_candidate().map(|candidate| {
                (
                    lightbox.target,
                    candidate.path.to_string_lossy().to_string(),
                    candidate.filename.clone(),
                    candidate.caption.clone(),
                )
            })
        });
    if !engine.models.file_upload_send_current() {
        return;
    }
    let Some((target, local_path, filename, caption)) = payload else {
        return;
    };
    if let Some(key) = file_upload_draft_key(engine, target) {
        engine.dispatch(UiAction::SendAttachment {
            key,
            local_path,
            filename,
            caption,
        });
    }
    engine
        .composer
        .request_focus(Some(Target::from_upload_target(target)));
}

fn file_upload_send_all(engine: &mut Engine) {
    let payload = engine
        .models
        .overlay
        .file_upload_lightbox
        .as_ref()
        .map(|lightbox| {
            let items = lightbox
                .candidates
                .iter()
                .map(|candidate| {
                    (
                        candidate.path.to_string_lossy().to_string(),
                        candidate.filename.clone(),
                        candidate.caption.clone(),
                    )
                })
                .collect::<Vec<_>>();
            (lightbox.target, items)
        });
    if !engine.models.file_upload_send_all() {
        return;
    }
    let Some((target, items)) = payload else {
        return;
    };
    if let Some(key) = file_upload_draft_key(engine, target) {
        for (local_path, filename, caption) in items {
            engine.dispatch(UiAction::SendAttachment {
                key: key.clone(),
                local_path,
                filename,
                caption,
            });
        }
    }
    engine
        .composer
        .request_focus(Some(Target::from_upload_target(target)));
}

// --- Joining ---------------------------------------------------------------

/// `AppWindow::join_current_channel`.
fn join_current_channel(engine: &mut Engine) {
    let Some(conversation_id) =
        conversation_id_from_navigated_route(&engine.models.navigation.current)
    else {
        tracing::warn!("join_current_channel: no conversation_id from route");
        return;
    };
    let workspace_id = engine.models.app.active_workspace_id.clone();
    let Ok(events) = engine.router.route_command(BackendCommand::JoinChannel {
        workspace_id: workspace_id.clone(),
        conversation_id: conversation_id.clone(),
    }) else {
        engine.models.push_toast("Failed to join channel", None);
        return;
    };
    if !events
        .iter()
        .any(|event| matches!(event, BackendEvent::ChannelJoined { .. }))
    {
        engine.models.push_toast("Failed to join channel", None);
        return;
    }
    engine.apply_backend_events_inline(events);
    engine.apply_backend_events_inline(vec![BackendEvent::WorkspaceConversationsExtended {
        workspace_id: workspace_id.clone(),
        channels: vec![engine.models.conversation.summary.clone()],
        direct_messages: Vec::new(),
        conversation_bindings: Vec::new(),
    }]);
    engine.preview_summaries.remove(&conversation_id);
    if let Ok(workspace_events) = engine
        .router
        .route_command(BackendCommand::LoadWorkspace { workspace_id })
    {
        engine.apply_backend_events_inline(workspace_events);
    }
    engine.models.conversation.can_post = true;
    sync_inputs_from_models(engine);
    restore_chat_focus(engine);
}

// --- Offsets ---------------------------------------------------------------

/// A DOM (UTF-16) offset as a byte offset into `text`.
fn utf16_to_byte(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        if units >= offset {
            return index;
        }
        units += ch.len_utf16();
    }
    text.len()
}

fn byte_to_utf16(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text.char_indices()
        .take_while(|(index, _)| *index < offset)
        .map(|(_, ch)| ch.len_utf16())
        .sum()
}
