# Parity with the gpui interface

zBase's interface moved from gpui to Tauri. This file records, feature by
feature, how the Tauri interface matches the gpui one it replaced. The gpui
sources named below (`views/...`, `app/...`, `AppWindow`) were removed; read
them at commit `3ed2b22`, the last commit with the gpui interface.

- `[x]` works the same against a live Keybase account
- `[~]` ported, but partial or not yet seen working live (the note says which)
- `[ ]` not ported

Architecture: `src/engine` owns the same `AppStore`, `BackendRouter` and
`AppModels` the gpui `AppWindow` did, and projects the store into the models
with the shared `models::store_sync::StoreSync`. Every model section is
serialized to the web view as a `model:<section>` event; the web view sends
`Intent`s back. Each area below has an engine module (`src/engine/<area>.rs`)
and a frontend folder (`ui/src/<area>/`).

## Shell (`src/engine/shell.rs`, `ui/src/shell/`)

Other areas navigate through `shell::navigate_to`, `shell::navigate_to_message`,
`shell::open_url_or_deep_link` and `shell::dismiss_overlays` (Rust), so
Settings, focus (`shell.navigation_seq`, `shell.focus_request`) and the shell's
layers stay in step.

- [~] Boot, splash screen and connection states (views/splash, `check_splash_dismiss`): boot status copy, 2s minimum, closes once on Ready; Cmd+Shift+S toggles it; the window stays hidden until its bounds are restored. Boot verified live; the splash itself not yet seen on screen (boot finished before the capture)
- [~] Window layout, resizable sidebar, minimum sizes (`AppWindow::render`, views/mod.rs constants): sidebar | 4px handle | main (min 560) | right pane, 800x680 minimum, 160–360px drag, `fit_window_to_sidebar` on style change. Window widening for side panes is the right pane's (`engine/right_pane/pane_window.rs`)
- [x] Titlebar: back/forward, connectivity, update badge. gpui has no titlebar or rail view any more (views/titlebar and views/rail are not compiled); these live in the sidebar toolbar and footer, ported as such
- [x] Rail: not used by gpui (see above); nothing to port
- [x] Sidebar, Recent mode: rows, unread dots, mention and DM badges, ambiguous team suffix, typing, drafts, failed sends (views/sidebar/recent.rs, `sync_sidebar_live_previews`)
- [~] Sidebar, Grouped mode: sections, collapse, drag to reorder (with edge auto-scroll), team groups, Grouped Unread, single selection (`selection_section`). Team groups and selection reveal verified live; collapse and drag-reorder not yet exercised
- [x] Sidebar row styles: names only or with previews, compact times below 272px (views/sidebar/preview_row.rs)
- [x] Sidebar avatars for DMs, group DMs (pair or count) and teams, presence dots by shape (views/avatar)
- [~] Sidebar filter: `set_sidebar_filter` intent, filtering and "No matching conversations" ported. gpui draws no filter field (the input exists but is never rendered), so neither does this
- [x] Sidebar pop-up menu to switch Recent / Teams, and the identity menu (views/popup_menu.rs), with the press-outside-doesn't-reopen guard
- [~] Home and Activity views (views/home, views/inbox): ported as they are in gpui (both are placeholder pages there); home's Search button calls the overlays' `submit_search`
- [x] Navigation history: back/forward (Cmd+[ / Cmd+], toolbar buttons with disabled states)
- [~] Cmd+1..5 (recent results while the quick switcher is open with an empty query, else Cmd+1 goes Home), Alt+Up/Down and Alt+Enter sidebar selection with reveal. Implemented; not yet exercised live
- [~] Deep links `keybase://chat/...`, `zbase-mention:`, `zbase-channel:` (`open_url_or_deep_link`), from any `<a href>` in the web view or `shell::open_url_or_deep_link`. No OS-level `keybase://` URL handler (gpui has none either)
- [x] Theme (light/dark/system via `prefers-color-scheme`), density and reduced motion as `data-theme`, `data-density`, `data-reduced-motion` on `<html>`; native window appearance follows the theme
- [x] Preferences / settings window: a second Tauri window (`?window=settings`, 640x780, fixed size, centered 48px below the main window's top) with every section of views/preferences
- [x] Settings persistence (`engine.persist_settings()` wherever `AppWindow` saves) and per-account sidebar order and collapse state
- [~] Remembered window size and position: same file and rules as gpui (services/window_preferences*, constrained to a connected display, flushed on close and quit). Saved sizes exclude the width a pane added and skip the app's own pane resizes, as gpui's `base_width` did; not yet checked live
- [~] Toasts and their actions: gpui keeps toasts in the model but never draws them; here they stack bottom-right for 5s with their action and a dismiss button. FocusComposer / FocusThreadReply publish `shell.focus_request` for the composer to act on
- [x] macOS menu bar: zBase (Check for Updates…, Settings… ⌘,, Services, Hide, Quit ⌘Q), View › Sort by Recent ⌥⌘1 / Group by Team ⌥⌘2 with check marks. Added Edit and Window menus, which a web view needs for copy/paste and ⌘W
- [~] Dock badge (`sync_dock_badge` through `set_badge_count`): implemented, not yet observed on the Dock
- [~] Updates UI: panel, toolbar badge, Settings rows, reminders, restart blockers. Official builds link Sparkle again (`macos-updater`; calls hop to the main thread, which the bridge asserts). Source builds show the "disabled" Homebrew/releases copy, as in gpui. A Sparkle-enabled build has not been run against the update feed yet; release notes render as plain text
- [~] Calls (views/calls): call window route and mini dock with Open/Mute/Share/Leave, as placeholders as in gpui (the Keybase backend has no calls)
- [~] Keybase inspector (Cmd+Shift+I): records `KeybaseNotifyStub` methods while open, pause, unknown-only, clear, copy. Benchmark capture (Cmd+Shift+B) not ported: `PerfHarness` times gpui render phases that don't exist here

## Timeline (`src/engine/timeline.rs`, `ui/src/timeline/`)

Rendering is precomputed in the engine (`engine/timeline/present.rs`: markdown,
emoji, mentions, find highlights, code highlighting, labels, reactions,
attachments) and published as `timeline_extra.messages`; `src/timeline/MessageRow.tsx`
draws a row and is shared with the thread pane. Seen working against a live
account: header, quotes, bold, emoji, continuation rows, bottom pinning. The
rest is built and type-checked but not yet clicked through live, so it is `[~]`.

- [~] Conversation header: title, topic, avatar, member count, pane toggles (views/conversation). Toggles send right_pane `toggle_pane`, find sends overlays `toggle_find_in_chat`; not clicked through live
- [~] Pinned message banner with dismiss. Not clicked through live
- [~] Message rows: grouping, headers, timestamps, edited marker, send states (views/timeline). Rows seen live; pending and failed states not exercised
- [~] Date dividers, unread divider, system events, loading indicator. Seen live except the loading indicator
- [~] Inline markdown: bold, italic, strike, spoilers, inline code, code blocks, quotes (views/inline_markdown.rs, copied into engine/timeline/inline_markdown.rs). Bold and quotes seen live
- [~] Code block syntax highlighting (views/code_highlight.rs, tree-sitter in engine/timeline/code_highlight.rs). Built; not seen live
- [~] Mentions, channel mentions, broadcasts, links; clicking mentions opens profiles/channels. Clicks go through `shell::open_url_or_deep_link`; non-member channels are resolved through the backend
- [~] Emoji: stock, custom, emoji-only messages at large size. Stock seen live
- [~] Selectable, copyable text (views/selectable_text.rs). Native DOM selection; "(edited)" is not selectable; the strip hides while text is selected
- [~] Attachments: images with lightbox, video, audio, files, downloads (views/scaled_image, app_window/downloads.rs). Audio is a file row with download, as in gpui. Video link previews play a muted `<video>` instead of gpui's decoded first frame
- [~] Upload progress for pending attachments
- [~] Link previews and GitHub PR previews (views/github_pr_preview.rs)
- [~] Reactions with tooltips and toggling; quick-react hover toolbar (views/quick_react_placement.rs). Hover, settle and clear delays live in the web view (`timeline/hover.ts`); tooltips use `ui/Tooltip`
- [~] Message hover toolbar: react, reply in thread, edit, delete, copy link. Edit goes to composer `edit_message`, reply to right_pane `open_thread`, the picker to overlays `open_reaction_picker`; copy uses pbcopy, clip, wl-copy or xclip
- [~] Reply context and thread summaries (views/timeline/reply_context.rs)
- [~] Failed sends: retry (composer `retry_send`) and discard
- [x] Typing indicator: drawn by the composer in its reserved row, as in gpui
- [~] Scrolling: pinned to bottom, older pages with anchor, unseen count, jump to latest. Bottom pinning seen live. The unseen count also updates after syncs, not only on scroll as in gpui
- [~] Jump to message (from search, pins, deep links) with highlight. Applies `pending_scroll_target` from syncs and ticks; `jump_to_message_active` clears once the web view confirms the jump (`scroll_applied`)
- [~] Mark read while focused and at the bottom. Core logic, driven by `reportView`

## Composer (`src/engine/composer.rs`, `ui/src/composer/`)

- [x] Multiline input with the gpui key bindings (views/input): auto-grow to 8 lines (thread 6), placeholder "Message #channel", Enter/Shift+Enter/Tab/Up/Down/Escape, typing slot under the field
- [~] Per-conversation and per-thread drafts: engine keeps them in `AppModels` and the store exactly as `sync_drafts_from_inputs`; conversation drafts verified live, thread drafts not yet exercised (right pane was in progress)
- [x] Send, Shift+Enter newline (verified sending to the self DM)
- [~] Edit last message (Up), edit mode, cancel edit: ported (`edit_last`, `edit_message`, `cancel_edit`, emptied edit deletes); live run of Up-to-edit was cut short by an app restart
- [~] Autocomplete: @mentions, :emoji: verified live (list, Up/Down, Tab/Return accept, click, Escape dismiss with gpui's dismiss signature). There is no #channel autocomplete in gpui (`autocomplete_detect` only knows @ and :). Member mentions depend on `conversation.details` being loaded.
- [x] Formatting shortcuts and menu: bold, italic, code (Cmd+B/I/E) and the "Aa" menu (all six formats)
- [~] Attachments: paste (image) and the upload lightbox with captions, Cancel/Next/Send/Send All, Return/Escape verified; picker (rfd open panel on the main thread) and drag and drop (Tauri drag-drop events, real paths) built but not exercised live
- [~] Thread reply composer: `<Composer target={{kind:"thread"}}/>` with its own field, autocomplete and focus; not exercised live
- [~] Composer disabled state for read-only or unjoined channels, join button: join bar and `join_current_channel` ported; not exercised live

## Right pane (`src/engine/right_pane.rs`, `ui/src/right_pane/`)

- [x] Pane host: Cmd+Shift+\\ / M / D / F toggles, header toggles, close button, Escape closes Thread/Profile/Details after overlays, edit and autocomplete (`dismiss_overlays_action`), "side panel is turned off" toast, overlay layout when the window is too narrow, resize drag on the left edge (`constrained_right_pane_width`)
- [x] Thread pane: root and replies (`build_thread_message_rows`) drawn with the timeline's `MessageRow`, "N replies" divider / "No replies yet" note, loading and unavailable states, reply composer, scroll to bottom on open and on new replies, toggle on re-open (views/right_pane)
- [~] Thread follow: `toggle_thread_following` intent is ported, but gpui has no control for it either; the "N new replies" pill is unreachable in gpui (new replies always scroll to the bottom), so it is not drawn
- [x] Members pane with filter: sections, owner-first sort, presence, identity markers, roles, no-match state with Clear filter, profile on click (views/right_pane/members.rs)
- [x] Details pane: identity card, badges, about, members and bots, shared files, leave channel with leaving / error states (views/right_pane/details.rs); verified rendering only, Leave was not clicked
- [~] Files pane: Media / Documents tabs per conversation, month groups, tile grid sized like gpui, document rows, show in conversation, load older, open (lightbox / player / timeline downloads). Missing: the per-file download control (timeline's `downloads::control` is not exported as a component yet). The asset scope now allows the Keybase service cache, where these thumbnails live; not yet rechecked live (views/right_pane/files.rs)
- [~] Search pane in the conversation: scoped hits, authors, loading and empty states, open hit (`shell::navigate_to_message`). Row is a local port of `search_result_row` (no mention painting in snippets); not exercised against a live search
- [x] Profile panel: avatar, presence, bio, location, title, proofs with service glyphs and states, teams, custom fields, followers/following tabs with loading, empty, failed and Retry, chunked long lists, follow/unfollow, Message, bottom fade, empty state (views/profile); Follow/Unfollow not clicked
- [x] Profile card popover (`render_profile_card` for `overlay.profile_card_user_id`, `show_profile_card` / dismiss); gpui never opens it except through the model
- [x] Window grows for the pane and shrinks back (views/pane_window.rs `PaneWindowSizing`, run from `right_pane::tick` with Tauri `set_size`; chat layout kept while growing, closed pane kept while shrinking). Not "pop-out windows": gpui has none

## Overlays (`src/engine/overlays.rs`, `ui/src/overlays/`)

- [x] Quick switcher (Cmd+K) with affinity ranking and message results: query observer with local fuzzy search (incremental cache, debounce), remote `QuickSwitcherSearch` with seq, Unread/Recent and Channels and people/Messages sections, highlighted labels and snippets, Up/Down/Tab/Shift+Tab/Return, Cmd+1..5 on recents, affinity recorded. Indexing status line ported but needs the core to call `overlays::observe_backend_event` (also needed for profile-name corpus rebuilds).
- [x] Command palette (Cmd+Shift+P), click to run like gpui; navigation, settings, mark-all-read and call go through the shell helpers.
- [x] New chat (Cmd+N): search, chips, highlight, Return adds, Cmd+Return starts, errors and states (not tested by creating a chat)
- [~] Emoji picker with skin tones and recents: catalog, search, bookmarks, tones, hover footer, recents. Reactions go through `ReactToMessage`. Composer insertion is published as `overlays_extra.emoji_insert` (`{seq, target, text}`); the composer must insert it at the caret and mark its container `data-composer="conversation"|"thread"`. Not tested live (no picker button wired yet).
- [x] Find in chat (Cmd+F) with match navigation: `FindBar` exported for the conversation view; Return/Shift+Return/Escape in the field, anchor, jump, close returns to latest
- [x] Global search (Cmd+J) with filters and snippets: `SearchView`, live query, Return/Up/Down, filters re-run, file rows, reply counts, link previews (video previews show the thumbnail, no decoded frame)
- [~] Image lightbox: rendered from `overlay.fullscreen_image`; opened through `overlays/open_image_lightbox` or the timeline's own intent. Not tested live.
- [~] Tooltips (views/tooltip.rs): shared `ui/Tooltip.tsx` (500ms delay, at the pointer, optional shortcut); not hover-tested live
- [~] Escape dismisses overlays in gpui's order: find field, overlays, cancel edit, close thread/profile/details pane. The update panel, autocomplete and format menu go first when they mark themselves `data-escape-layer` (composer autocomplete is detected from the models); the inspector and splash are the shell's.
