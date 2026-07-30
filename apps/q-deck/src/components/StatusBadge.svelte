<script lang="ts">
  import type { RunStatus, ConversationStatus } from "../lib/types";

  let { status }: { status: RunStatus | ConversationStatus } = $props();

  const tone: Record<string, string> = {
    queued: "tone-neutral",
    running: "tone-active",
    completed: "tone-good",
    failed: "tone-bad",
    cancelled: "tone-neutral",
    interrupted: "tone-warn",
    open: "tone-active",
    closed: "tone-neutral",
  };
</script>

<span class="badge {tone[status] ?? 'tone-neutral'}">{status}</span>

<style>
  .badge {
    display: inline-block;
    padding: 0.15rem 0.55rem;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.02em;
    line-height: 1.4;
    white-space: nowrap;
  }
  .tone-neutral {
    background: var(--surface-2);
    color: var(--text-muted);
  }
  .tone-active {
    background: var(--accent-soft);
    color: var(--accent);
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
</style>
