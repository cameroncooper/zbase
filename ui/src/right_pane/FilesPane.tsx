// Files pane: media grid and document list by month (views/right_pane/files.rs).

import { assetUrl, send } from "../core/engine";
import { Tooltip } from "../ui/Tooltip";
import { EmptyState, Icon, PaneHeader, PaneWithHeader, SecondaryButton, SectionLabel } from "./parts";
import type { FileItem, FilesTab, RightPaneState } from "./types";

/** `format_duration_ms`. */
export function formatDuration(ms: number): string {
  const total = Math.floor(ms / 1000);
  const minutes = Math.floor(total / 60);
  const seconds = String(total % 60).padStart(2, "0");
  if (minutes >= 60) return `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}:${seconds}`;
  return `${minutes}:${seconds}`;
}

/** `attachment_size_text`. */
export function attachmentSizeText(bytes: number): string | null {
  if (bytes === 0) return null;
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit + 1 < units.length) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

const monthLabel = (timestamp: number | null) =>
  timestamp == null
    ? "Date unknown"
    : new Date(timestamp).toLocaleDateString("en-US", { month: "long", year: "numeric" });
const dayLabel = (timestamp: number | null) =>
  timestamp == null
    ? "Date unknown"
    : new Date(timestamp).toLocaleDateString("en-US", { month: "short", day: "numeric" });

/** `filename`: keep the extension when a long name is shortened. */
export function shortFilename(name: string): string {
  const value = name.trim() ? name : "Untitled file";
  const chars = [...value];
  if (chars.length <= 34) return value;
  const [stem, extension] = splitExtension(value);
  return `${[...stem].slice(0, 27).join("")}…${extension}`;
}

function splitExtension(name: string): [string, string] {
  const dot = name.lastIndexOf(".");
  if (dot < 0 || name.length - dot - 1 > 10) return [name, ""];
  return [name.slice(0, dot), name.slice(dot)];
}

export function FilesPane({ state, width }: { state: RightPaneState; width: number }) {
  const tab = state.files_tab;
  const media = tab === "Media";
  const items = state.files
    .filter((item) => item.is_media === media)
    .sort((a, b) => (b.timestamp_ms ?? -Infinity) - (a.timestamp_ms ?? -Infinity));
  const usable = Math.max(100, width - 4 - 32 - 8);
  const columns = Math.max(2, Math.floor(usable / 94));
  const tile = Math.floor((usable - (columns - 1) * 6) / columns);
  const groups: { month: string; items: FileItem[] }[] = [];
  for (const item of items) {
    const month = monthLabel(item.timestamp_ms);
    if (groups.at(-1)?.month !== month) groups.push({ month, items: [] });
    groups.at(-1)!.items.push(item);
  }
  const select = (next: FilesTab) => send("right_pane", "select_files_tab", { tab: next.toLowerCase() });

  return (
    <PaneWithHeader header={<PaneHeader title="Files" subtitle={state.location} />}>
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <div className="rp-files-tabs">
          {(["Media", "Documents"] as const).map((label) => (
            <div key={label} className={`rp-files-tab${label === tab ? " rp-selected" : ""}`} onClick={() => select(label)}>
              {label}
            </div>
          ))}
        </div>
        <div key={tab} className="rp-scroll" style={{ padding: "0 16px 16px", display: "flex", flexDirection: "column", gap: 16 }}>
          {items.length === 0 && (
            <EmptyState
              icon="files"
              title={media ? "No media yet" : "No documents yet"}
              body={
                media
                  ? "Images and videos shared in this conversation appear here."
                  : "Documents and audio shared in this conversation appear here."
              }
            />
          )}
          {groups.map((group) => (
            <div key={group.month} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <SectionLabel>{group.month}</SectionLabel>
              <div style={media ? { display: "flex", flexWrap: "wrap", gap: 6 } : { display: "flex", flexDirection: "column" }}>
                {group.items.map((item) => (
                  <FileEntry key={`${item.message_id}-${item.index}`} item={item} media={media} size={tile} />
                ))}
              </div>
            </div>
          ))}
          {state.files_has_older && (
            <>
              <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>Showing attachments from loaded messages.</div>
              <div>
                <SecondaryButton
                  onClick={state.files_loading_older ? undefined : () => send("right_pane", "load_older_files")}
                >
                  {state.files_loading_older ? "Loading older messages…" : "Load older messages"}
                </SecondaryButton>
              </div>
            </>
          )}
        </div>
      </div>
    </PaneWithHeader>
  );
}

function FileEntry({ item, media, size }: { item: FileItem; media: boolean; size: number }) {
  const { attachment } = item;
  const metadata = `${attachmentSizeText(attachment.size_bytes) ?? "Size unknown"} · ${item.author} · ${dayLabel(item.timestamp_ms)}`;
  const open = () => send("right_pane", "open_attachment", { message_id: item.message_id, index: item.index });
  const jump = (
    <Tooltip text="Show in conversation" singleLine>
      <div
        className="rp-file-jump"
        onClick={(event) => {
          event.stopPropagation();
          send("right_pane", "show_file_in_conversation", { message_id: item.message_id });
        }}
      >
        ↗
      </div>
    </Tooltip>
  );
  const tooltip = `${attachment.name}\n${metadata}`;
  if (media) {
    const thumbnail = assetUrl(item.thumbnail);
    return (
      <Tooltip text={tooltip} singleLine>
        <div className="rp-file-item" onClick={open} style={{ width: size, display: "flex", flexDirection: "column", gap: 4 }}>
          <div className="rp-file-tile" style={{ width: size, height: size }}>
            {thumbnail ? (
              <img src={thumbnail} alt="" draggable={false} onError={(event) => (event.currentTarget.style.visibility = "hidden")} />
            ) : (
              <Icon name="files" size={24} color="var(--text-secondary)" />
            )}
            {attachment.kind === "Video" && (
              <div className="rp-file-duration">▶ {attachment.duration_ms != null ? formatDuration(attachment.duration_ms) : ""}</div>
            )}
          </div>
          <div className="rp-file-jump-overlay">{jump}</div>
        </div>
      </Tooltip>
    );
  }
  const [stem, extension] = splitExtension(shortFilename(attachment.name));
  return (
    <Tooltip text={tooltip} singleLine>
      <div className="rp-file-item rp-file-doc" onClick={open}>
        <Icon name="files" size={24} color="var(--text-secondary)" />
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 4 }}>
          <div style={{ minWidth: 0, display: "flex", fontSize: 14 }}>
            <div className="rp-truncate">{stem}</div>
            <div style={{ flexShrink: 0 }}>{extension}</div>
          </div>
          <div className="rp-file-meta">{metadata}</div>
        </div>
        {jump}
      </div>
    </Tooltip>
  );
}
