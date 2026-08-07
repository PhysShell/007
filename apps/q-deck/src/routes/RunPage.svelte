<script lang="ts">
  import { onDestroy } from "svelte";
  import { getRun } from "../lib/api";
  import type { RunDto } from "../lib/types";
  import { isSealedRun, mergeRunProjection, isFinalCandidateProjection } from "../lib/types";
  import { ConversationEventStream } from "../lib/eventStream.svelte";
  import { relativeAge, duration, absoluteTime } from "../lib/format";
  import Link from "../components/Link.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import ConnectionIndicator from "../components/ConnectionIndicator.svelte";
  import EventTimeline from "../components/EventTimeline.svelte";
  import CandidateStateCard from "../components/CandidateStateCard.svelte";

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
  // Q-Deck A0.5 corrective round 4 (fresh exact-head CodeRabbit Major +
  // Codex P1, PR #110): once sealed with no projection yet, retries
  // continue INDEFINITELY (never a fixed cutoff — the server-side
  // contract makes "exec never configured" indistinguishable from "replay
  // limiter transiently saturated," and there is no bound on how long
  // saturation can last; a fixed budget just turns that uncertainty into
  // a permanently wrong "unavailable"), but at a BACKED-OFF frequency —
  // 5s, 10s, 20s, capped at 30s — rather than hammering an already
  // apparently-busy server forever at the same 5s cadence. Unmounting
  // (navigating away) is what actually stops it, same as every other
  // poll on this page.
  const MAX_POST_SEAL_BACKOFF_MS = 30_000;
  // Q-Deck A0.5 corrective round 7 (fresh exact-head Codex P1, PR #110):
  // round 4 made "at most one request in flight" an invariant by only
  // ever scheduling the next poll after the previous one settles — but an
  // unbounded `fetch` (a server sending headers and then never finishing
  // the body, say) meant that single request could itself never settle,
  // permanently stalling the whole loop. A DEADLINE per request, not
  // merely a `Promise.race` (which would stop WAITING on the fetch
  // without stopping the fetch itself, still violating the one-real-
  // request-in-flight invariant in spirit), via a real `AbortController`,
  // closes this. 15s is generous for a local `o7d` — deliberately NOT
  // tied to `REFRESH_INTERVAL_MS` (5s): one is "how often do we want a
  // fresh sample," the other is "how long may any ONE sample attempt
  // run," and conflating them would make the poll cadence itself a
  // function of an unrelated timeout tuning decision.
  const RUN_POLL_REQUEST_TIMEOUT_MS = 15_000;
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    let cancelled = false;
    let activePollController: AbortController | undefined;
    loadState = "loading";
    run = null;

    // Deliberately NOT `stream?.close()` / `stream = null` / `clearTimeout(timer)`
    // here at the top: reading `stream`/`timer` synchronously in this effect's
    // own body — even just to tear them down — makes them tracked
    // dependencies of this SAME effect. `stream`/`timer` are then written
    // asynchronously below (inside `poll`, after this function has already
    // returned), so that later write would retrigger this very effect,
    // which tears down and reopens the stream/timer again, forever — a
    // real, previously-undetected infinite loop (no test had ever actually
    // rendered this component before Q-Deck R0.5). The effect's own
    // `return () => {...}` cleanup below already closes the PREVIOUS
    // stream/timer at exactly the right time (before this effect reruns for a
    // new `runId`, and on unmount) without reading them reactively here.
    let nextPostSealDelayMs = REFRESH_INTERVAL_MS;

    // Q-Deck A0.5 corrective round 4 (fresh exact-head CodeRabbit Major,
    // PR #110): a single self-rescheduling loop, never `setInterval` —
    // the next `getRun` is only ever scheduled AFTER the previous one has
    // fully resolved and been processed, so at most one request is ever
    // in flight and responses can never complete out of order. The
    // earlier `setInterval`-driven version could have two overlapping
    // `refreshRun` calls in flight at once; if the NEWER one (correctly
    // sealed + materialized) resolved and cleared the interval first, a
    // SLOWER, now-stale OLDER response could still land afterward and
    // silently overwrite that correct state with nothing left running to
    // ever correct it again.
    async function poll(isFirst: boolean): Promise<void> {
      const controller = new AbortController();
      activePollController = controller;
      // Q-Deck A0.5 corrective round 8 (fresh exact-head Codex P1, PR
      // #110): distinguishes WHY the request stopped waiting — aborting
      // the browser's own `fetch` only ever stops the CLIENT side of the
      // connection. `crates/o7d/src/routes.rs`'s own doc comment on
      // `run_behind_replay_limiter` is explicit: a future dropped after
      // the blocking closure has already been spawned has NO effect on
      // that closure or the shared replay-limiter permit it holds — the
      // server-side `candidate_projection` work keeps running to
      // completion regardless of what the client does. See `catch`
      // below for why this flag exists.
      let deadlineExpired = false;
      const deadline = setTimeout(() => {
        deadlineExpired = true;
        controller.abort();
      }, RUN_POLL_REQUEST_TIMEOUT_MS);
      try {
        const r = await getRun(runId, controller.signal);
        if (cancelled) return;
        if (isFirst) {
          run = r;
          loadState = "ready";
          stream = new ConversationEventStream(r.conversation_id);
        } else {
          // Q-Deck A0.5 corrective round 3 (fresh exact-head Codex P1, PR
          // #110): the stop/retry decision must read the FRESH response
          // `r`, never the merged DISPLAY state — `mergeRunProjection`
          // can legitimately preserve an earlier, genuinely-obtained
          // "not_applicable" (e.g. polled between RunStarted and
          // CandidateStateMaterialized being appended) across a LATER
          // poll that omits its own projection because the replay
          // limiter is saturated. Trusting the merged object here would
          // treat that stale "not_applicable" as a trustworthy final
          // sealed answer and stop polling — the page would then
          // permanently under-report a run that later genuinely
          // materialized. Preserving `run` for DISPLAY and deciding
          // freshness from `r` alone are two different questions;
          // conflating them is exactly the bug.
          run = mergeRunProjection(run, r);
        }
        // Q-Deck A0.5 corrective round 6 (fresh exact-head Codex P1, PR
        // #110): a PRESENT projection is not automatically a FINAL one —
        // `"failed"`/`"verification_failed"` can themselves be the
        // server's own transient I/O-error report, not a permanent
        // materialization outcome. See `isFinalCandidateProjection`'s own
        // doc comment for the full reasoning; only `materialized`/
        // `not_applicable` ever stop this loop.
        if (isSealedRun(r.status) && isFinalCandidateProjection(r)) {
          return; // done — a trustworthy final projection has arrived.
        }
        const delay = isSealedRun(r.status)
          ? ((): number => {
              const d = nextPostSealDelayMs;
              nextPostSealDelayMs = Math.min(nextPostSealDelayMs * 2, MAX_POST_SEAL_BACKOFF_MS);
              return d;
            })()
          : REFRESH_INTERVAL_MS;
        timer = setTimeout(() => void poll(false), delay);
      } catch {
        // Q-Deck A0.5 corrective round 5 (fresh exact-head Codex P1, PR
        // #110): cleanup (unmount, or a new `runId`) sets `cancelled` and
        // clears the CURRENT timer, but it cannot cancel an HTTP request
        // already in flight. If that request rejects AFTER cleanup ran,
        // this catch block used to reschedule itself unconditionally —
        // resurrecting a poll loop for a page that no longer exists.
        // During a sustained outage, every such stale rejection would
        // keep rescheduling itself forever; repeated navigation during
        // one outage could accumulate multiple permanent zombie loops.
        // This check must come FIRST, before either branch below.
        if (cancelled) return;
        // Q-Deck A0.5 corrective round 8 (fresh exact-head Codex P1, PR
        // #110): round 7's own deadline unconditionally retried on
        // timeout, exactly as if the timeout were an ordinary transient
        // network failure. It is NOT — the server-side replay work is
        // still genuinely running behind the shared replay-limiter
        // permit (round 7's own finding on `run_behind_replay_limiter`),
        // so dispatching another poll dispatches ANOTHER real replay
        // rather than merely retrying a failed one. A slow-storage run
        // that keeps missing its deadline could accumulate multiple
        // concurrently-running server-side replays from ONE open tab,
        // saturating the same limiter `POST /commands` admission also
        // depends on — degrading a genuinely unrelated code path's
        // availability in the name of this page's own liveness.
        //
        // A client-side timeout is an AMBIGUOUS dispatch outcome — the
        // same principle this system already applies to provider
        // dispatch elsewhere (losing the response never proves the
        // action didn't happen) now applies to this GET too. The
        // deadline still exists so the UI is never stuck awaiting one
        // `Promise` forever, but expiring it must never trigger an
        // automatic redispatch. Preserve whatever was last displayed —
        // never revert an already-loaded run back to "unavailable" —
        // and simply stop polling this run for the rest of this page's
        // lifetime; only a real navigation/reload starts a fresh attempt.
        // Safely retrying a deadline timeout requires server-side
        // cancellation-awareness or replay deduplication, deliberately
        // NOT added here to keep this round frontend-only (disclosed as
        // a future backend consideration in the PR body).
        if (deadlineExpired) {
          if (isFirst) {
            loadState = "error";
          }
          return; // no retry — see the reasoning above.
        }
        // A transient refresh failure doesn't blank an already-loaded run
        // — the next poll tries again at the normal cadence, regardless
        // of whatever post-seal backoff state existed before this
        // failure (a failure to even reach the server isn't evidence
        // about the server's own replay-limiter saturation).
        timer = setTimeout(() => void poll(false), REFRESH_INTERVAL_MS);
      } finally {
        clearTimeout(deadline);
        if (activePollController === controller) {
          activePollController = undefined;
        }
      }
    }

    void poll(true);

    return () => {
      // `cancelled = true` MUST happen before `activePollController?.abort()`
      // — abort() rejects the pending fetch asynchronously, and by the
      // time that rejection reaches `poll`'s own catch block, `cancelled`
      // must already read true there (round 5's own fix) or the aborted
      // request would incorrectly reschedule another poll for a page
      // that's already gone.
      cancelled = true;
      activePollController?.abort();
      stream?.close();
      clearTimeout(timer);
    };
  });

  onDestroy(() => clearTimeout(timer));

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
        {#if run.parent_run_id}
          <!-- Q-Deck A0.5: run lineage — the command-continuation parent this
               run was dispatched from. A SEPARATE concept from candidate-state
               provenance (CandidateStateCard below) — they commonly coincide
               but neither is derived from the other. -->
          <div>
            <dt>parent run</dt>
            <dd><Link to="/runs/{run.parent_run_id}">{run.parent_run_id}</Link></dd>
          </div>
        {/if}
      </dl>
    </header>

    <CandidateStateCard {run} />

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
