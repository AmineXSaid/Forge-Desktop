<script lang="ts">
  import { toasts } from '../lib/store.svelte';
</script>

<div class="toasts" aria-live="polite">
  {#each toasts as t (t.id)}<div class="toast" class:error={t.tone === 'error'}>{t.text}</div>{/each}
</div>

<style>
  .toasts { position: fixed; right: 18px; bottom: 18px; display: flex; flex-direction: column; gap: 8px; z-index: 200; }
  .toast { background: var(--card); color: var(--ink); box-shadow: var(--float); border-radius: var(--r-card); padding: 10px 14px; font-size: 14px; max-width: 380px; animation: rise 160ms var(--ease); }
  .toast.error { color: var(--status-error-fg); }
</style>
