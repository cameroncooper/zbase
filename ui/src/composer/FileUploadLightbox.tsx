// The file upload lightbox (views/overlays `render_file_upload_lightbox`):
// a preview of each picked, pasted or dropped file with a caption, then
// Cancel / Next / Send / Send All.

import { useEffect, useLayoutEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { assetUrl, readModel, send, useModel } from "../core/engine";
import { useShortcuts } from "../core/shortcuts";
import type { AttachmentKind, FileUploadLightboxModel } from "./types";

/** `OVERLAY_TOP_PX` in views/overlays. */
const OVERLAY_TOP_PX = 96;
let handledCaptionFocusSeq = 0;

function attachmentKindTag(kind: AttachmentKind): string {
  return { Image: "IMG", Video: "VID", Audio: "AUD", File: "FILE" }[kind];
}

/** views/mod.rs `attachment_size_text`. */
function attachmentSizeText(sizeBytes: number): string | null {
  if (sizeBytes === 0) return null;
  if (sizeBytes < 1024) return `${sizeBytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = sizeBytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit + 1 < units.length) {
    value /= 1024;
    unit += 1;
  }
  return value >= 10 ? `${value.toFixed(0)} ${units[unit]}` : `${value.toFixed(1)} ${units[unit]}`;
}

/** views/mod.rs `attachment_display_label`. */
function attachmentDisplayLabel(name: string, kind: AttachmentKind, sizeBytes: number): string {
  const prefix = attachmentKindTag(kind);
  const size = attachmentSizeText(sizeBytes);
  return size ? `[${prefix}] ${name} (${size})` : `[${prefix}] ${name}`;
}

/** views/overlays `file_upload_preview_frame_size`. */
function previewFrameSize(
  width: number | null,
  height: number | null,
  maxWidth: number,
  maxHeight: number,
): [number, number] {
  let w = width ?? 900;
  let h = height ?? 600;
  if (w <= 1 || h <= 1) return [Math.min(maxWidth, 420), Math.min(maxHeight, 240)];
  const scale = Math.min(maxWidth / w, maxHeight / h, 1);
  w *= scale;
  h *= scale;
  return [Math.max(w, 60), Math.max(h, 60)];
}

function sendLightbox(total: number) {
  send("composer", total > 1 ? "upload_send_all" : "upload_send");
}

export function FileUploadLightbox({ lightbox }: { lightbox: FileUploadLightboxModel }) {
  const extra = useModel("composer_extra");
  const captionRef = useRef<HTMLTextAreaElement>(null);
  const total = lightbox.candidates.length;
  const index = Math.min(lightbox.current_index, Math.max(total - 1, 0));
  const candidate = lightbox.candidates[index];

  // A fresh caption field per file, seeded from the model.
  useLayoutEffect(() => {
    if (captionRef.current) captionRef.current.value = candidate?.caption ?? "";
  }, [index, candidate?.path]);

  const focusRequest = extra?.focus_request;
  useEffect(() => {
    if (!focusRequest || focusRequest.target !== null || focusRequest.seq <= handledCaptionFocusSeq) return;
    handledCaptionFocusSeq = focusRequest.seq;
    captionRef.current?.focus();
  }, [focusRequest?.seq]);

  const open = () => Boolean((readModel("overlay") as { file_upload_lightbox?: unknown } | undefined)?.file_upload_lightbox);
  useShortcuts([
    {
      // ConfirmPrimary with the lightbox open; Shift+Return stays a newline in the caption.
      keys: "enter",
      when: open,
      handler: () => sendLightbox(total),
    },
    {
      keys: "escape",
      when: open,
      handler: () => {
        send("composer", "upload_cancel");
      },
    },
  ]);

  const isLast = index + 1 >= total;

  let body;
  if (!candidate) {
    body = null;
  } else {
    const label = attachmentDisplayLabel(candidate.filename, candidate.kind, candidate.size_bytes);
    body = (
      <>
        <div className="zc-lightbox-header">Upload files</div>
        <div className="zc-lightbox-body">
          <div className="zc-lightbox-preview">
            <Preview candidate={candidate} />
          </div>
          <div className="zc-lightbox-filename">{candidate.filename}</div>
          <div className="zc-lightbox-meta">{`${index + 1} of ${total} · ${label}`}</div>
          <div className="zc-lightbox-caption" onMouseDown={() => captionRef.current?.focus()}>
            <textarea
              ref={captionRef}
              rows={2}
              placeholder="Add a caption..."
              onInput={(event) => send("composer", "upload_caption", { caption: event.currentTarget.value })}
            />
          </div>
        </div>
        <div className="zc-lightbox-footer">
          <button className="zc-badge zc-badge-plain" onClick={() => send("composer", "upload_cancel")}>
            Cancel
          </button>
          {!isLast && (
            <button className="zc-badge zc-badge-plain" onClick={() => send("composer", "upload_next")}>
              Next
            </button>
          )}
          {(isLast || total === 1) && (
            <button className="zc-badge zc-badge-accent" onClick={() => sendLightbox(total)}>
              Send
            </button>
          )}
          {total > 1 && (
            <button className="zc-badge zc-badge-plain" onClick={() => send("composer", "upload_send_all")}>
              Send All
            </button>
          )}
        </div>
      </>
    );
  }

  return createPortal(
    <div className="zc-lightbox-backdrop" onClick={() => send("composer", "upload_cancel")}>
      <div
        className={candidate ? "zc-lightbox" : "zc-lightbox zc-lightbox-empty"}
        style={{ marginTop: OVERLAY_TOP_PX }}
        onClick={(event) => event.stopPropagation()}
      >
        {body ?? "No files selected."}
      </div>
    </div>,
    document.body,
  );
}

function Preview({ candidate }: { candidate: FileUploadLightboxModel["candidates"][number] }) {
  switch (candidate.kind) {
    case "Image": {
      const [width, height] = previewFrameSize(candidate.width, candidate.height, 480, 280);
      return <ImagePreview src={assetUrl(candidate.path)} width={width} height={height} />;
    }
    case "Video":
      return <div className="zc-lightbox-note">Video preview coming soon</div>;
    case "Audio":
      return <div className="zc-lightbox-note">Audio file selected</div>;
    default:
      return <div className="zc-lightbox-note">File selected</div>;
  }
}

function ImagePreview({ src, width, height }: { src: string | undefined; width: number; height: number }) {
  const ref = useRef<HTMLImageElement>(null);
  const failedRef = useRef<HTMLDivElement>(null);
  if (!src) return <div className="zc-lightbox-note">Image unavailable</div>;
  return (
    <div className="zc-lightbox-note">
      <img
        ref={ref}
        src={src}
        style={{ width, height, objectFit: "contain" }}
        onError={() => {
          if (ref.current) ref.current.style.display = "none";
          if (failedRef.current) failedRef.current.style.display = "block";
        }}
      />
      <div ref={failedRef} style={{ display: "none" }}>
        Image unavailable
      </div>
    </div>
  );
}
