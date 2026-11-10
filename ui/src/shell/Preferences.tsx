// The Settings window (views/settings_window.rs) and its content
// (views/preferences): every control writes back through a shell intent; the
// engine owns and persists the settings.

import type { ReactNode } from "react";
import { send, useModel } from "../core/engine";
import { Avatar } from "./Avatar";
import type { Connectivity, ThemeMode, UpdateSnapshot } from "./types";

function RowText({ title, description }: { title: string; description: string }) {
  return (
    <div className="row-text">
      <div className="title">{title}</div>
      <div className="description">{description}</div>
    </div>
  );
}

function SettingRow({ title, description, children }: { title: string; description: string; children: ReactNode }) {
  return (
    <div className="setting-row">
      <RowText title={title} description={description} />
      <div style={{ flexShrink: 0 }}>{children}</div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="settings-section">
      <div className="section-label">{title}</div>
      <div className="settings-group">{children}</div>
    </div>
  );
}

function Segmented<T extends string>({ options, onSelect }: { options: [label: string, selected: boolean, value: T][]; onSelect: (value: T) => void }) {
  return (
    <div className="segmented">
      {options.map(([label, selected, value]) => (
        <button key={label} className={selected ? "selected" : undefined} onClick={selected ? undefined : () => onSelect(value)}>
          {label}
        </button>
      ))}
    </div>
  );
}

function Toggle({ on, onFlip }: { on: boolean; onFlip: () => void }) {
  return (
    <button className={`toggle${on ? " on" : ""}`} role="switch" aria-checked={on} onClick={onFlip}>
      <div />
    </button>
  );
}

/** `mini_window`: a sketch of the app in fixed light or dark colors. */
function MiniWindow({ dark }: { dark: boolean }) {
  const [window, sidebar, line, field, fieldLine] = dark
    ? ["#191919", "#202020", "#4a4a4a", "#202020", "#3b3b3b"]
    : ["#ffffff", "#f2f4f7", "#d9dce1", "#f5f6f8", "#d6d9de"];
  const selected = dark ? "#2b4a85" : "#3663ea";
  const bar = (width: number) => <div style={{ height: 3, width, borderRadius: 999, background: line }} />;
  return (
    <div style={{ width: "100%", height: "100%", display: "flex", background: window }}>
      <div style={{ width: 24, height: "100%", background: sidebar, padding: "10px 3px 0", display: "flex", flexDirection: "column", gap: 3 }}>
        <div style={{ height: 5, width: "100%", borderRadius: 1.5, background: selected }} />
        {bar(14)}
        {bar(12)}
        {bar(15)}
      </div>
      <div style={{ flex: 1, height: "100%", padding: "9px 6px 5px", display: "flex", flexDirection: "column", gap: 4 }}>
        {bar(30)}
        {bar(44)}
        {bar(22)}
        <div style={{ flex: 1 }} />
        <div style={{ height: 9, width: "100%", borderRadius: 2.5, border: `1px solid ${fieldLine}`, background: field, boxSizing: "border-box" }} />
      </div>
    </div>
  );
}

function ThemeTile({ mode, label, selected }: { mode: ThemeMode; label: string; selected: boolean }) {
  return (
    <div className={`theme-tile${selected ? " selected" : ""}`} onClick={() => send("shell", "set_theme_mode", { mode })}>
      <div className="ring">
        <div className="thumbnail">
          {mode === "System" ? (
            <>
              <MiniWindow dark={false} />
              <div style={{ position: "absolute", top: 0, left: 44, width: 44, height: "100%", overflow: "hidden" }}>
                <div style={{ position: "absolute", top: 0, left: -44, width: 88, height: "100%" }}>
                  <MiniWindow dark />
                </div>
              </div>
            </>
          ) : (
            <MiniWindow dark={mode === "Dark"} />
          )}
        </div>
      </div>
      <div className="label">{label}</div>
    </div>
  );
}

const CONNECTION: Record<Connectivity, [string, string]> = {
  Online: ["Connected to Keybase", "var(--success)"],
  Reconnecting: ["Connecting to Keybase…", "var(--warning)"],
  Offline: ["Keybase can't be reached", "var(--text-secondary)"],
};

function AccountRow({ name, username, avatar, connectivity }: { name: string; username: string; avatar: string | null; connectivity: Connectivity }) {
  const [status, dot] = CONNECTION[connectivity];
  return (
    <div style={{ minHeight: 64, padding: "12px 16px", display: "flex", alignItems: "center", gap: 12 }}>
      <Avatar name={name} asset={avatar} size={36} />
      <div className="row-text">
        <div className="title">{name}</div>
        <div className="description">Signed in as {username}</div>
      </div>
      <div style={{ flexShrink: 0, display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: "var(--text-secondary)" }}>
        <div
          style={{
            width: 8,
            height: 8,
            borderRadius: "50%",
            boxSizing: "border-box",
            ...(connectivity === "Offline" ? { border: `1.5px solid ${dot}` } : { background: dot }),
          }}
        />
        {status}
      </div>
    </div>
  );
}

/** `update_settings_rows`. */
function UpdateRows({ snapshot }: { snapshot: UpdateSnapshot }) {
  const command = (action: string) => send("shell", "update_command", { action });
  const note = (text: string) => (
    <div style={{ padding: "8px 16px", fontSize: 12, color: "var(--text-secondary)" }}>{text}</div>
  );
  const last =
    snapshot.last_check > 0
      ? new Date(snapshot.last_check * 1000)
          .toLocaleString("en-US", { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })
          .replace(",", " at")
      : "Not checked yet";
  return (
    <>
      <SettingRow title="Software updates" description="Check for the latest version of zBase.">
        <button style={{ color: "var(--accent)", cursor: "pointer" }} onClick={() => send("shell", "check_for_updates")}>
          Check now
        </button>
      </SettingRow>
      {snapshot.state !== "disabled" ? (
        <>
          <SettingRow title="Automatically check" description="Look for stable updates in the background.">
            <Toggle on={snapshot.automatic_checks} onFlip={() => command("toggle_checks")} />
          </SettingRow>
          <SettingRow title="Download in background" description="Install on quit, or restart when ready.">
            <Toggle on={snapshot.automatic_downloads} onFlip={() => command("toggle_downloads")} />
          </SettingRow>
          {note(`Last checked: ${last}`)}
        </>
      ) : (
        note(snapshot.message)
      )}
    </>
  );
}

/** `PreferencesView`. */
export function Preferences() {
  const settings = useModel("settings");
  const sidebar = useModel("sidebar");
  const app = useModel("app");
  const shell = useModel("shell");
  if (!settings || !sidebar || !app || !shell) return null;
  const account = app.current_user_id
    ? {
        name: app.current_user_display_name.trim() || app.current_user_id,
        username: app.current_user_id,
      }
    : null;
  return (
    <div className="settings-scroll">
      <div className="settings-content">
        {account && (
          <Section title="Account">
            <AccountRow name={account.name} username={account.username} avatar={app.current_user_avatar_asset} connectivity={app.connectivity} />
          </Section>
        )}
        <Section title="Appearance">
          <div style={{ padding: "14px 16px", display: "flex", flexDirection: "column", gap: 12 }}>
            <RowText title="Theme" description="Match your Mac, or keep zBase light or dark." />
            <div style={{ display: "flex", gap: 16 }}>
              {(
                [
                  ["System", "System"],
                  ["Light", "Light"],
                  ["Dark", "Dark"],
                ] as const
              ).map(([mode, label]) => (
                <ThemeTile key={mode} mode={mode} label={label} selected={settings.theme_mode === mode} />
              ))}
            </div>
          </div>
          <SettingRow title="Message density" description="Compact fits more messages on screen.">
            <Segmented
              options={[
                ["Comfortable", settings.density === "Comfortable", "Comfortable"],
                ["Compact", settings.density === "Compact", "Compact"],
              ]}
              onSelect={(density) => send("shell", "set_density", { density })}
            />
          </SettingRow>
        </Section>
        <Section title="Layout">
          <SettingRow title="Sidebar order" description="Newest activity first, or channels under their teams.">
            <Segmented
              options={[
                ["Recent", sidebar.mode === "Recent", "Recent"],
                ["By team", sidebar.mode === "Grouped", "Grouped"],
              ]}
              onSelect={(mode) => send("shell", "set_sidebar_mode", { mode })}
            />
          </SettingRow>
          <SettingRow title="Sidebar rows" description="Previews add the latest message under each name.">
            <Segmented
              options={[
                ["Names only", settings.sidebar_style === "Compact", "Compact"],
                ["With previews", settings.sidebar_style === "Previews", "Previews"],
              ]}
              onSelect={(style) => send("shell", "set_sidebar_style", { style })}
            />
          </SettingRow>
          <SettingRow title="Side panel" description="Open threads, members and profiles beside the conversation.">
            <Toggle on={settings.show_right_pane} onFlip={() => send("shell", "set_show_right_pane", { show: !settings.show_right_pane })} />
          </SettingRow>
        </Section>
        <Section title="Accessibility">
          <SettingRow title="Reduce motion" description="Hold the typing indicator still and skip animations.">
            <Toggle on={settings.reduced_motion} onFlip={() => send("shell", "set_reduced_motion", { reduced: !settings.reduced_motion })} />
          </SettingRow>
        </Section>
        <Section title="Updates">
          <UpdateRows snapshot={shell.update.snapshot} />
        </Section>
        <div style={{ padding: "0 4px", fontSize: 12, color: "var(--text-secondary)" }}>
          zBase {shell.version} · Settings are saved on this Mac.
        </div>
      </div>
    </div>
  );
}

/**
 * `SettingsWindow`: the title strip over the preferences. Cmd+W, Cmd+Q and
 * Cmd+, come from the menu bar, which routes them to the key window.
 */
export function SettingsWindow() {
  return (
    <div className="settings-window">
      <div className="settings-title" data-tauri-drag-region>
        Settings
      </div>
      <Preferences />
    </div>
  );
}
