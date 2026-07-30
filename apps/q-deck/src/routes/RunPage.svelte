<script lang="ts">
  import { onDestroy } from "svelte";
  import { getRun } from "../lib/api";
  import type { RunDto } from "../lib/types";
  import { isSealedRun } from "../lib/types";
  import { ConversationEventStream } from "../lib/eventStream.svelte";
  import { relativeAge, duration, absoluteTime } from "../lib/format";
  import Link from "../components/Link.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import ConnectionIndicator from "../components/ConnectionIndicator.svelte";
  import EventTimeline from "../components/EventTimeline.svelte";

  let { runId }: { runId: string } = $props();

  type LoadState = "loading" | "ready" | "error";

  let run: RunDto | null = $state(null);
  let loadState: LoadState = $state("loading");
  let stream: ConversationEventStream | null = $state(null);

  // The SSE stream only ever adds to the event timeline — it carries no
  // mechanism for updating the run's OWN fields (status, finished_at). A
  // lifecycle transition (running -> completed, say) would otherwise leave
  // this page showing "running" forever even while a live connection is
  // displayed, which is exactly the kind of stale-presented-as-fresh state
  // this app's honesty rule elsewhere forbids. Poll the run itself, same
  // interval as the dashboard, stopping only once it's SEALED
  // (completed/failed/cancelled) — never on `interrupted`, which is a
  // resumable pause in `o7-ledger` (`resume_interrupted_run`), not a fixed
  // verdict; stopping on it would mean this page never notices a
  // subsequent resume back to `running`.
  const REFRESH_INTERVAL_MS = 5000;
  let timer: ReturnType<typeof setInterval> | undefined;

  $effect(() => {
    let cancelled = false;
    loadState = "loading";
    run = null;

    // Deliberately NOT `stream?.close()` / `stream = null` / `clearInterval(timer)`
    // here at the top: reading `stream`/`timer` synchronously in this effect's
    // own body — even just to tear them down — makes them tracked
    // dependencies of this SAME effect. `stream`/`timer` are then written
    // asynchronously below (inside `getRun(...).then(...)`, after this
    // function has already returned), so that later write would retrigger
    // this very effect, which tears down and reopens the stream/timer again,
    // forever — a real, previously-undetected infinite loop (no test had ever
    // actually rendered this component before Q-Deck R0.5). The effect's own
    // `return () => {...}` cleanup below already closes the PREVIOUS
    // stream/timer at exactly the right time (before this effect reruns for a
    // new `runId`, and on unmount) without reading them reactively here.
    async function refreshRun(): Promise<void> {
      try {
        const r = await getRun(runId);
        if (cancelled) return;
        run = r;
        if (isSealedRun(r.status)) {
          clearInterval(timer);
        }
      } catch {
        // A transient refresh failure doesn't blank an already-loaded run —
        // the next poll tries again; only the FIRST load surfaces an error.
      }
    }

    getRun(runId)
      .then((r) => {
        if (cancelled) return;
        run = r;
        loadState = "ready";
        stream = new ConversationEventStream(r.conversation_id);
        if (!isSealedRun(r.status)) {
          timer = setInterval(() => void refreshRun(), REFRESH_INTERVAL_MS);
        }
      })
      .catch(() => {
        if (!cancelled) loadState = "error";
      });

    return () => {
      cancelled = true;
      stream?.close();
      clearInterval(timer);
    };
  });

  onDestroy(() => clearInterval(timer));

  // Only this run's own events — the conversation may hold other runs' too.
  let runEvents = $derived.by(() => {
    const s = stream;
    return s ? s.events.filter((e) => e.run_id === runId) : [];
  });
</script>

<div class="run-page">
  <Link to="/" class="back">&larr; Runs</Link>

  {#if loadState === "loading"}
    <div class="state-message">Loading run…</div>
  {:else if loadState === "error"}
    <div class="state-message error">Run not found or o7d unreachable.</div>
  {:else if run}
    <header class="run-header">
      <div class="run-title">
        <StatusBadge status={run.status} />
        <h1>{run.agent} · {run.role}</h1>
      </div>
      <dl class="meta">
        <div><dt>started</dt><dd>{absoluteTime(run.created_at)} ({relativeAge(run.created_at)} ago)</dd></div>
        {#if run.finished_at}
          <div><dt>duration</dt><dd>{duration(run.created_at, run.finished_at)}</dd></div>
        {/if}
        <div>
          <dt>conversation</dt>
          <dd><Link to="/conversations/{run.conversation_id}">{run.conversation_id}</Link></dd>
        </div>
      </dl>
    </header>

    <section>
      <div class="section-header">
        <h2>Timeline</h2>
        {#if stream}<ConnectionIndicator status={stream.status} />{/if}
      </div>
      <EventTimeline events={runEvents} />
    </section>
  {/if}
</div>

<style>
  .run-page {
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
  .run-title {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .run-title h1 {
    font-size: 1.15rem;
    margin: 0;
    overflow-wrap: anywhere;
  }
  .meta {
    margin: 0.75rem 0 0;
    font-size: 0.85rem;
    color: var(--text-muted);
  }
  .meta > div {
    display: flex;
    gap: 0.4rem;
    padding: 0.15rem 0;
  }
  .meta dt {
    flex-shrink: 0;
    width: 5.5rem;
  }
  .meta dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: 1.25rem 0 0.5rem;
  }
  .section-header h2 {
    font-size: 0.9rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    margin: 0;
  }
  .state-message {
    padding: 2rem 0;
    text-align: center;
    color: var(--text-muted);
  }
  .state-message.error {
    color: var(--bad);
  }
</style>
