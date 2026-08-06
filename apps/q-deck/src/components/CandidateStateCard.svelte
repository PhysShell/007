<script lang="ts">
  // Q-Deck A0.5 (docs/q-deck/a0-candidate-state.md §9): a read-only
  // projection of the three candidate-state fields RunDto already carries
  // (candidate_source_run_id, candidate_tree_oid, materialization_status) —
  // nothing here is computed client-side; every value shown is exactly what
  // the server sent, or an honest "unavailable"/unknown fallback when it
  // didn't. Deliberately NOT the same concept as run lineage
  // (parent_run_id, shown separately in RunPage's own metadata list):
  // candidate-state PROVENANCE and run PARENTAGE commonly coincide but the
  // backend makes no promise they always do, so this component never reads
  // or infers anything from `run.parent_run_id`.
  import type { RunDto, MaterializationStatus } from "../lib/types";
  import Link from "./Link.svelte";

  let { run }: { run: RunDto } = $props();

  // Exhaustive over the four values `candidate_projection` currently
  // produces, PLUS a safe fallback for both "no value at all" (no `exec`
  // configured server-side — a deployment condition, not a per-run one) and
  // "a value the frontend doesn't recognize yet" (a future fifth status
  // must render honestly, never crash or silently look like one of the
  // four known ones).
  function materializationLabel(status: MaterializationStatus | null | undefined): string {
    switch (status) {
      case undefined:
      case null:
        return "unavailable";
      case "materialized":
        return "materialized";
      case "not_applicable":
        return "not applicable";
      case "failed":
        return "failed";
      case "verification_failed":
        return "verification failed";
      default:
        // An unrecognized value from a newer server build — show it
        // verbatim rather than hiding or misclassifying it.
        return status;
    }
  }

  function materializationTone(status: MaterializationStatus | null | undefined): string {
    switch (status) {
      case undefined:
      case null:
      case "not_applicable":
        return "tone-neutral";
      case "materialized":
        return "tone-good";
      case "failed":
      case "verification_failed":
        return "tone-bad";
      default:
        // Unknown value: never claim good or bad about something this
        // build doesn't understand — flag it distinctly instead.
        return "tone-warn";
    }
  }

  let label = $derived(materializationLabel(run.materialization_status));
  let tone = $derived(materializationTone(run.materialization_status));
</script>

<section class="candidate-card">
  <div class="section-header">
    <h2>Candidate state</h2>
    <span class="badge {tone}">{label}</span>
  </div>

  {#if run.candidate_source_run_id || run.candidate_tree_oid}
    <dl class="meta">
      {#if run.candidate_source_run_id}
        <div>
          <dt>source run</dt>
          <dd><Link to="/runs/{run.candidate_source_run_id}">{run.candidate_source_run_id}</Link></dd>
        </div>
      {/if}
      {#if run.candidate_tree_oid}
        <div>
          <dt>tree OID</dt>
          <dd class="oid" title={run.candidate_tree_oid}>{run.candidate_tree_oid}</dd>
        </div>
      {/if}
    </dl>
  {:else}
    <p class="empty">No candidate-state data for this run.</p>
  {/if}
</section>

<style>
  .candidate-card {
    margin-top: 1.25rem;
  }
  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    margin: 0 0 0.5rem;
  }
  .section-header h2 {
    font-size: 0.9rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    margin: 0;
  }
  .badge {
    display: inline-block;
    padding: 0.15rem 0.55rem;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 600;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .tone-neutral {
    background: var(--surface-2);
    color: var(--text-muted);
  }
  .tone-good {
    background: var(--good-soft);
    color: var(--good);
  }
  .tone-bad {
    background: var(--bad-soft);
    color: var(--bad);
  }
  .tone-warn {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .meta {
    margin: 0;
    font-size: 0.85rem;
    color: var(--text-muted);
  }
  .meta > div {
    display: flex;
    gap: 0.4rem;
    padding: 0.15rem 0;
    /* Long tree OIDs (see .oid below) must never force this row wider than
       its container on a narrow viewport — this is what actually keeps the
       mobile layout intact, not the truncation alone. */
    min-width: 0;
  }
  .meta dt {
    flex-shrink: 0;
    width: 5.5rem;
  }
  .meta dd {
    margin: 0;
    min-width: 0;
  }
  /* Shortened VISUALLY only: the full OID stays in the DOM as real text
     (never JS-truncated with an ellipsis substring), so selecting/copying
     this element always yields the complete value — `text-overflow:
     ellipsis` is a paint-time effect, not a content change. `title` above
     also surfaces the full value on hover for sighted desktop users. */
  .oid {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 0.8rem;
  }
  .empty {
    margin: 0;
    font-size: 0.85rem;
    color: var(--text-muted);
  }
</style>
