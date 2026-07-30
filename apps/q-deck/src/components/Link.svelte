<script lang="ts">
  import { navigate } from "../lib/router.svelte";
  import type { Snippet } from "svelte";

  let {
    to,
    class: className = "",
    children,
  }: { to: string; class?: string; children?: Snippet } = $props();

  function onClick(event: MouseEvent): void {
    // A modifier click (open in new tab, middle-click, etc.) must behave like
    // a normal link — only a plain left click gets intercepted into the SPA
    // router.
    if (event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    navigate(to);
  }
</script>

<a href={to} class={className} onclick={onClick}>
  {@render children?.()}
</a>
