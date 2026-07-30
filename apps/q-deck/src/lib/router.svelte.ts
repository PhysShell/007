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

export function resolve(path: string): Route {
  if (path === "/") return { name: "dashboard" };
  const runMatch = /^\/runs\/([^/]+)\/?$/.exec(path);
  if (runMatch) return { name: "run", runId: decodeURIComponent(runMatch[1]) };
  const convMatch = /^\/conversations\/([^/]+)\/?$/.exec(path);
  if (convMatch) return { name: "conversation", conversationId: decodeURIComponent(convMatch[1]) };
  return { name: "not-found" };
}
