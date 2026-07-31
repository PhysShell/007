<script lang="ts">
  // Q-Deck R1 (docs/q-deck/r1-command.md §9.7): one durable command per
  // submission — no cancel/approve/reject, no queue, no workflow designer.
  // The idempotency key is minted once per distinct submission and reused
  // across a retry of the SAME text (so a resend after a transient failure
  // cannot double-invoke the provider); editing the text after a failure
  // mints a fresh key, since that is a genuinely different request.
  import { postCommand } from "../lib/api";

  let { conversationId, parentRunId }: { conversationId: string; parentRunId: string | null } =
    $props();

  let text = $state("");
  let pending = $state(false);
  let error: string | null = $state(null);

  // Deliberately NOT `$state`: these two never need to redraw anything by
  // themselves — they only steer what `send()` does next time it runs.
  let idempotencyKey: string | null = null;
  let keyForText: string | null = null;

  function keyFor(current: string): string {
    if (idempotencyKey === null || keyForText !== current) {
      idempotencyKey = crypto.randomUUID();
      keyForText = current;
    }
    return idempotencyKey;
  }

  async function send(): Promise<void> {
    if (pending || !parentRunId) return;
    const submitted = text;
    if (submitted.trim().length === 0) return;

    const key = keyFor(submitted);
    pending = true;
    error = null;
    try {
      await postCommand(conversationId, {
        parentRunId,
        command: submitted,
        idempotencyKey: key,
      });
      // Cleared ONLY after durable acceptance (202) — a rejection must
      // never discard what the user typed.
      text = "";
      idempotencyKey = null;
      keyForText = null;
    } catch (e) {
      error = e instanceof Error ? e.message : "Failed to send the command.";
    } finally {
      pending = false;
    }
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void send();
    }
  }
</script>

<div class="command-box">
  {#if !parentRunId}
    <p class="command-hint">Commands are available once this conversation has a run.</p>
  {/if}
  <textarea
    bind:value={text}
    onkeydown={onKeydown}
    disabled={pending || !parentRunId}
    placeholder="Send a follow-up command…"
    rows="2"
  ></textarea>
  <button
    type="button"
    onclick={send}
    disabled={pending || !parentRunId || text.trim().length === 0}
  >
    {pending ? "Sending…" : "Send"}
  </button>
  {#if error}
    <p class="command-error" role="alert">{error}</p>
  {/if}
</div>

<style>
  .command-box {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: flex-start;
  }
  .command-hint {
    flex-basis: 100%;
    margin: 0 0 0.25rem;
    color: var(--text-muted);
    font-size: 0.8rem;
  }
  textarea {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 44px;
    resize: vertical;
    padding: 0.5rem 0.6rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: inherit;
    font: inherit;
  }
  button {
    flex-shrink: 0;
    min-height: 44px;
    min-width: 44px;
    padding: 0 1rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .command-error {
    flex-basis: 100%;
    margin: 0;
    color: var(--bad);
    font-size: 0.8rem;
  }
</style>
