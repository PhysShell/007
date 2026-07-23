import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { registerAppShellSW } from "./sw-register";
import "./styles.css";

const rootEl = document.getElementById("root");
if (rootEl) {
  // Deep-link a conversation with ?c=<id> — used by the screenshot script and
  // handy for sharing a specific fixture state. UI-only convenience, not routing.
  const params = new URLSearchParams(window.location.search);
  const initialConversationId = params.get("c");
  createRoot(rootEl).render(
    <StrictMode>
      <App initialConversationId={initialConversationId} />
    </StrictMode>
  );
  registerAppShellSW();
}
