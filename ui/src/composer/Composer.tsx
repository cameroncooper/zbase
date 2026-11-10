// The message composer (views/composer, views/input) for the conversation and
// for a thread. The textarea is the field; the engine owns drafts, sending,
// autocomplete and formatting (engine/composer.rs).

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { readModel, send, useModel } from "../core/engine";
import { useShortcuts } from "../core/shortcuts";
import { Icon } from "../ui/Icon";
import { Tooltip, shortcutGlyphs } from "../ui/Tooltip";
import { registerDropZone } from "./dragDrop";
import { FileUploadLightbox } from "./FileUploadLightbox";
import type {
  AutocompleteCandidate,
  AutocompleteState,
  ComposerModel,
  FieldState,
  FileUploadLightboxModel,
  Target,
} from "./types";
import "./composer.css";

export type ComposerTarget = { kind: "conversation" } | { kind: "thread"; rootId: string };

/** 22px lines; the conversation field grows to 8 of them, the thread's to 6 (app/window.rs). */
const LINE_HEIGHT_PX = 22;
const MAX_LINES: Record<Target, number> = { conversation: 8, thread: 6 };

const FORMATS = [
  { format: "bold", label: "Bold", keys: "cmd-b", glyphs: "⌘B" },
  { format: "italic", label: "Italic", keys: "cmd-i", glyphs: "⌘I" },
  { format: "strikethrough", label: "Strikethrough" },
  { format: "code", label: "Code", keys: "cmd-e", glyphs: "⌘E" },
  { format: "code_block", label: "Code Block" },
  { format: "quote", label: "Quote" },
] as const;

type Connectivity = "Online" | "Reconnecting" | "Offline";

/** `Connectivity::send_notice().send_tooltip`. */
function offlineSendTooltip(connectivity: Connectivity | undefined): string | null {
  return connectivity === "Offline" ? "Offline · sending fails until Keybase reconnects" : null;
}

type OverlayLike = {
  new_chat_open?: boolean;
  quick_switcher_open?: boolean;
  command_palette_open?: boolean;
  emoji_picker_open?: boolean;
  fullscreen_image?: unknown;
  file_upload_lightbox?: FileUploadLightboxModel | null;
  profile_card_user_id?: string | null;
};

function anyOverlayOpen(): boolean {
  const overlay = readModel("overlay") as OverlayLike | undefined;
  if (!overlay) return false;
  return Boolean(
    overlay.new_chat_open ||
      overlay.quick_switcher_open ||
      overlay.command_palette_open ||
      overlay.emoji_picker_open ||
      overlay.fullscreen_image ||
      overlay.file_upload_lightbox ||
      overlay.profile_card_user_id,
  );
}

/** Focus requests already carried out, shared by both fields. */
let handledFocusSeq = 0;

/** The message composer for the conversation or a thread (views/composer, views/input). */
export function Composer({ target: composerTarget }: { target: ComposerTarget }) {
  const target: Target = composerTarget.kind;
  const composer = useModel("composer") as ComposerModel | undefined;
  const extra = useModel("composer_extra");
  const conversation = useModel("conversation") as
    | { can_post?: boolean; summary?: { title?: string } }
    | undefined;
  const threadPane = useModel("thread_pane") as
    | { reply_autocomplete?: AutocompleteState | null; width_px?: number }
    | undefined;
  const app = useModel("app") as { connectivity?: Connectivity } | undefined;
  const timeline = useModel("timeline") as { typing_text?: string | null } | undefined;
  const settings = useModel("settings") as { reduced_motion?: boolean } | undefined;
  const overlay = useModel("overlay") as OverlayLike | undefined;

  const field: FieldState | undefined = extra?.[target];
  const autocomplete =
    target === "conversation" ? (composer?.autocomplete ?? null) : (threadPane?.reply_autocomplete ?? null);
  const editing = target === "conversation" && composer && typeof composer.mode === "object" && "Edit" in composer.mode;
  const replying =
    target === "conversation" && composer && typeof composer.mode === "object" && "ReplyInThread" in composer.mode;
  const canPost = target === "thread" || conversation?.can_post !== false;
  const lightbox = overlay?.file_upload_lightbox ?? null;
  const lightboxHere = lightbox && (lightbox.target === "Composer") === (target === "conversation");

  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const appliedRev = useRef(-1);
  const lastSent = useRef({ text: "", start: 0, end: 0 });
  /** Set while the engine's text is written into the field, which fires input events. */
  const adopting = useRef(false);
  const [hasText, setHasText] = useState(false);
  const [rootWidth, setRootWidth] = useState(0);

  // --- Text sync with the engine -----------------------------------------

  const fitHeight = () => {
    const input = inputRef.current;
    if (!input) return;
    input.style.height = "0px";
    const max = LINE_HEIGHT_PX * MAX_LINES[target];
    input.style.height = `${Math.min(input.scrollHeight, max)}px`;
    input.style.overflowY = input.scrollHeight > max ? "auto" : "hidden";
  };

  /** Formats what the field shows now, not what the engine last heard. */
  const applyFormat = (format: string) => {
    const input = inputRef.current;
    if (!input) return;
    lastSent.current = { text: input.value, start: input.selectionStart, end: input.selectionEnd };
    send("composer", "apply_format", {
      target,
      format,
      rev: appliedRev.current,
      text: input.value,
      selection_start: input.selectionStart,
      selection_end: input.selectionEnd,
    });
  };

  /** `sync_drafts_from_inputs`: tell the engine what the field holds. */
  const report = () => {
    const input = inputRef.current;
    if (!input || appliedRev.current < 0 || adopting.current) return;
    const next = { text: input.value, start: input.selectionStart, end: input.selectionEnd };
    const last = lastSent.current;
    if (next.text === last.text && next.start === last.start && next.end === last.end) return;
    lastSent.current = next;
    send("composer", "input", {
      target,
      rev: appliedRev.current,
      text: next.text,
      selection_start: next.start,
      selection_end: next.end,
    });
  };

  // The field is replaced by the join bar while you can't post; a new one adopts the draft afresh.
  useLayoutEffect(() => {
    appliedRev.current = -1;
  }, [canPost]);

  // `sync_inputs_from_models`: adopt the engine's text when it set a new one.
  useLayoutEffect(() => {
    const input = inputRef.current;
    if (!input || !field || field.rev <= appliedRev.current) return;
    appliedRev.current = field.rev;
    if (input.value !== field.text) {
      if (document.activeElement === input) {
        // Through the editing commands, so Cmd+Z can undo a completion or format.
        // They fire input events with the caret at the end; the engine already
        // knows this text and selection, so don't report that interim state.
        adopting.current = true;
        try {
          input.select();
          if (field.text ? !document.execCommand("insertText", false, field.text) : !document.execCommand("delete")) {
            input.value = field.text;
          }
        } finally {
          adopting.current = false;
        }
      } else {
        input.value = field.text;
      }
    }
    input.setSelectionRange(field.selection_start, field.selection_end);
    lastSent.current = { text: field.text, start: field.selection_start, end: field.selection_end };
    setHasText(field.text.trim().length > 0);
    fitHeight();
  }, [field?.rev, field?.text, field?.selection_start, field?.selection_end, canPost]);

  useEffect(() => {
    const onSelectionChange = () => {
      if (document.activeElement === inputRef.current) report();
    };
    document.addEventListener("selectionchange", onSelectionChange);
    return () => document.removeEventListener("selectionchange", onSelectionChange);
  });

  // Overlays that open a conversation hand focus back to the chat.
  useEffect(() => {
    if (target !== "conversation") return;
    const restore = () => send("composer", "restore_focus");
    window.addEventListener("zbase:restore-chat-focus", restore);
    return () => window.removeEventListener("zbase:restore-chat-focus", restore);
  }, [target]);

  // Focus requests: after sending, editing, switching chats, joining.
  const focusRequest = extra?.focus_request;
  useEffect(() => {
    if (!focusRequest || focusRequest.seq <= handledFocusSeq || focusRequest.target !== target) return;
    handledFocusSeq = focusRequest.seq;
    inputRef.current?.focus();
  }, [focusRequest?.seq, canPost]);

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const observer = new ResizeObserver(() => setRootWidth(root.getBoundingClientRect().width));
    observer.observe(root);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const root = rootRef.current;
    return root ? registerDropZone(target, root) : undefined;
  }, [target]);

  useEffect(() => fitHeight(), [extra?.placeholder, rootWidth]);

  // --- Key bindings (app/key_bindings.rs, views/input) -------------------

  const focused = () => document.activeElement === inputRef.current && inputRef.current !== null;
  const currentAutocomplete = (): AutocompleteState | null => {
    if (target === "conversation") return (readModel("composer") as ComposerModel | undefined)?.autocomplete ?? null;
    return (readModel("thread_pane") as { reply_autocomplete?: AutocompleteState | null } | undefined)
      ?.reply_autocomplete ?? null;
  };
  const autocompleteOpen = () => (currentAutocomplete()?.candidates.length ?? 0) > 0;
  const isEditing = () => {
    const mode = (readModel("composer") as ComposerModel | undefined)?.mode;
    return typeof mode === "object" && "Edit" in mode;
  };
  const sendNow = () => {
    const input = inputRef.current;
    if (!input) return;
    report();
    send("composer", "send", { target, rev: appliedRev.current, text: input.value });
  };

  useShortcuts([
    {
      // ConfirmPrimary: accept the highlighted completion, else send.
      keys: "enter",
      when: focused,
      handler: () => {
        report();
        if (autocompleteOpen()) send("composer", "accept_autocomplete", { target, index: null });
        else sendNow();
      },
    },
    {
      keys: "tab",
      when: focused,
      handler: () => {
        report();
        if (autocompleteOpen()) send("composer", "accept_autocomplete", { target, index: null });
      },
    },
    {
      keys: "up",
      when: focused,
      handler: () => {
        if (autocompleteOpen()) {
          send("composer", "move_autocomplete", { target, direction: -1 });
          return;
        }
        // `up_at_top_triggered`: Up in an empty conversation field edits your last message.
        if (target === "conversation" && inputRef.current?.value === "") {
          send("composer", "edit_last");
          return;
        }
        return false;
      },
    },
    {
      keys: "down",
      when: () => focused() && autocompleteOpen(),
      handler: () => {
        send("composer", "move_autocomplete", { target, direction: 1 });
      },
    },
    {
      // DismissOverlays, the composer's part: the list, the format menu, then the edit.
      keys: "escape",
      handler: () => {
        if (focused() && autocompleteOpen()) {
          send("composer", "dismiss_autocomplete", { target });
          return;
        }
        if ((readModel("composer_extra") as { format_menu?: Target | null } | undefined)?.format_menu === target) {
          send("composer", "close_format_menu");
          return;
        }
        if (target === "conversation" && isEditing() && !anyOverlayOpen()) {
          send("composer", "cancel_edit");
          return;
        }
        return false;
      },
    },
    ...FORMATS.filter((entry) => "keys" in entry).map((entry) => ({
      keys: (entry as { keys: string }).keys,
      when: focused,
      handler: () => applyFormat(entry.format),
    })),
  ]);

  // --- Paste --------------------------------------------------------------

  /** `TextField::paste`: text goes in as typed; an image opens the upload lightbox. */
  const onPaste = (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const data = event.clipboardData;
    if (data.types.includes("text/plain")) return;
    const image = [...data.items].find((item) => item.kind === "file" && item.type.startsWith("image/"));
    const file = image?.getAsFile();
    if (!file) return;
    event.preventDefault();
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result ?? "");
      const comma = url.indexOf(",");
      if (comma >= 0) send("composer", "paste_image", { target, data: url.slice(comma + 1) });
    };
    reader.readAsDataURL(file);
  };

  // --- Render ---------------------------------------------------------------

  const canSend = hasText || (target === "conversation" && (composer?.attachments.length ?? 0) > 0);
  const formatMenuOpen = extra?.format_menu === target;
  const placeholder = target === "thread" ? "Reply in thread" : (extra?.placeholder ?? "Message this conversation");
  const sendTooltip = offlineSendTooltip(app?.connectivity);
  const typingLabel =
    target === "conversation" && app?.connectivity === "Online" && timeline?.typing_text?.trim()
      ? timeline.typing_text
      : null;

  const formatButton = (
    <button
      className={`zc-tool${formatMenuOpen ? " zc-tool-active" : ""}`}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => {
        report();
        send("composer", "toggle_format_menu", { target });
      }}
    >
      <span className="zc-format-label">Aa</span>
    </button>
  );

  const fieldElement = (
    <div
      className="zc-field"
      onMouseDown={(event) => {
        if (event.target === inputRef.current) return;
        event.preventDefault();
        inputRef.current?.focus();
      }}
    >
      <div className="zc-editor">
        <textarea
          ref={inputRef}
          rows={1}
          spellCheck
          placeholder={placeholder}
          onInput={(event) => {
            setHasText(event.currentTarget.value.trim().length > 0);
            fitHeight();
            report();
          }}
          onKeyUp={report}
          onMouseUp={report}
          onFocus={() => send("composer", "focus", { target })}
          onBlur={() => send("composer", "focus", { target: null })}
          onPaste={onPaste}
        />
      </div>
      <div className="zc-tools">
        <div className="zc-format-wrapper">
          {formatMenuOpen && <FormatMenu onApply={applyFormat} />}
          {formatMenuOpen ? formatButton : <Tooltip text="Formatting">{formatButton}</Tooltip>}
        </div>
        <Tooltip text="Mention someone">
          <button
            className="zc-tool"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => {
              inputRef.current?.focus();
              report();
              send("composer", "insert_mention", { target });
            }}
          >
            <Icon name="at-sign" size={16} />
          </button>
        </Tooltip>
        <Tooltip text="Attach files">
          <button
            className="zc-tool"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => send("composer", "open_file_picker", { target })}
          >
            <Icon name="paperclip" size={16} />
          </button>
        </Tooltip>
        <Tooltip text="Emoji">
          <button
            className="zc-tool"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => {
              report();
              send("composer", "toggle_emoji_picker");
            }}
          >
            <Icon name="emoji" size={16} />
          </button>
        </Tooltip>
        {canSend ? (
          <Tooltip
            text={sendTooltip ?? (target === "thread" ? "Send reply" : "Send")}
            shortcut={sendTooltip ? undefined : "↩"}
          >
            <button className="zc-send zc-send-enabled" onMouseDown={(event) => event.preventDefault()} onClick={sendNow}>
              <Icon name="send" size={16} />
            </button>
          </Tooltip>
        ) : (
          <div className="zc-send zc-send-disabled">
            <Icon name="send" size={16} />
          </div>
        )}
      </div>
    </div>
  );

  const popupWidth =
    target === "conversation"
      ? Math.min(Math.max(rootWidth - 32, 240), 520)
      : Math.min(Math.max((threadPane?.width_px ?? rootWidth) - 32, 220), 460);

  return (
    <div
      ref={rootRef}
      data-composer={target}
      className={`${target === "conversation" ? "zc-panel" : "zc-thread-panel"}${canPost ? "" : " zc-join"}`}
      data-reduced-motion={settings?.reduced_motion ? "" : undefined}
    >
      {canPost ? (
        <>
          {editing && <div className="zc-mode-banner">Editing · press Escape to cancel</div>}
          {replying && <div className="zc-mode-banner">Replying in thread</div>}
          {fieldElement}
          {target === "conversation" && (
            <div className="zc-typing-slot">{typingLabel && <TypingIndicator label={typingLabel} />}</div>
          )}
          {autocomplete && autocomplete.candidates.length > 0 && (
            <div
              className="zc-autocomplete-anchor"
              style={{ width: popupWidth, bottom: target === "conversation" ? 54 : 76 }}
            >
              <AutocompletePopup autocomplete={autocomplete} target={target} />
            </div>
          )}
        </>
      ) : (
        <JoinBar title={conversation?.summary?.title ?? ""} />
      )}
      {lightboxHere && lightbox && <FileUploadLightbox lightbox={lightbox} />}
    </div>
  );
}

/** "Aa": wraps the selection in Keybase Markdown (`format_tool`). */
function FormatMenu({ onApply }: { onApply: (format: string) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // `on_mouse_down_out`.
    const onMouseDown = (event: MouseEvent) => {
      const element = ref.current;
      if (element && !element.contains(event.target as Node)) send("composer", "close_format_menu");
    };
    window.addEventListener("mousedown", onMouseDown, true);
    return () => window.removeEventListener("mousedown", onMouseDown, true);
  }, []);
  return (
    <div ref={ref} className="zc-menu">
      {FORMATS.map((entry) => (
        <div
          key={entry.format}
          className="zc-menu-item"
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => onApply(entry.format)}
        >
          <div className="zc-menu-label">{entry.label}</div>
          {"glyphs" in entry && <div className="zc-menu-shortcut">{shortcutGlyphs(entry.glyphs)}</div>}
        </div>
      ))}
    </div>
  );
}

/** `render_autocomplete_popup`. */
function AutocompletePopup({ autocomplete, target }: { autocomplete: AutocompleteState; target: Target }) {
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const row = listRef.current?.children[autocomplete.selected_index] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  }, [autocomplete.selected_index]);
  return (
    <div ref={listRef} className="zc-autocomplete">
      {autocomplete.candidates.map((candidate, index) => (
        <div
          key={index}
          className={`zc-candidate${index === autocomplete.selected_index ? " zc-candidate-selected" : ""}`}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => send("composer", "accept_autocomplete", { target, index })}
        >
          <CandidateRow candidate={candidate} />
        </div>
      ))}
    </div>
  );
}

function CandidateRow({ candidate }: { candidate: AutocompleteCandidate }) {
  if ("MentionUser" in candidate) {
    const { username, display_name } = candidate.MentionUser;
    return (
      <div className="zc-candidate-split">
        <div className="zc-candidate-primary">{display_name}</div>
        <div className="zc-candidate-secondary">@{username}</div>
      </div>
    );
  }
  if ("MentionBroadcast" in candidate) {
    const { keyword, description } = candidate.MentionBroadcast;
    return (
      <div className="zc-candidate-split">
        <div className="zc-candidate-primary">@{keyword}</div>
        <div className="zc-candidate-secondary">{description}</div>
      </div>
    );
  }
  const { label, glyph } = candidate.Emoji;
  return (
    <div className="zc-candidate-emoji">
      <div className="zc-candidate-glyph">{glyph ?? "◻"}</div>
      <div className="zc-candidate-primary">{label}</div>
    </div>
  );
}

/** views/conversation `typing_indicator`: three pulsing dots and who is typing. */
function TypingIndicator({ label }: { label: string }) {
  return (
    <div className="zc-typing">
      <div className="zc-typing-dots">
        {[0, 1, 2].map((index) => (
          <div key={index} className="zc-typing-dot" style={{ animationDelay: `${index * 240}ms` }} />
        ))}
      </div>
      <div className="zc-typing-label">{label}</div>
    </div>
  );
}

/** views/conversation `render_join_bar`, in place of the composer when you can't post. */
function JoinBar({ title }: { title: string }) {
  const label = title.trim() ? `Join #${title.trim()}` : "Join Channel";
  return (
    <div className="zc-join-bar">
      <div className="zc-join-text">You can read this channel, but need to join to send messages.</div>
      <button className="zc-join-button" onClick={() => send("composer", "join_channel")}>
        {label}
      </button>
    </div>
  );
}
