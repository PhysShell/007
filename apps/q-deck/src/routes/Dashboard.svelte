<script lang="ts">
  import { onDestroy } from "svelte";
  import { listRuns } from "../lib/api";
  import type { RunDto } from "../lib/types";
  import { isActiveRun } from "../lib/types";
  import { relativeAge } from "../lib/format";
  import Link from "../components/Link.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";

  type LoadState = "loading" | "ready" | "error" | "offline";

  // Fetched separately, not derived from one bounded page: "active" must be
  // the real active set (a status-filtered query the database answers
  // completely), not whatever happened to fall inside one newest-first page
  // — an older queued/running run must not silently vanish from "Running"
  // just because enough newer runs exist to push it past a fixed-size page.
  // The status filter alone only bounds what KIND of run comes back, not HOW
  // MANY — with 201+ genuinely active runs, a single page still truncates,
  // so fetchAllActive below pages through every page, same as
  // ConversationPage's full-history walk.
  let active: RunDto[] = $state([]);
  let terminal: RunDto[] = $state([]);
  let loadState: LoadState = $state("loading");
  let lastRefreshed: number | null = $state(null);

  const REFRESH_INTERVAL_MS = 5000;
  const RECENT_LIMIT = 20;
  let timer: ReturnType<typeof setInterval> | undefined;
  // Overlapping refresh() calls can resolve out of order (a slow request
  // started earlier finishing after a faster later one) — a generation
  // counter lets a stale completion recognize itself and no-op instead of
  // overwriting newer state with older data.
  let generation = 0;

  async function fetchAllActive(): Promise<RunDto[]> {
    const collected: RunDto[] = [];
    let before: string | undefined;
    for (;;) {
      const pg = await listRuns({ status: ["queued", "running"], limit: 200, before });
      collected.push(...pg.items);
      if (pg.items.length === 0 || !pg.next_before) break;
      before = pg.next_before;
    }
    return collected;
  }

  async function refresh(): Promise<void> {
    const myGeneration = ++generation;
    try {
      const [activeItems, recentPage] = await Promise.all([
        fetchAllActive(),
        listRuns({ limit: RECENT_LIMIT }),
      ]);
      if (myGeneration !== generation) return; // superseded by a later refresh
      active = activeItems;
      terminal = recentPage.items
        .filter((r) => !isActiveRun(r.status))
        .sort((a, b) => b.created_at - a.created_at);
      loadState = "ready";
      lastRefreshed = Date.now();
    } catch {
      if (myGeneration !== generation) return;
      // Keep the last-known list on screen — going blank on a transient
      // network blip would be a worse failure than showing slightly stale
      // data with an honest "offline" indicator.
      loadState = active.length > 0 || terminal.length > 0 ? "offline" : "error";
    }
  }

  $effect(() => {
    void refresh();
    timer = setInterval(() => void refresh(), REFRESH_INTERVAL_MS);
    return () => clearInterval(timer);
  });

  onDestroy(() => clearInterval(timer));
</script>

<div class="dashboard">
  <header class="page-header">
    <h1>Q-Deck</h1>
    <p class="subtitle">007 control surface</p>
  </header>

  {#if loadState === "loading"}
    <div class="state-message">Loading runs…</div>
  {:else if loadState === "error"}
    <div class="state-message error">Couldn't reach o7d. Retrying…</div>
  {:else}
    {#if loadState === "offline"}
      <div class="state-banner">Offline — showing last known state</div>
    {/if}

    <section>
      <h2>Running <span class="count">{active.length}</span></h2>
      {#if active.length === 0}
        <p class="empty">Nothing running right now.</p>
      {:else}
        <ul class="run-list">
          {#each active as run (run.run_id)}
            <li>
              <Link to="/runs/{run.run_id}" class="run-card">
                <div class="run-card-top">
                  <StatusBadge status={run.status} />
                  <span class="agent">{run.agent} · {run.role}</span>
                </div>
                <div class="run-card-bottom">
                  <span class="age">{relativeAge(run.created_at)} ago</span>
                  <Link to="/conversations/{run.conversation_id}" class="conv-link"
                    >conversation {run.conversation_id.slice(0, 8)}</Link
                  >
                </div>
              </Link>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h2>Recent</h2>
      {#if terminal.length === 0}
        <p class="empty">No finished runs yet.</p>
      {:else}
        <ul class="run-list">
          {#each terminal as run (run.run_id)}
            <li>
              <Link to="/runs/{run.run_id}" class="run-card">
                <div class="run-card-top">
                  <StatusBadge status={run.status} />
                  <span class="agent">{run.agent} · {run.role}</span>
                </div>
                <div class="run-card-bottom">
                  <span class="age">{relativeAge(run.created_at)} ago</span>
                </div>
              </Link>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

<style>
  .dashboard {
    padding: 0.75rem 0.9rem 2rem;
    max-width: 640px;
    margin: 0 auto;
  }
  .page-header h1 {
    margin: 0;
    font-size: 1.4rem;
  }
  .subtitle {
    margin: 0 0 1rem;
    color: var(--text-muted);
    font-size: 0.85rem;
  }
  h2 {
    font-size: 0.9rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    margin: 1.25rem 0 0.5rem;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .count {
    background: var(--surface-2);
    border-radius: 999px;
    padding: 0.05rem 0.5rem;
    font-size: 0.75rem;
  }
  .run-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  :global(a.run-card) {
    display: block;
    padding: 0.75rem;
    border-radius: 0.6rem;
    border: 1px solid var(--border);
    background: var(--surface-1);
    text-decoration: none;
    color: inherit;
    min-height: 44px;
  }
  .run-card-top {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.35rem;
  }
  .agent {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .run-card-bottom {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.8rem;
    color: var(--text-muted);
  }
  :global(a.conv-link) {
    color: var(--accent);
    text-decoration: none;
  }
  .empty {
    color: var(--text-muted);
    font-size: 0.9rem;
  }
  .state-message {
    padding: 2rem 0;
    text-align: center;
    color: var(--text-muted);
  }
  .state-message.error {
    color: var(--bad);
  }
  .state-banner {
    background: var(--warn-soft);
    color: var(--warn);
    padding: 0.5rem 0.75rem;
    border-radius: 0.5rem;
    font-size: 0.85rem;
    margin-bottom: 0.75rem;
  }
</style>
