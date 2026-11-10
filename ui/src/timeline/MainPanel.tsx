import { Composer } from "../composer/Composer";
import { FindBar } from "../overlays/FindBar";
import { ConnectionBanner, ConversationHeader, PinnedBanner } from "./Header";
import { TimelineList } from "./TimelineList";
import "./timeline.css";

/**
 * The conversation view (views/conversation `ConversationView::render`):
 * header, connection and pinned banners, the find bar, the timeline and the
 * composer (which draws the typing indicator in its reserved row, and the join
 * bar for channels you can only read).
 */
export function MainPanel() {
  return (
    <div className="tl-view">
      <ConversationHeader />
      <ConnectionBanner />
      <PinnedBanner />
      <FindBar />
      <TimelineList />
      <Composer target={{ kind: "conversation" }} />
    </div>
  );
}
