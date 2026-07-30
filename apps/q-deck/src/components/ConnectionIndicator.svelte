<script lang="ts">
  import type { StreamStatus } from "../lib/eventStream.svelte";

  let { status }: { status: StreamStatus } = $props();

  const label: Record<StreamStatus, string> = {
    connecting: "Connecting…",
    open: "Live",
    reconnecting: "Reconnecting…",
    closed: "Offline",
    unsupported: "Update required",
  };
</script>

<span class="conn conn-{status}">
  <span class="dot" aria-hidden="true"></span>
  {label[status]}
</span>

<style>
  .conn {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.8rem;
    color: var(--text-muted);
  }
  .dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--text-muted);
  }
  .conn-open .dot {
    background: var(--good);
    box-shadow: 0 0 0 3px var(--good-soft);
  }
  .conn-connecting .dot,
  .conn-reconnecting .dot {
    background: var(--warn);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .conn-closed .dot,
  .conn-unsupported .dot {
    background: var(--bad);
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }
</style>
