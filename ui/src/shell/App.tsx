import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { readModel, send, useModel } from "../core/engine";
import { useShortcuts } from "../core/shortcuts";
import { MainPanel } from "../timeline/MainPanel";
import { RightPane } from "../right_pane/RightPane";
import { Overlays } from "../overlays/Overlays";
import { SearchView } from "../overlays/SearchView";
import { Sidebar } from "./Sidebar";
import { ActivityView, CallWindow, HomeView, MiniCallDock } from "./Pages";
import { KeybaseInspector, Splash, Toasts, UpdatePanel } from "./Floating";
import { SettingsWindow } from "./Preferences";
import { isConversation, isRoute } from "./routes";
import "./shell.css";

/** views/mod.rs `MAIN_PANEL_MIN_WIDTH_PX`, `RIGHT_PANE_RESIZE_HANDLE_WIDTH_PX`, `SIDEBAR_WIDTH_PX`. */
const MAIN_PANEL_MIN_WIDTH_PX = 560;
const RESIZE_HANDLE_WIDTH_PX = 4;
const SIDEBAR_WIDTH_PX = 236;

const IS_SETTINGS_WINDOW = new URLSearchParams(window.location.search).get("window") === "settings";

/** The window: the main layout, or Settings in its own window. */
export function App() {
  useAppearance();
  useDeepLinks();
  return IS_SETTINGS_WINDOW ? <SettingsWindow /> : <MainWindow />;
}

/** `AppWindow::render`: sidebar, resize handle, main panel, right pane, then the layers. */
function MainWindow() {
  const navigation = useModel("navigation");
  const sidebar = useModel("sidebar");
  const shell = useModel("shell");
  const call = useModel("call");
  const notifications = useModel("notifications");
  const route = navigation?.current;
  const resize = useSidebarResize(sidebar?.width_px ?? SIDEBAR_WIDTH_PX);
  useAppShortcuts();
  useFitWindowToSidebar(sidebar?.width_px, sidebar?.style);
  return (
    <div className={`shell${resize.dragging ? " resizing" : ""}`}>
      <Sidebar widthPx={resize.width} />
      <div className={`sidebar-resize${resize.dragging ? " dragging" : ""}`} onPointerDown={resize.begin}>
        <div />
      </div>
      <main className="shell-main">
        {isConversation(route) ? (
          <MainPanel />
        ) : isRoute(route, "Search") ? (
          <SearchView />
        ) : isRoute(route, "WorkspaceHome") ? (
          <HomeView />
        ) : isRoute(route, "Activity") ? (
          <ActivityView />
        ) : isRoute(route, "ActiveCall") ? (
          <CallWindow />
        ) : null}
      </main>
      <RightPane />
      {call?.active_call && <MiniCallDock call={call} />}
      <Overlays />
      {notifications && <Toasts notifications={notifications} />}
      {shell?.update.open && <UpdatePanel shell={shell} sidebarWidth={resize.width} />}
      {shell?.inspector.open && <KeybaseInspector shell={shell} />}
      {shell?.splash_open && <Splash status={shell.splash_status} />}
    </div>
  );
}

/**
 * Theme, density and motion from Settings on `<html>`: `data-theme` resolves
 * System through `prefers-color-scheme`, like `resolve_theme`. Also marks
 * whether the window is key, for selection colors.
 */
function useAppearance() {
  const settings = useModel("settings");
  const [systemDark, setSystemDark] = useState(() => window.matchMedia("(prefers-color-scheme: dark)").matches);
  const [active, setActive] = useState(() => document.hasFocus());
  useEffect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setSystemDark(query.matches);
    query.addEventListener("change", onChange);
    const onFocus = () => setActive(true);
    const onBlur = () => setActive(false);
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      query.removeEventListener("change", onChange);
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, []);
  const mode = settings?.theme_mode ?? "System";
  const theme = mode === "Dark" || (mode === "System" && systemDark) ? "dark" : "light";
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = theme;
    root.dataset.density = (settings?.density ?? "Comfortable").toLowerCase();
    root.dataset.reducedMotion = String(settings?.reduced_motion ?? false);
    root.dataset.windowActive = String(active);
  }, [theme, settings?.density, settings?.reduced_motion, active]);
}

/**
 * In-app links: `keybase://chat/...` deep links, `zbase-mention:` and
 * `zbase-channel:` go to the engine (`open_url_or_deep_link`) before the
 * generic handler in main.tsx sends links to the browser.
 */
function useDeepLinks() {
  useEffect(() => {
    const onClick = (event: MouseEvent) => {
      const link = (event.target as Element).closest?.("a[href]") as HTMLAnchorElement | null;
      const href = link?.getAttribute("href");
      if (!href || !/^(keybase:\/\/|zbase-mention:|zbase-channel:)/.test(href)) return;
      event.preventDefault();
      event.stopPropagation();
      send("shell", "open_url", { url: href });
    };
    window.addEventListener("click", onClick, true);
    return () => window.removeEventListener("click", onClick, true);
  }, []);
}

/** App-level bindings from app/key_bindings.rs that the shell owns. */
function useAppShortcuts() {
  const shell = () => readModel("shell");
  useShortcuts([
    { keys: "cmd-[", handler: () => void send("shell", "navigate_back") },
    { keys: "cmd-]", handler: () => void send("shell", "navigate_forward") },
    { keys: "cmd-1", handler: () => void send("shell", "show_home") },
    { keys: "cmd-2", handler: () => void send("shell", "open_recent", { index: 1 }) },
    { keys: "cmd-3", handler: () => void send("shell", "open_recent", { index: 2 }) },
    { keys: "cmd-4", handler: () => void send("shell", "open_recent", { index: 3 }) },
    { keys: "cmd-5", handler: () => void send("shell", "open_recent", { index: 4 }) },
    { keys: "cmd-shift-a", handler: () => void send("shell", "show_activity") },
    { keys: "alt-up", handler: () => void send("shell", "select_sidebar", { direction: -1 }) },
    { keys: "alt-down", handler: () => void send("shell", "select_sidebar", { direction: 1 }) },
    { keys: "alt-enter", handler: () => void send("shell", "activate_sidebar_selection") },
    { keys: "cmd-shift-i", handler: () => void send("shell", "toggle_keybase_inspector") },
    { keys: "cmd-shift-s", handler: () => void send("shell", "toggle_splash") },
    // `dismiss_overlays_action`: the update panel closes first; the inspector
    // and splash close with every other overlay.
    {
      keys: "escape",
      when: () => !!shell()?.update.open,
      handler: () => void send("shell", "close_updates"),
    },
    {
      keys: "escape",
      when: () => !!(shell()?.inspector.open || shell()?.splash_open),
      handler: () => void send("shell", "dismiss_overlays"),
    },
  ]);
}

/** `begin_sidebar_resize` / `update_sidebar_resize`: drag the handle, clamped to 160–360px. */
function useSidebarResize(modelWidth: number) {
  const [drag, setDrag] = useState<{ anchor: number; start: number; width: number } | null>(null);
  const frame = useRef(0);
  const begin = (event: React.PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    const state = { anchor: event.clientX, start: modelWidth, width: modelWidth };
    setDrag(state);
    const onMove = (move: PointerEvent) => {
      const width = Math.min(360, Math.max(160, state.start + move.clientX - state.anchor));
      state.width = width;
      setDrag({ ...state });
      cancelAnimationFrame(frame.current);
      frame.current = requestAnimationFrame(() => send("shell", "set_sidebar_width", { width_px: width }));
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      send("shell", "set_sidebar_width", { width_px: state.width });
      setDrag(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };
  return { width: drag?.width ?? modelWidth, dragging: !!drag, begin };
}

/**
 * `fit_window_to_sidebar`: after the sidebar widens for a row style (and at
 * launch), keep the conversation at its minimum width by growing the window
 * when the screen has room, else by narrowing the sidebar.
 */
function useFitWindowToSidebar(width: number | undefined, style: string | undefined) {
  useEffect(() => {
    if (width === undefined) return;
    const required = width + RESIZE_HANDLE_WIDTH_PX + MAIN_PANEL_MIN_WIDTH_PX;
    if (window.innerWidth + 1 >= required) return;
    const appWindow = getCurrentWindow();
    void (async () => {
      const fullscreen = await appWindow.isFullscreen().catch(() => false);
      const maximized = await appWindow.isMaximized().catch(() => false);
      if (!fullscreen && !maximized && window.screen.availWidth >= required) {
        await appWindow.setSize(new LogicalSize(required, window.innerHeight)).catch(() => undefined);
      } else {
        const fitted = Math.max(SIDEBAR_WIDTH_PX, window.innerWidth - RESIZE_HANDLE_WIDTH_PX - MAIN_PANEL_MIN_WIDTH_PX);
        if (fitted < width) send("shell", "set_sidebar_width", { width_px: fitted });
      }
    })();
    // Only when the style changes, as gpui's `sidebar_fit_pending`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [style]);
}
