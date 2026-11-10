// Small building blocks from src/views/mod.rs shared across areas:
// empty_state, section_label, keycap, badge, primary/secondary buttons and
// search_field_shell.

import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { Icon, type IconName } from "./Icon";
import "./ui.css";

// `window_is_active()` decides selection colors; mirror it as an attribute.
function syncWindowActive() {
  if (document.hasFocus()) delete document.documentElement.dataset.windowInactive;
  else document.documentElement.dataset.windowInactive = "";
}
window.addEventListener("focus", syncWindowActive);
window.addEventListener("blur", syncWindowActive);
syncWindowActive();

/** `empty_state`: a washed circle with an icon, a title and an optional body. */
export function EmptyState({
  icon,
  title,
  body,
  children,
}: {
  icon: IconName;
  title: ReactNode;
  body?: string | null;
  children?: ReactNode;
}) {
  return (
    <div
      style={{
        width: "100%",
        padding: "32px 24px",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 4,
      }}
    >
      <div
        style={{
          width: 40,
          height: 40,
          marginBottom: 8,
          borderRadius: "50%",
          background: "var(--empty-wash)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name={icon} size={18} color="var(--text-secondary)" />
      </div>
      <div style={{ fontSize: 14, lineHeight: "20px", fontWeight: 600, color: "var(--text-primary)", textAlign: "center" }}>
        {title}
      </div>
      {body && (
        <div
          style={{
            maxWidth: 300,
            fontSize: 13,
            lineHeight: "18px",
            color: "var(--text-secondary)",
            textAlign: "center",
          }}
        >
          {body}
        </div>
      )}
      {children}
    </div>
  );
}

/** `section_label`. */
export function SectionLabel({ children, style }: { children: ReactNode; style?: CSSProperties }) {
  return (
    <div style={{ fontSize: 12, lineHeight: "16px", fontWeight: 600, color: "var(--text-secondary)", ...style }}>
      {children}
    </div>
  );
}

/** `keycap`. */
export function Keycap({ children }: { children: ReactNode }) {
  return (
    <span
      style={{
        height: 18,
        minWidth: 18,
        padding: "0 4px",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
        borderRadius: 4,
        border: "1px solid var(--field-border)",
        background: "var(--field-bg)",
        fontSize: 11,
        lineHeight: "14px",
        fontWeight: 500,
        color: "var(--text-secondary)",
        boxSizing: "border-box",
      }}
    >
      {children}
    </span>
  );
}

/** `badge`: a small pill. */
export function Badge({
  children,
  background,
  foreground,
  onClick,
}: {
  children: ReactNode;
  background: string;
  foreground: string;
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        padding: "4px 10px",
        borderRadius: 9999,
        background,
        color: foreground,
        fontSize: 12,
        lineHeight: "16px",
        fontWeight: 500,
        flexShrink: 0,
      }}
    >
      {children}
    </button>
  );
}

const buttonBase: CSSProperties = {
  height: 28,
  padding: "0 12px",
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  borderRadius: 6,
  fontSize: 13,
  fontWeight: 500,
  cursor: "pointer",
  flexShrink: 0,
};

/** `primary_button`; `disabled` dims it to 45% like the new chat start button. */
export function PrimaryButton({
  children,
  onClick,
  disabled = false,
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
}) {
  const [hover, setHover] = useState(false);
  return (
    <button
      type="button"
      onClick={disabled ? undefined : onClick}
      onPointerEnter={() => setHover(true)}
      onPointerLeave={() => setHover(false)}
      style={{
        ...buttonBase,
        background: hover && !disabled ? "var(--accent)" : "var(--accent-strong)",
        color: "#ffffff",
        opacity: disabled ? 0.45 : 1,
        cursor: disabled ? "default" : "pointer",
      }}
    >
      {children}
    </button>
  );
}

/** `secondary_button`. */
export function SecondaryButton({ children, onClick }: { children: ReactNode; onClick?: () => void }) {
  const [hover, setHover] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      onPointerEnter={() => setHover(true)}
      onPointerLeave={() => setHover(false)}
      style={{
        ...buttonBase,
        border: "1px solid var(--field-border)",
        background: hover ? "var(--hover-wash-button)" : "var(--field-bg)",
        color: "var(--text-primary)",
      }}
    >
      {children}
    </button>
  );
}

/** `search_field_shell`: the bordered field with a search icon; accent border and ring when focused. */
export function SearchFieldShell({
  children,
  focused,
  height = 36,
  onClick,
}: {
  children: ReactNode;
  focused: boolean;
  height?: number;
  onClick?: () => void;
}) {
  return (
    <div
      onClick={onClick}
      style={{
        height,
        width: "100%",
        padding: "0 8px 0 10px",
        display: "flex",
        alignItems: "center",
        gap: 8,
        borderRadius: 8,
        border: `1px solid ${focused ? "var(--accent)" : "var(--field-border)"}`,
        boxShadow: focused ? "var(--focus-ring)" : undefined,
        background: "var(--field-bg)",
        cursor: "text",
        fontSize: 13,
        lineHeight: "18px",
        color: "var(--text-primary)",
        boxSizing: "border-box",
      }}
    >
      <Icon name="search" size={14} color="var(--text-secondary)" />
      <div style={{ flex: 1, minWidth: 0, display: "flex" }}>{children}</div>
    </div>
  );
}

/** Tracks focus of an input for focus-dependent styling. */
export function useFocused(): [boolean, { onFocus: () => void; onBlur: () => void }] {
  const [focused, setFocused] = useState(false);
  useEffect(() => {
    const onBlur = () => setFocused(false);
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, []);
  return [focused, { onFocus: () => setFocused(true), onBlur: () => setFocused(false) }];
}

/** Text input styled like gpui's `TextField`: no chrome, inherits font. */
export const bareInputStyle: CSSProperties = {
  flex: 1,
  minWidth: 0,
  width: "100%",
  border: "none",
  outline: "none",
  background: "transparent",
  padding: 0,
  font: "inherit",
  color: "inherit",
  userSelect: "text",
};
