import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { openUrl } from "@tauri-apps/plugin-opener";
import { startEngine } from "./core/engine";
import { App } from "./shell/App";
import "./core/palette.css";
import "./core/base.css";

// Links open in the default browser rather than replacing the app's page.
document.addEventListener("click", (event) => {
  const link = (event.target as Element).closest?.("a[href]");
  if (!link) return;
  event.preventDefault();
  openUrl((link as HTMLAnchorElement).href);
});

startEngine();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
