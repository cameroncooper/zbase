// The gpui app's icons, read from assets/icons/*.svg; they draw with currentColor.
// Shared: any area can use <Icon name="..." />.

import type { CSSProperties } from "react";

const FILES = import.meta.glob<string>("../../../assets/icons/*.svg", {
  query: "?raw",
  import: "default",
  eager: true,
});

const ICONS = Object.fromEntries(
  Object.entries(FILES).map(([path, svg]) => [path.split("/").pop()!.replace(/\.svg$/, ""), svg]),
) as Record<IconName, string>;


export type IconName =
  | "activity"
  | "alert-triangle"
  | "arrow-down-circle"
  | "arrow-left"
  | "arrow-right"
  | "at-sign"
  | "call"
  | "check"
  | "check-circle"
  | "chevron-down"
  | "chevron-left"
  | "chevron-right"
  | "chevron-up"
  | "chevrons-up-down"
  | "clipboard"
  | "clock"
  | "cloud-off"
  | "copy"
  | "crown"
  | "dm"
  | "download"
  | "download-spinner"
  | "emoji"
  | "eye"
  | "file-diff"
  | "files"
  | "format"
  | "git-merge"
  | "git-pull-request"
  | "git-pull-request-closed"
  | "git-pull-request-draft"
  | "hash"
  | "history"
  | "home"
  | "info"
  | "link"
  | "loader"
  | "members"
  | "mention"
  | "message-square"
  | "more-horizontal"
  | "paperclip"
  | "pencil"
  | "pin"
  | "play"
  | "plus"
  | "search"
  | "send"
  | "service-facebook"
  | "service-github"
  | "service-globe"
  | "service-hackernews"
  | "service-key"
  | "service-reddit"
  | "service-twitter"
  | "sliders"
  | "thread"
  | "trash"
  | "user-check"
  | "video"
  | "x";

/** `views::icon_asset`: a square icon tinted with `color` (CSS color, default currentColor). */
export function Icon({
  name,
  size,
  color,
  style,
  className,
}: {
  name: IconName;
  size: number;
  color?: string;
  style?: CSSProperties;
  className?: string;
}) {
  return (
    <span
      className={className}
      aria-hidden
      style={{
        display: "inline-flex",
        width: size,
        height: size,
        minWidth: size,
        flexShrink: 0,
        color,
        ...style,
      }}
      dangerouslySetInnerHTML={{
        __html: ICONS[name].replace("<svg ", `<svg width="${size}" height="${size}" `),
      }}
    />
  );
}
