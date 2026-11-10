// App-wide keyboard shortcuts, the counterpart of src/app/key_bindings.rs.
// Areas register their own bindings; keys use gpui's notation ("cmd-k",
// "alt-down", "cmd-shift-p"). "cmd" means Ctrl off macOS, as in shortcuts.rs.

import { useEffect, useRef } from "react";

export const IS_MAC = navigator.userAgent.includes("Mac");

type Binding = {
  keys: string;
  /** Return false to let the event continue to lower-priority bindings. */
  handler: (event: KeyboardEvent) => boolean | void;
  /** Only fires while this returns true; mirrors gpui key contexts. */
  when?: () => boolean;
  /**
   * The element this binding belongs to. While focus is inside it, the binding
   * is tried before every unscoped one, like gpui's focused key context.
   */
  within?: () => Element | null;
};

const bindings: Binding[] = [];

/** "cmd-shift-k" for an event, with the modifiers in a fixed order. */
export function eventKeys(event: KeyboardEvent): string {
  const parts: string[] = [];
  if (event.ctrlKey && IS_MAC) parts.push("ctrl");
  if (event.altKey) parts.push("alt");
  if (event.shiftKey) parts.push("shift");
  if (IS_MAC ? event.metaKey : event.ctrlKey) parts.push("cmd");
  let key = event.key;
  // Option changes the character on macOS; use the physical key instead.
  if (event.altKey && /^Key[A-Z]$/.test(event.code)) key = event.code.slice(3);
  if (event.altKey && /^Digit\d$/.test(event.code)) key = event.code.slice(5);
  const named: Record<string, string> = {
    ArrowUp: "up",
    ArrowDown: "down",
    ArrowLeft: "left",
    ArrowRight: "right",
    Escape: "escape",
    Enter: "enter",
    Tab: "tab",
    Backspace: "backspace",
    Delete: "delete",
    " ": "space",
  };
  parts.push(named[key] ?? key.toLowerCase());
  return parts.join("-");
}

/** Accepts gpui order too ("alt-cmd-1" and "cmd-alt-1" both work). */
function normalize(keys: string): string {
  const parts = keys.toLowerCase().split("-");
  const key = parts.pop()!;
  const order = ["ctrl", "alt", "shift", "cmd"];
  return [...order.filter((modifier) => parts.includes(modifier)), key].join("-");
}

window.addEventListener(
  "keydown",
  (event) => {
    if (event.isComposing) return;
    const keys = eventKeys(event);
    const focused = document.activeElement;
    const inFocus = (binding: Binding) => {
      const scope = binding.within?.();
      return !!scope && !!focused && scope.contains(focused);
    };
    // Bindings scoped to the focused element first, then later registrations.
    const ordered = [...bindings.filter(inFocus).reverse(), ...bindings.filter((b) => !inFocus(b)).reverse()];
    for (const binding of ordered) {
      if (normalize(binding.keys) !== keys) continue;
      if (binding.when && !binding.when()) continue;
      if (binding.handler(event) === false) continue;
      event.preventDefault();
      event.stopPropagation();
      return;
    }
  },
  true,
);

/** Registers bindings for as long as the calling component is mounted. */
export function useShortcuts(list: Binding[]) {
  const latest = useRef(list);
  latest.current = list;
  useEffect(() => {
    const registered = latest.current.map((binding, index) => ({
      keys: binding.keys,
      handler: (event: KeyboardEvent) => latest.current[index]?.handler(event),
      when: () => latest.current[index]?.when?.() ?? true,
      within: () => latest.current[index]?.within?.() ?? null,
    }));
    bindings.push(...registered);
    return () => {
      for (const binding of registered) bindings.splice(bindings.indexOf(binding), 1);
    };
  }, [list.length]);
}

/** "⌘K" style label for hints, like `shortcuts::command_label`. */
export function shortcutLabel(keys: string): string {
  return keys
    .split("-")
    .map((part) => {
      switch (part) {
        case "cmd":
          return IS_MAC ? "⌘" : "Ctrl+";
        case "shift":
          return IS_MAC ? "⇧" : "Shift+";
        case "alt":
          return IS_MAC ? "⌥" : "Alt+";
        case "ctrl":
          return IS_MAC ? "⌃" : "Ctrl+";
        case "enter":
          return "↩";
        case "up":
          return "↑";
        case "down":
          return "↓";
        default:
          return part.length === 1 ? part.toUpperCase() : part;
      }
    })
    .join("");
}
