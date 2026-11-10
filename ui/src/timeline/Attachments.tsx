// Attachments under a message (views/timeline render_message, views/mod.rs
// attachment rows, app_window/downloads.rs) and link previews with GitHub PR
// cards (views/github_pr_preview.rs).

import { useState } from "react";
import { assetUrl, send, useModel } from "../core/engine";
import { Icon, type IconName } from "../ui/Icon";
import { Tooltip } from "../ui/Tooltip";
import { openLink } from "./RichText";
import type { AttachmentView, GithubPrPreview, GithubPrStatus, LinkPreview } from "./types";

/** `TimelineList::media_frame_size`. */
export function mediaFrameSize(
  width: number | null,
  height: number | null,
  maxWidth: number,
  maxHeight: number,
): [number, number] {
  let w = width ?? 320;
  let h = height ?? 200;
  if (w <= 1 || h <= 1) return [Math.min(maxWidth, 320), Math.min(maxHeight, 200)];
  const scale = Math.min(maxWidth / w, maxHeight / h, 1);
  w *= scale;
  h *= scale;
  const minWidth = Math.min(Math.max(maxWidth, 1), 120);
  const minHeight = Math.min(Math.max(maxHeight, 1), 90);
  return [Math.max(w, minWidth), Math.max(h, minHeight)];
}

function Img({
  src,
  fallback,
  ...rest
}: { src: string | null | undefined; fallback: string } & React.ImgHTMLAttributes<HTMLImageElement>) {
  const [failed, setFailed] = useState(false);
  if (failed || !src) return <div className="tl-fallback">{fallback}</div>;
  return <img src={assetUrl(src)} draggable={false} onError={() => setFailed(true)} {...rest} />;
}

function AttachmentHeader({ attachment }: { attachment: AttachmentView }) {
  return (
    <div className="tl-attachment-header">
      <Icon name="paperclip" size={14} />
      <span className="name">{attachment.name}</span>
      {attachment.size_label && <span style={{ flexShrink: 0 }}>{attachment.size_label}</span>}
    </div>
  );
}

function FileRow({ attachment }: { attachment: AttachmentView }) {
  return (
    <div className="tl-file-row">
      <Icon name="files" size={16} />
      <span className="name">{attachment.name}</span>
      {attachment.size_label && <span className="size">{attachment.size_label}</span>}
    </div>
  );
}

function UploadRow({ attachment }: { attachment: AttachmentView }) {
  const upload = attachment.upload!;
  return (
    <div className="tl-upload-row">
      <Icon name="files" size={16} />
      <span className="name">{attachment.name}</span>
      <span className="status">{upload.label}</span>
      <div className="tl-upload-track">
        <div style={{ width: `${upload.fraction * 100}%` }} />
      </div>
    </div>
  );
}

const downloadKey = (source: string, name: string) => `${source}\n${name}`;

/** `downloads::control`: download, spinner, Open, Show or Retry. */
function DownloadControl({ attachment }: { attachment: AttachmentView }) {
  const extra = useModel("timeline_extra");
  const source = attachment.download_source;
  const state = source ? extra?.downloads[downloadKey(source, attachment.name)] : undefined;
  let label = "";
  let tooltip = "Download file to Downloads";
  switch (state?.state) {
    case "downloading":
      tooltip = "Downloading…";
      break;
    case "saved":
      label = "Open";
      tooltip = "Open downloaded file";
      break;
    case "reveal":
      label = "Show";
      tooltip = "Show in Finder — no application could open this file";
      break;
    case "failed":
      label = "Retry";
      tooltip = `Download failed: ${state.error}`;
      break;
  }
  if (!source) tooltip = "File is not available yet";
  return (
    <Tooltip text={tooltip} singleLine>
      <div
        className={`tl-download-control${source ? "" : " unavailable"}`}
        onClick={(event) => {
          event.stopPropagation();
          activateDownload(attachment);
        }}
      >
        {state?.state === "downloading" ? (
          <Icon name="download-spinner" size={14} className="tl-spin" />
        ) : !state ? (
          <Icon name="download" size={16} />
        ) : (
          label
        )}
      </div>
    </Tooltip>
  );
}

function activateDownload(attachment: AttachmentView) {
  if (attachment.download_source) {
    send("timeline", "download", { source: attachment.download_source, name: attachment.name });
  }
}

/** `downloads::row`: the header (or the whole file row) with the control at its end. */
function DownloadRow({ attachment, header }: { attachment: AttachmentView; header: React.ReactNode }) {
  const generic = attachment.kind === "File";
  return (
    <div
      className={`tl-download-row${generic ? " generic" : ""}`}
      onClick={generic ? () => activateDownload(attachment) : undefined}
    >
      <div className="grow">{generic ? <AttachmentHeader attachment={attachment} /> : header}</div>
      <DownloadControl attachment={attachment} />
    </div>
  );
}

type MediaProps = {
  messageId: string;
  thread: boolean;
  attachments: AttachmentView[];
  caption: string | null;
  /** The thread pane's column width, or the main timeline's limits. */
  mediaMaxWidth?: number;
  roomy: boolean;
  onHover: (event: React.PointerEvent) => void;
};

export function Attachments({ messageId, thread, attachments, caption, mediaMaxWidth, roomy, onHover }: MediaProps) {
  const [maxWidth, maxHeight] =
    mediaMaxWidth !== undefined ? [mediaMaxWidth, 220] : roomy ? [360, 300] : [280, 240];
  return (
    <div className="tl-attachments">
      {attachments.map((attachment, index) => {
        const inlinePreview = (attachment.kind === "Image" || attachment.kind === "Video") && attachment.image;
        const base = attachment.upload ? (
          <UploadRow attachment={attachment} />
        ) : inlinePreview ? (
          <AttachmentHeader attachment={attachment} />
        ) : (
          <FileRow attachment={attachment} />
        );
        const header = attachment.upload ? base : <DownloadRow attachment={attachment} header={base} />;
        const [width, height] = mediaFrameSize(attachment.preview_width, attachment.preview_height, maxWidth, maxHeight);
        return (
          <div key={index} className="tl-attachment">
            {header}
            {attachment.kind === "Image" && attachment.image && (
              <>
                <Img
                  className={`tl-media${attachment.lightbox ? " clickable" : ""}`}
                  src={attachment.image}
                  fallback={attachment.display_label}
                  style={{ width, height }}
                  onPointerMove={attachment.lightbox ? onHover : undefined}
                  onClick={
                    attachment.lightbox
                      ? () => send("timeline", "open_image", { message_id: messageId, index, thread })
                      : undefined
                  }
                />
                {caption && <div className="tl-caption">{caption}</div>}
              </>
            )}
            {attachment.kind === "Video" && attachment.image && (
              <div
                className="tl-video"
                style={{ width, height }}
                onPointerMove={onHover}
                onClick={
                  attachment.video_path
                    ? () => send("timeline", "open_video", { path: attachment.video_path })
                    : undefined
                }
              >
                <Img src={attachment.image} fallback="" alt="" />
                <div className="tl-video-play">
                  <div>
                    <Icon name="play" size={24} />
                  </div>
                </div>
                {attachment.duration_label && <div className="tl-video-duration">{attachment.duration_label}</div>}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

// Link previews (`render_link_previews`).

export function LinkPreviews({
  previews,
  hidden,
  tightBottom,
  mediaMaxWidth,
  onHover,
}: {
  previews: LinkPreview[];
  hidden: number;
  tightBottom: boolean;
  mediaMaxWidth?: number;
  onHover: (event: React.PointerEvent) => void;
}) {
  const maxWidth = mediaMaxWidth ?? 360;
  const maxHeight = mediaMaxWidth !== undefined ? 220 : 300;
  return (
    <div className={`tl-previews${tightBottom ? "" : " loose"}`}>
      {previews.slice(0, 3).map((preview, index) => (
        <Preview key={index} preview={preview} maxWidth={maxWidth} maxHeight={maxHeight} onHover={onHover} />
      ))}
      {hidden > 0 && <div className="tl-more-links">+{hidden} more links</div>}
    </div>
  );
}

function Preview({
  preview,
  maxWidth,
  maxHeight,
  onHover,
}: {
  preview: LinkPreview;
  maxWidth: number;
  maxHeight: number;
  onHover: (event: React.PointerEvent) => void;
}) {
  const open = () => openLink(preview.url);
  if (preview.github_pr) {
    return (
      <div onClick={open} onPointerMove={onHover}>
        <GithubPrCard preview={preview.github_pr} />
      </div>
    );
  }
  const lower = preview.url.toLowerCase();
  const giphy = lower.includes("giphy.com") || lower.includes("gph.is");
  if (preview.is_media) {
    const [width, height] = mediaFrameSize(preview.media_width, preview.media_height, maxWidth, maxHeight);
    const source = preview.thumbnail_asset ?? preview.url;
    return (
      <div className="tl-media-preview" style={{ width, height }} onClick={open} onPointerMove={onHover}>
        {preview.is_video && !preview.thumbnail_asset ? (
          <video src={assetUrl(preview.video_url ?? preview.url)} muted autoPlay loop playsInline preload="metadata" />
        ) : (
          <Img src={source} fallback={preview.site ?? "media"} />
        )}
        {preview.is_video && preview.thumbnail_asset && <div className="tl-video-tag">video</div>}
      </div>
    );
  }
  const title = giphy
    ? preview.title?.trim() || "GIPHY"
    : (preview.title ?? preview.url);
  const thumbMaxHeight = Math.min(maxHeight, 200);
  let thumb: [number, number] = [maxWidth, thumbMaxHeight];
  if (preview.media_width && preview.media_height && preview.media_width > 0 && preview.media_height > 0) {
    const scale = Math.min(maxWidth / preview.media_width, thumbMaxHeight / preview.media_height, 1);
    thumb = [preview.media_width * scale, preview.media_height * scale];
  }
  return (
    <div className="tl-link-preview" onClick={open} onPointerMove={onHover}>
      <div className="site">{preview.site ?? "link"}</div>
      <div className="title">{title}</div>
      {preview.title !== null && !giphy && <div className="url">{preview.url}</div>}
      {preview.thumbnail_asset && (
        <Img src={preview.thumbnail_asset} fallback="" style={{ width: thumb[0], height: thumb[1] }} />
      )}
    </div>
  );
}

type Tone = "good" | "bad" | "waiting" | "neutral" | "merged";

const TONES: Record<Tone, [string, string]> = {
  good: ["var(--affinity-positive)", "var(--success-soft)"],
  bad: ["var(--affinity-broken)", "var(--danger-soft)"],
  waiting: ["var(--warning)", "var(--warning-soft)"],
  neutral: ["var(--text-secondary)", "var(--panel-alt-bg)"],
  merged: ["var(--tl-merged)", "var(--tl-merged-soft)"],
};

function Chip({ icon, label, tone }: { icon: IconName; label: string; tone: Tone }) {
  const [fg, bg] = TONES[tone];
  return (
    <div className="tl-pr-chip" style={{ color: fg, background: bg }}>
      <Icon name={icon} size={11} />
      {label}
    </div>
  );
}

const plural = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n} ${many}`);

function statusChips(status: GithubPrStatus, refreshing: boolean) {
  const chips: React.ReactNode[] = [];
  if (status.state === "Merged") {
    chips.push(
      <Chip key="m" icon="git-merge" label={status.merged_by ? `Merged by ${status.merged_by}` : "Merged"} tone="merged" />,
    );
  } else if (status.state === "Closed") {
    chips.push(<Chip key="c" icon="git-pull-request-closed" label="Closed without merging" tone="neutral" />);
  } else {
    const { passed, failed, running } = status.checks;
    const total = passed + failed + running;
    if (total > 0) {
      chips.push(
        failed > 0 ? (
          <Chip key="k" icon="x" label={`${failed} of ${total} checks failed`} tone="bad" />
        ) : running > 0 ? (
          <Chip
            key="k"
            icon="loader"
            label={passed > 0 ? `${running} running · ${passed} passed` : plural(running, "check running", "checks running")}
            tone="waiting"
          />
        ) : (
          <Chip key="k" icon="check" label={plural(passed, "check passed", "checks passed")} tone="good" />
        ),
      );
    }
    const review: Record<string, [IconName, string, Tone]> = {
      Approved: ["check", "Approved", "good"],
      ChangesRequested: ["file-diff", "Changes requested", "bad"],
      ReviewRequired: ["eye", "Review required", "neutral"],
      NoReviews: ["eye", "No reviews", "neutral"],
    };
    const [icon, label, tone] = review[status.review];
    chips.push(<Chip key="r" icon={icon} label={label} tone={tone} />);
    const more = status.threads_complete ? "" : "+";
    chips.push(
      status.unresolved_threads > 0 ? (
        <Chip key="t" icon="message-square" label={`${status.unresolved_threads}${more} unresolved`} tone="waiting" />
      ) : !status.threads_complete ? (
        <Chip key="t" icon="message-square" label={`${status.total_threads} threads`} tone="neutral" />
      ) : status.total_threads > 0 ? (
        <Chip key="t" icon="message-square" label="All resolved" tone="neutral" />
      ) : (
        <Chip key="t" icon="message-square" label="No comments" tone="neutral" />
      ),
    );
  }
  chips.push(
    <div key="d" className="tl-pr-diff">
      <span style={{ color: "var(--affinity-positive)" }}>+{status.additions}</span>
      <span style={{ color: "var(--affinity-broken)" }}>−{status.deletions}</span>
    </div>,
  );
  if (refreshing) {
    chips.push(
      <div key="u" style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12, color: "var(--text-secondary)" }}>
        <Icon name="loader" size={11} />
        updating…
      </div>,
    );
  }
  return chips;
}

/** `render_github_pr_preview`. */
export function GithubPrCard({ preview }: { preview: GithubPrPreview }) {
  const status = preview.status;
  const state = status?.state;
  const [bar, icon, iconColor]: [string, IconName, string] =
    state === undefined
      ? preview.needs_auth
        ? ["var(--text-secondary)", "git-pull-request", "var(--text-secondary)"]
        : ["var(--accent)", "git-pull-request", "var(--text-secondary)"]
      : state === "Open"
        ? ["var(--success)", "git-pull-request", "var(--success)"]
        : state === "Draft"
          ? ["var(--text-secondary)", "git-pull-request-draft", "var(--text-secondary)"]
          : state === "Merged"
            ? ["var(--tl-merged)", "git-merge", "var(--tl-merged)"]
            : ["var(--danger)", "git-pull-request-closed", "var(--danger)"];
  const pill: [string, Tone] | null =
    state === "Draft" ? ["Draft", "neutral"] : state === "Merged" ? ["Merged", "merged"] : state === "Closed" ? ["Closed", "bad"] : null;
  const repoLabel = `${preview.pr.owner}/${preview.pr.repo}`;
  return (
    <div className="tl-pr" style={{ borderLeftColor: bar }}>
      <div className="tl-pr-site">
        <Icon name={icon} size={12} color={iconColor} />
        {repoLabel} #{preview.pr.number}
        {pill && (
          <span className="tl-pr-pill" style={{ color: TONES[pill[1]][0], background: TONES[pill[1]][1] }}>
            {pill[0]}
          </span>
        )}
      </div>
      <div className="tl-pr-title">
        {status ? (
          status.title
        ) : preview.needs_auth ? (
          "Pull request"
        ) : (
          <div className="tl-skeleton" style={{ width: 240, height: 12, margin: "2px 0" }} />
        )}
      </div>
      <div className="tl-pr-chips">
        {status ? (
          statusChips(status, preview.refreshing)
        ) : preview.needs_auth ? (
          <div style={{ fontSize: 12, color: "var(--link)" }}>Run `gh auth login` to see checks and reviews</div>
        ) : (
          <>
            <div className="tl-skeleton" style={{ width: 110, height: 20 }} />
            <div className="tl-skeleton" style={{ width: 80, height: 20 }} />
            <div className="tl-skeleton" style={{ width: 90, height: 20 }} />
          </>
        )}
      </div>
    </div>
  );
}
