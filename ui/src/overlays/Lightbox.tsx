// `OverlayHost::render_fullscreen_image_overlay`: the image lightbox. Clicking
// outside the image or caption dismisses it.

import { useEffect, useState } from "react";
import { assetUrl, send } from "../core/engine";
import type { FullscreenImageOverlay } from "./types";

/** `fullscreen_image_frame_size`: fit, never upscale; unknown sizes count as 1600×1000. */
function frameSize(width: number | null, height: number | null, maxWidth: number, maxHeight: number) {
  const w = width && width > 0 ? width : 1600;
  const h = height && height > 0 ? height : 1000;
  const scale = Math.min(maxWidth / w, maxHeight / h, 1);
  return [w * scale, h * scale] as const;
}

function useViewport() {
  const [size, setSize] = useState({ width: window.innerWidth, height: window.innerHeight });
  useEffect(() => {
    const onResize = () => setSize({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return size;
}

const dismiss = (event: React.MouseEvent) => {
  event.stopPropagation();
  send("overlays", "dismiss_overlays");
};
const keep = (event: React.MouseEvent) => event.stopPropagation();

export function Lightbox({ image }: { image: FullscreenImageOverlay }) {
  const viewport = useViewport();
  const [failed, setFailed] = useState(false);
  const caption = image.caption?.trim() || null;
  const source = "Url" in image.source ? image.source.Url : image.source.LocalPath;
  const url = assetUrl(source);
  useEffect(() => setFailed(false), [url]);
  if (!url) {
    return (
      <div className="lb-root" onMouseDown={dismiss}>
        <div
          onMouseDown={keep}
          style={{
            padding: "12px 16px",
            borderRadius: 8,
            background: "var(--panel-bg)",
            fontSize: 14,
            color: "var(--text-primary)",
          }}
        >
          Unable to load image preview.
        </div>
      </div>
    );
  }
  const [width, height] = frameSize(
    image.width,
    image.height,
    Math.min(1120, Math.max(1, viewport.width - 48)),
    Math.min(780, Math.max(1, viewport.height - (caption ? 120 : 48))),
  );
  return (
    <div className="lb-root" onMouseDown={dismiss}>
      <div className="lb-column">
        <div className="lb-image-row">
          <div onMouseDown={keep} style={{ width, height, flexShrink: 0, display: "flex" }}>
            {failed ? (
              <div className="lb-message" style={{ margin: "auto" }}>
                Image unavailable
              </div>
            ) : (
              <img
                src={url}
                alt=""
                draggable={false}
                onError={() => setFailed(true)}
                style={{ width: "100%", height: "100%", objectFit: "contain" }}
              />
            )}
          </div>
        </div>
        {caption && (
          <div className="lb-caption" onMouseDown={keep}>
            {caption}
          </div>
        )}
      </div>
    </div>
  );
}
