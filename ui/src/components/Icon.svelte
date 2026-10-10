<script lang="ts">
  import { ICONS } from '../lib/icons';
  let { name, size = 16 }: { name: string; size?: number } = $props();
</script>

<svg class="icon" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">{@html ICONS[name] ?? ''}</svg>

<style>
  .icon { flex: none; stroke: currentColor; fill: none; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
</style>
