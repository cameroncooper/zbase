// `OverlayHost::render_command_palette`: Cmd+Shift+P, click to run.

import { send } from "../core/engine";
import { shortcutGlyphs } from "../ui/Tooltip";

const COMMANDS: { label: string; shortcut: string; action: string }[] = [
  { label: "Start new chat", shortcut: "⌘N", action: "new-chat" },
  { label: "Search messages", shortcut: "⌘J", action: "search" },
  { label: "Toggle thread pane", shortcut: "⌘⇧\\", action: "thread" },
  { label: "Open settings", shortcut: "⌘,", action: "preferences" },
  { label: "Show activity", shortcut: "⌘⇧A", action: "activity" },
  { label: "Mark all as read", shortcut: "", action: "mark-all-read" },
];

/** `run_palette_action`. */
export function runPaletteAction(action: string) {
  send("overlays", "run_palette_action", { action });
}

export function CommandPalette() {
  return (
    <div className="ov-dialog cp-dialog" onClick={(event) => event.stopPropagation()}>
      <div className="cp-title">Commands</div>
      <div className="cp-rows">
        {COMMANDS.map((command) => (
          <div key={command.action} className="cp-row" onClick={() => runPaletteAction(command.action)}>
            {command.label}
            <span className="cp-shortcut">{shortcutGlyphs(command.shortcut)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
