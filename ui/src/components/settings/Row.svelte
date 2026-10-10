<script lang="ts">
  // One setting: label and description on the left, the control on the right,
  // and for a Claude Code key, where its value comes from and a reset.
  import type { Snippet } from 'svelte';
  import Icon from '../Icon.svelte';
  import { effective, getPath, overriddenBy, scopeLabel } from '../../lib/ccsettings';
  import { settingsWindow as sw } from '../../lib/settings.svelte';

  let {
    label, desc = '', key = null, stacked = false, children,
  }: { label: string; desc?: string; key?: string | null; stacked?: boolean; children: Snippet } = $props();

  const here = $derived(key ? getPath(sw.cc[sw.scope], key) : undefined);
  const eff = $derived(key ? effective(sw.cc, key) : null);
  const over = $derived(key ? overriddenBy(sw.cc, key, sw.scope) : null);
  const q = $derived(sw.query.trim().toLowerCase());
  const hit = $derived(q.length > 1 && (label.toLowerCase().includes(q) || desc.toLowerCase().includes(q)));
</script>

<div class="row" class:stacked class:hit>
  <div class="text">
    <div class="label">
      {label}
      {#if key}
        {#if here !== undefined}
          <span class="src set" title="Written in {scopeLabel(sw.scope)} settings">Set in {scopeLabel(sw.scope)}</span>
          <button class="reset" title="Reset: remove it from {scopeLabel(sw.scope)} settings" onclick={() => sw.reset(key!)}><Icon name="reset" size={12} />Reset</button>
        {:else if eff?.from}
          <span class="src" title="Not set in {scopeLabel(sw.scope)}; this value comes from {scopeLabel(eff.from)} settings">From {scopeLabel(eff.from)}</span>
        {:else}
          <span class="src quiet">Default</span>
        {/if}
      {/if}
    </div>
    {#if desc}<div class="desc">{desc}</div>{/if}
    {#if over}
      <div class="warn"><Icon name="alert" size={12} />{here !== undefined
        ? `${scopeLabel(over)} settings also set this, and win over ${scopeLabel(sw.scope)}.`
        : `${scopeLabel(over)} settings set this, so a value in ${scopeLabel(sw.scope)} would not apply.`}</div>
    {/if}
  </div>
  <div class="control">{@render children()}</div>
</div>

<style>
  .row { display: flex; align-items: center; gap: 24px; padding: 14px 18px; transition: background var(--t) var(--ease); }
  .row:not(:first-child) { border-top: 1px solid var(--line); }
  .row.stacked { flex-direction: column; align-items: stretch; gap: 10px; }
  .row.hit { background: var(--sky-tint); }
  .text { flex: 1; min-width: 0; }
  .label { font-weight: 500; display: flex; align-items: center; flex-wrap: wrap; gap: 8px; }
  .desc { font-size: 13px; color: var(--ink-3); margin-top: 2px; }
  .src { font-size: 11px; font-weight: 500; height: 18px; padding: 0 7px; border-radius: var(--r-pill); display: inline-flex; align-items: center; background: var(--hover); color: var(--ink-3); }
  .src.set { background: var(--sky-tint); color: var(--blue); }
  .src.quiet { color: var(--ink-4); background: none; padding: 0; }
  .reset { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--ink-3); height: 20px; padding: 0 6px; border-radius: var(--r-small); }
  .reset:hover { background: var(--hover); color: var(--ink); }
  .warn { display: flex; align-items: center; gap: 5px; margin-top: 4px; font-size: 12px; color: var(--status-needs-fg); }
  .control { flex: none; display: flex; align-items: center; gap: 8px; }
  .stacked .control { flex-direction: column; align-items: stretch; }
</style>
