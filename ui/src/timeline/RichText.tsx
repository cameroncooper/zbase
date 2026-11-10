// Styled message text: the web counterpart of `SelectableText`'s runs
// (views/selectable_text.rs). The engine has already applied markdown, emoji,
// mentions, find highlights and code highlighting (engine/timeline/present.rs).

import { useState, type CSSProperties } from "react";
import { assetUrl, send } from "../core/engine";
import type { Block, Role, Run } from "./types";

const roleVar = (role: Role) => `var(--${role.replace(/_/g, "-")})`;

function RunImage({ run }: { run: Run }) {
  const [failed, setFailed] = useState(false);
  const image = run.image!;
  if (failed) return <>{image.fallback}</>;
  return (
    <img
      className="tl-inline-emoji"
      src={assetUrl(image.src)}
      alt={image.fallback}
      title={image.fallback}
      width={image.size}
      height={image.size}
      onError={() => setFailed(true)}
      draggable={false}
    />
  );
}

/** Opens a link the way `open_url_or_deep_link` does (profiles, channels, deep links). */
export function openLink(url: string) {
  send("timeline", "open_url", { url });
}

function RunSpan({ run }: { run: Run }) {
  if (run.image) return <RunImage run={run} />;
  const internal = run.link?.startsWith("zbase-") ?? false;
  const style: CSSProperties = {};
  if (run.color) style.color = roleVar(run.color);
  if (run.bg) {
    style.background = roleVar(run.bg);
    style.borderRadius = 3;
  }
  if (run.bold) style.fontWeight = 700;
  if (run.italic) style.fontStyle = "italic";
  if (run.strike) style.textDecoration = "line-through";
  const classes = [
    run.mono && "tl-run-mono",
    run.meta && "tl-run-meta",
    run.link && (internal ? "tl-run-internal" : "tl-run-link"),
  ]
    .filter(Boolean)
    .join(" ");
  if (!run.link && !classes && Object.keys(style).length === 0) return <>{run.text}</>;
  return (
    <span
      className={classes || undefined}
      style={style}
      onClick={
        run.link
          ? (event) => {
              // A drag that selected text is not a click.
              if (!window.getSelection()?.isCollapsed) return;
              event.stopPropagation();
              openLink(run.link!);
            }
          : undefined
      }
    >
      {run.text}
    </span>
  );
}

export function Runs({ runs }: { runs: Run[] }) {
  return (
    <>
      {runs.map((run, index) => (
        <RunSpan key={index} run={run} />
      ))}
    </>
  );
}

/** `render_message_fragments`: paragraphs, code blocks and quotes. */
export function MessageBlocks({ blocks, emojiOnly }: { blocks: Block[]; emojiOnly: boolean }) {
  if (blocks.length === 0) return null;
  const rendered = blocks.map((block, index) => {
    switch (block.kind) {
      case "inline":
        return (
          <div key={index} className={`tl-text${emojiOnly ? " emoji-only tl-emoji-only" : ""}`}>
            <Runs runs={block.runs} />
          </div>
        );
      case "code":
        return (
          <div key={index} className="tl-code-block">
            <Runs runs={block.runs} />
          </div>
        );
      case "quote":
        return (
          <div key={index} className="tl-quote">
            <Runs runs={block.runs} />
          </div>
        );
    }
  });
  if (rendered.length === 1) return rendered[0];
  return <div className="tl-blocks">{rendered}</div>;
}
