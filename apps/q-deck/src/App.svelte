<script lang="ts">
  import { currentPath, resolve } from "./lib/router.svelte";
  import Dashboard from "./routes/Dashboard.svelte";
  import RunPage from "./routes/RunPage.svelte";
  import ConversationPage from "./routes/ConversationPage.svelte";

  let route = $derived(resolve(currentPath()));

  let isOnline = $state(navigator.onLine);
  $effect(() => {
    const goOnline = () => (isOnline = true);
    const goOffline = () => (isOnline = false);
    window.addEventListener("online", goOnline);
    window.addEventListener("offline", goOffline);
    return () => {
      window.removeEventListener("online", goOnline);
      window.removeEventListener("offline", goOffline);
    };
  });
</script>

{#if !isOnline}
  <div class="offline-banner" role="status">No network connection</div>
{/if}

{#if route.name === "dashboard"}
  <Dashboard />
{:else if route.name === "run"}
  {#key route.runId}
    <RunPage runId={route.runId} />
  {/key}
{:else if route.name === "conversation"}
  {#key route.conversationId}
    <ConversationPage conversationId={route.conversationId} />
  {/key}
{:else}
  <div class="not-found">
    <p>Not found.</p>
    <a href="/">Back to Q-Deck</a>
  </div>
{/if}

<style>
  .offline-banner {
    background: var(--bad);
    color: white;
    text-align: center;
    font-size: 0.8rem;
    padding: 0.3rem;
  }
  .not-found {
    padding: 3rem 1rem;
    text-align: center;
    color: var(--text-muted);
  }
</style>
