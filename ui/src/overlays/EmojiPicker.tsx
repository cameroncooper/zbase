// `emoji_picker::render_emoji_picker` (src/views/overlays/emoji_picker.rs).
// The catalog (sections, search, footer) is built by the engine; picks go back
// as keys and the engine reacts or asks the composer to insert.

import { memo, useRef, useState } from "react";
import { assetUrl, send, useModel } from "../core/engine";
import { Badge } from "../ui/common";
import { Icon } from "../ui/Icon";
import { focusedComposer, useModelInput } from "./MessageHit";
import type { EmojiCell, EmojiPickerModel, EmojiPickerView } from "./types";

let lastHovered: string | null = null;

function hover(key: string) {
  if (lastHovered === key) return;
  lastHovered = key;
  send("overlays", "set_emoji_hovered", { key });
}

function pick(key: string) {
  const composer = focusedComposer();
  send("overlays", "pick_emoji", {
    key,
    thread_focused: composer === "thread",
    composer_focused: composer === "conversation",
  });
}

export function EmojiPicker() {
  const picker = useModel("emoji_picker") as EmojiPickerModel | undefined;
  const extra = useModel("overlays_extra");
  const view = extra?.emoji_picker as EmojiPickerView | null | undefined;
  const input = useModelInput(picker?.query, (query) => send("overlays", "set_emoji_query", { query }));
  const inputRef = useRef<HTMLInputElement>(null);
  if (!picker || !view) return null;

  return (
    <div className="ep-panel" onClick={(event) => event.stopPropagation()}>
      <div className="ep-search">
        <Icon name="search" size={16} color="var(--text-secondary)" />
        <div style={{ flex: 1, minWidth: 0, display: "flex" }} onClick={() => inputRef.current?.focus()}>
          <input
            placeholder="Search emoji"
            ref={inputRef}
            className="ov-input"
            spellCheck={false}
            autoComplete="off"
            data-overlay-input="emoji-picker"
            {...input}
          />
        </div>
        <Badge
          background="var(--panel-alt-bg)"
          foreground="var(--text-secondary)"
          onClick={() => send("overlays", "toggle_emoji_tone_expanded")}
        >
          Tone
        </Badge>
        <Badge
          background="var(--panel-alt-bg)"
          foreground="var(--text-secondary)"
          onClick={() => send("overlays", "dismiss_overlays")}
        >
          Close
        </Badge>
      </div>
      {picker.skin_tone_expanded && (
        <div className="ep-tones">
          {view.tones.map((tone) => (
            <div
              key={tone.tone}
              className="ep-tone"
              style={{ background: tone.color, borderColor: tone.selected ? "var(--accent)" : undefined }}
              onClick={() => send("overlays", "set_emoji_tone", { tone: tone.tone })}
            />
          ))}
        </div>
      )}
      {view.search_results ? (
        <div className="ov-scroll ep-scroll">
          <div className="ep-content">
            <div className="ep-section-title">Search results</div>
            {view.search_results.length === 0 ? (
              <div className="ep-empty">No emoji matches</div>
            ) : (
              <Grid items={view.search_results} />
            )}
          </div>
        </div>
      ) : (
        <>
          <div className="ep-bookmarks">
            {view.bookmarks.map((bookmark) => (
              <div
                key={bookmark.label}
                className={`ep-bookmark${bookmark.selected ? " ep-selected" : ""}`}
                onClick={() => send("overlays", "set_emoji_group", { group: bookmark.group })}
              >
                {bookmark.label}
              </div>
            ))}
          </div>
          <Sections view={view} />
        </>
      )}
      <Footer view={view} />
    </div>
  );
}

/** The active group is moved first by the engine; the list scrolls back to the top on a change. */
const Sections = memo(function Sections({ view }: { view: EmojiPickerView }) {
  return (
    <div className="ov-scroll ep-scroll" key={view.sections[0]?.title}>
      <div className="ep-content">
        {view.sections.map((section) => (
          <SectionBlock key={section.title} title={section.title} items={section.items} />
        ))}
      </div>
    </div>
  );
});

const SectionBlock = memo(function SectionBlock({ title, items }: { title: string; items: EmojiCell[] }) {
  return (
    <>
      <div className="ep-section-title">{title}</div>
      <Grid items={items} />
    </>
  );
});

function Grid({ items }: { items: EmojiCell[] }) {
  return (
    <div className="ep-grid">
      {items.map((item) => (
        <div
          key={item.key}
          className="ep-cell"
          onMouseMove={() => hover(item.key)}
          // Keep focus in the composer so the pick knows where to insert.
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => pick(item.key)}
        >
          <EmojiGlyph cell={item} />
        </div>
      ))}
    </div>
  );
}

function EmojiGlyph({ cell }: { cell: EmojiCell }) {
  const [failed, setFailed] = useState(false);
  if (cell.asset && !failed) {
    return (
      <img
        src={assetUrl(cell.asset)}
        alt={`:${cell.alias}:`}
        draggable={false}
        onError={() => setFailed(true)}
        style={{ width: 24, height: 24, objectFit: "contain" }}
      />
    );
  }
  if (cell.glyph) return <span className="ep-glyph">{cell.glyph}</span>;
  return <span className="ep-alias">{`:${cell.alias}:`}</span>;
}

function Footer({ view }: { view: EmojiPickerView }) {
  const footer = view.footer;
  return (
    <div className="ep-footer">
      {footer ? (
        <>
          <div
            style={{
              width: 30,
              height: 30,
              borderRadius: 4,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <EmojiGlyph cell={footer.cell} key={footer.cell.key} />
          </div>
          <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
            <div style={{ fontSize: 14, color: "var(--text-primary)" }}>{footer.title}</div>
            <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>{footer.subtitle}</div>
          </div>
        </>
      ) : (
        <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>Pick an emoji</div>
      )}
    </div>
  );
}
