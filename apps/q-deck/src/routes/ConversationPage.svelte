<script lang="ts">
  import { listRuns } from "../lib/api";
  import type { RunDto } from "../lib/types";
  import { ConversationEventStream } from "../lib/eventStream.svelte";
  import { relativeAge } from "../lib/format";
  import Link from "../components/Link.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import ConnectionIndicator from "../components/ConnectionIndicator.svelte";
  import EventTimeline from "../components/EventTimeline.svelte";

  let { conversationId }: { conversationId: string } = $props();

  type LoadState = "loading" | "ready" | "error";

  let runs: RunDto[] = $state([]);
  let loadState: LoadState = $state("loading");
  let stream: ConversationEventStream | null = $state(null);

  $effect(() => {
    let cancelled = false;
    loadState = "loading";
    runs = [];
    stream?.close();
    stream = new ConversationEventStream(conversationId);

    listRuns({ conversationId, limit: 200 })
      .then((page) => {
        if (cancelled) return;
        runs = [...page.items].sort((a, b) => a.created_at - b.created_at);
        loadState = "ready";
      })
      .catch(() => {
        if (!cancelled) loadState = "error";
      });

    return () => {
      cancelled = true;
      stream?.close();
    };
  });
</script>

<div class="conv-page">
  <Link to="/" class="back">&larr; Runs</Link>

  <header class="conv-header">
    <h1>Conversation</h1>
    <p class="conv-id">{conversationId}</p>
  </header>

  <section>
    <h2>Runs</h2>
    {#if loadState === "loading"}
      <div class="state-message">Loading…</div>
    {:else if loadState === "error"}
      <div class="state-message error">Couldn't load runs for this conversation.</div>
    {:else if runs.length === 0}
      <p class="empty">No runs recorded for this conversation.</p>
    {:else}
      <ul class="run-list">
        {#each runs as run (run.run_id)}
          <li>
            <Link to="/runs/{run.run_id}" class="run-row">
              <StatusBadge status={run.status} />
              <span class="agent">{run.agent} · {run.role}</span>
              <span class="age">{relativeAge(run.created_at)} ago</span>
            </Link>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section>
    <div class="section-header">
      <h2>Events</h2>
      {#if stream}<ConnectionIndicator status={stream.status} />{/if}
    </div>
    <EventTimeline events={stream ? stream.events : []} />
  </section>
</div>

<style>
  .conv-page {
    padding: 0.75rem 0.9rem 2rem;
    max-width: 640px;
    margin: 0 auto;
  }
  :global(a.back) {
    display: inline-block;
    color: var(--text-muted);
    text-decoration: none;
    font-size: 0.85rem;
    margin-bottom: 0.75rem;
    min-height: 44px;
    line-height: 44px;
  }
  .conv-header h1 {
    font-size: 1.15rem;
    margin: 0;
  }
  .conv-id {
    margin: 0.15rem 0 0;
    color: var(--text-muted);
    font-size: 0.8rem;
    overflow-wrap: anywhere;
  }
  h2 {
    font-size: 0.9rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    margin: 1.25rem 0 0.5rem;
  }
  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: 1.25rem 0 0.5rem;
  }
  .section-header h2 {
    margin: 0;
  }
  .run-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  :global(a.run-row) {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.6rem 0.75rem;
    min-height: 44px;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background: var(--surface-1);
    text-decoration: none;
    color: inherit;
  }
  .agent {
    font-weight: 600;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .age {
    font-size: 0.8rem;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .empty {
    color: var(--text-muted);
    font-size: 0.9rem;
  }
  .state-message {
    padding: 1rem 0;
    text-align: center;
    color: var(--text-muted);
  }
  .state-message.error {
    color: var(--bad);
  }
</style>
