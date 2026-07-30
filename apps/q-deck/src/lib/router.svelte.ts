// Three routes total (/, /runs/:id, /conversations/:id) — a full router
// dependency isn't proportionate. Plain History API + a reactive path.

const state = $state({ path: window.location.pathname });

window.addEventListener("popstate", () => {
  state.path = window.location.pathname;
});

export function currentPath(): string {
  return state.path;
}

export function navigate(path: string): void {
  if (path === state.path) return;
  history.pushState({}, "", path);
  state.path = path;
}

export type Route =
  | { name: "dashboard" }
  | { name: "run"; runId: string }
  | { name: "conversation"; conversationId: string }
  | { name: "not-found" };

/** `decodeURIComponent` throws on a malformed percent-escape (e.g. a
 * truncated multi-byte sequence from a hand-edited or corrupted URL) — that
 * must resolve to "not found", not crash the whole app on navigation. */
function tryDecode(raw: string): string | null {
  try {
    return decodeURIComponent(raw);
  } catch {
    return null;
  }
}

export function resolve(path: string): Route {
  if (path === "/") return { name: "dashboard" };
  const runMatch = /^\/runs\/([^/]+)\/?$/.exec(path);
  if (runMatch) {
    const runId = tryDecode(runMatch[1]);
    return runId === null ? { name: "not-found" } : { name: "run", runId };
  }
  const convMatch = /^\/conversations\/([^/]+)\/?$/.exec(path);
  if (convMatch) {
    const conversationId = tryDecode(convMatch[1]);
    return conversationId === null ? { name: "not-found" } : { name: "conversation", conversationId };
  }
  return { name: "not-found" };
}
