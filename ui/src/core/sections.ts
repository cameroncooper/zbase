// Types of the model sections the engine publishes, keyed by section name.
// Each area keeps the types it renders in its own `types.ts` and registers
// them here. Field names are the Rust ones (snake_case); Rust enums use serde's
// default external tagging: unit variants are strings, others `{ Variant: ... }`.

import type * as Shell from "../shell/types";
import type * as Timeline from "../timeline/types";
import type * as Composer from "../composer/types";
import type * as RightPane from "../right_pane/types";
import type * as Overlays from "../overlays/types";

export type Sections = {
  // Shell
  app: Shell.AppModel;
  workspace: Shell.WorkspaceModel;
  navigation: Shell.NavigationModel;
  sidebar: Shell.SidebarModel;
  notifications: Shell.NotificationsModel;
  settings: Shell.SettingsModel;
  call: Shell.CallModel;
  shell: Shell.ShellState;
  sidebar_avatars: Shell.SidebarAvatars;
  // Timeline
  conversation: Timeline.ConversationModel;
  timeline: Timeline.TimelineModel;
  timeline_extra: Timeline.TimelineState;
  // Composer
  composer: Composer.ComposerModel;
  composer_extra: Composer.ComposerState;
  // Right pane
  thread_pane: RightPane.ThreadPaneModel;
  profile_panel: RightPane.ProfilePanelModel;
  right_pane_extra: RightPane.RightPaneState;
  // Overlays
  quick_switcher: Overlays.QuickSwitcherModel;
  emoji_picker: Overlays.EmojiPickerModel;
  find_in_chat: Overlays.FindInChatModel;
  search: Overlays.SearchModel;
  new_chat: Overlays.NewChatModel;
  overlay: Overlays.OverlayModel;
  overlays_extra: Overlays.OverlaysState;
};
