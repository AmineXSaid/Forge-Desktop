<script lang="ts">
  import type { Snippet } from 'svelte';
  let { title, caption = '', children }: { title: string; caption?: string; children: Snippet } = $props();
</script>

<section>
  <h3>{title}</h3>
  {#if caption}<p class="cap">{caption}</p>{/if}
  <div class="card group">{@render children()}</div>
</section>

<style>
  section { margin-top: 26px; }
  section:first-child { margin-top: 0; }
  h3 { margin: 0 0 8px; font-size: 13px; font-weight: 600; color: var(--ink-2); }
  .cap { margin: -4px 0 10px; font-size: 13px; color: var(--ink-3); }
  .group { overflow: hidden; }
</style>
