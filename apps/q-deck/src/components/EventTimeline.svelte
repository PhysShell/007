<script lang="ts">
  import type { EventDto } from "../lib/types";
  import { absoluteTime } from "../lib/format";

  let { events }: { events: EventDto[] } = $props();

  let expanded = $state(new Set<string>());

  function toggle(eventId: string): void {
    const next = new Set(expanded);
    if (next.has(eventId)) next.delete(eventId);
    else next.add(eventId);
    expanded = next;
  }
</script>

<ol class="timeline">
  {#each events as event (event.event_id)}
    <li class="row">
      <button
        class="row-header"
        onclick={() => toggle(event.event_id)}
        aria-expanded={expanded.has(event.event_id)}
      >
        <span class="seq">#{event.sequence}</span>
        <span class="type">{event.event_type}</span>
        <span class="time">{absoluteTime(event.created_at)}</span>
      </button>
      {#if expanded.has(event.event_id)}
        <pre class="payload"><code>{JSON.stringify(event.payload, null, 2)}</code></pre>
      {/if}
    </li>
  {:else}
    <li class="empty">No events yet.</li>
  {/each}
</ol>

<style>
  .timeline {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .row {
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    overflow: hidden;
  }
  .row-header {
    width: 100%;
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    padding: 0.65rem 0.75rem;
    min-height: 44px;
    background: var(--surface-1);
    border: none;
    text-align: left;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .seq {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }
  .type {
    font-weight: 600;
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .time {
    color: var(--text-muted);
    font-size: 0.8rem;
    flex-shrink: 0;
  }
  .payload {
    margin: 0;
    padding: 0.75rem;
    background: var(--surface-0);
    border-top: 1px solid var(--border);
    overflow-x: auto;
    font-size: 0.8rem;
    max-height: 40vh;
    overflow-y: auto;
  }
  .empty {
    color: var(--text-muted);
    padding: 1rem 0;
    text-align: center;
  }
</style>
