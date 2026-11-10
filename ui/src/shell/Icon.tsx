// The assets/icons SVGs the shell draws (the web view can't read ../assets),
// inlined so they take `currentColor` like gpui's tinted `svg()`.

import type { CSSProperties } from "react";

const ICONS = {
  "chevron-left": { width: 2.1, body: <path d="m15 6-6 6 6 6" /> },
  "chevron-right": { width: 2.1, body: <path d="m9 6 6 6-6 6" /> },
  "chevron-down": { width: 2.1, body: <path d="m6 9 6 6 6-6" /> },
  "chevrons-up-down": {
    width: 2.1,
    body: (
      <>
        <path d="m7 15 5 5 5-5" />
        <path d="m7 9 5-5 5 5" />
      </>
    ),
  },
  search: {
    width: 1.9,
    body: (
      <>
        <circle cx="11" cy="11" r="5.5" />
        <path d="m16 16 4 4" />
      </>
    ),
  },
  plus: {
    width: 1.9,
    body: (
      <>
        <path d="M12 5v14" />
        <path d="M5 12h14" />
      </>
    ),
  },
  "arrow-down-circle": {
    width: 2,
    body: (
      <>
        <circle cx="12" cy="12" r="10" />
        <path d="m8 12 4 4 4-4M12 8v8" />
      </>
    ),
  },
  "cloud-off": {
    width: 2,
    body: (
      <>
        <path d="M2 2l20 20" />
        <path d="M5.8 5.8A7 7 0 0 0 9 19h8.5a4.5 4.5 0 0 0 1.3-.2" />
        <path d="M21.5 15.9A4.5 4.5 0 0 0 17.5 10h-1.8A7 7 0 0 0 10 5.1" />
      </>
    ),
  },
  hash: {
    width: 1.9,
    body: (
      <>
        <path d="M5 9h14" />
        <path d="M4 15h14" />
        <path d="M10 4 8 20" />
        <path d="m16 4-2 16" />
      </>
    ),
  },
  "alert-triangle": {
    width: 2,
    body: (
      <>
        <path d="M10.3 4.2L2.6 17.5A2 2 0 0 0 4.3 20.5h15.4a2 2 0 0 0 1.7-3L13.7 4.2a2 2 0 0 0-3.4 0z" />
        <path d="M12 9.5v4.5" />
        <path d="M12 17.2h.01" />
      </>
    ),
  },
  check: { width: 2.4, body: <path d="M20 6 9 17l-5-5" /> },
  x: {
    width: 2,
    body: (
      <>
        <path d="M18 6 6 18" />
        <path d="m6 6 12 12" />
      </>
    ),
  },
} as const;

export type IconName = keyof typeof ICONS;

export function Icon({ name, size, color, style }: { name: IconName; size: number; color?: string; style?: CSSProperties }) {
  const icon = ICONS[name];
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={icon.width}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ flexShrink: 0, color, ...style }}
      aria-hidden
    >
      {icon.body}
    </svg>
  );
}
