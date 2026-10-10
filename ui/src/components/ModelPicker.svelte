<script lang="ts">
  // The model picker: every connected provider's models in one searchable
  // list, with what each can do. Arrow keys and Enter pick; Esc closes.
  import Icon from './Icon.svelte';
  import { app } from '../lib/store.svelte';
  import type { ModelInfo } from '../lib/ipc';

  let { current, onpick, onclose }: { current: { route: string; model: string | null }; onpick: (route: string, model: string) => void; onclose: () => void } = $props();

  let query = $state('');
  let active = $state(0);
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLDivElement>();
  let panel = $state<HTMLDivElement>();
  // Above the composer when there is room, else below it; never off screen.
  let below = $state(false);
  let maxH = $state(460);
  $effect(() => {
    const r = panel?.parentElement?.getBoundingClientRect();
    if (!r) return;
    // The content area starts under the title bar.
    const top = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--titlebar')) || 40;
    const above = r.top - top - 16, under = innerHeight - r.bottom - 16;
    below = above < 300 && under > above;
    maxH = Math.min(460, below ? under : above);
  });

  const profiles = $derived(app.settings?.profiles ?? []);
  $effect(() => { for (const p of profiles) app.loadModels(p.id); });
  $effect(() => { input?.focus(); });

  const ACCOUNT: ModelInfo[] = [
    { id: 'default', name: 'Default' },
    { id: 'opus', name: 'Opus' },
    { id: 'sonnet', name: 'Sonnet' },
    { id: 'haiku', name: 'Haiku' },
  ];
  const accountModels = $derived(
    app.models?.map((m) => ({ id: m.value, name: m.displayName.replace(/\s*\(.*\)$/, '') })).filter((m) => !m.id.includes('/')) ?? ACCOUNT,
  );

  type Row = { route: string; group: string; model: ModelInfo; isDefault: boolean };
  const rows = $derived.by(() => {
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
    const match = (r: Row) => terms.every((t) => `${r.group} ${r.model.id} ${r.model.name ?? ''}`.toLowerCase().includes(t));
    const all: Row[] = [
      ...accountModels.map((m) => ({ route: '', group: 'Claude account', model: m, isDefault: false })),
      ...profiles.flatMap((p) => {
        const models = app.providerModels[p.id] ?? (p.model ? [{ id: p.model }] : []);
        const withDefault = p.model && !models.some((m) => m.id === p.model) ? [{ id: p.model }, ...models] : models;
        return withDefault.map((m) => ({ route: p.id, group: p.name, model: m, isDefault: m.id === p.model }));
      }),
    ];
    return all.filter(match).slice(0, 400);
  });
  $effect(() => { query; active = 0; });

  const groups = $derived.by(() => {
    const out: { name: string; route: string; rows: { row: Row; i: number }[] }[] = [];
    rows.forEach((row, i) => {
      if (out.at(-1)?.route !== row.route) out.push({ name: row.group, route: row.route, rows: [] });
      out.at(-1)!.rows.push({ row, i });
    });
    return out;
  });

  function key(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') { e.preventDefault(); active = Math.min(rows.length - 1, active + 1); scroll(); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); active = Math.max(0, active - 1); scroll(); }
    else if (e.key === 'Enter') { e.preventDefault(); const r = rows[active]; if (r) onpick(r.route, r.model.id); }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onclose(); }
  }
  function scroll() { queueMicrotask(() => list?.querySelector('.on')?.scrollIntoView({ block: 'nearest' })); }

  const k = (n: number) => (n >= 1_000_000 ? `${Math.round(n / 100_000) / 10}M` : `${Math.round(n / 1000)}k`);
  const price = (m: ModelInfo) => (m.inputPrice != null && m.outputPrice != null ? `$${m.inputPrice}/$${m.outputPrice}` : '');
  const loading = $derived(profiles.some((p) => !app.providerModels[p.id]));
</script>

<svelte:window onmousedown={(e) => { if (!(e.target as HTMLElement).closest('.picker-panel, .model-pill')) onclose(); }} />

<div class="picker-panel" class:below bind:this={panel} style:max-height="{maxH}px" role="dialog" aria-label="Choose a model">
  <label class="search">
    <Icon name="search" size={14} />
    <input bind:this={input} bind:value={query} placeholder="Search models" spellcheck="false" onkeydown={key} />
    {#if loading}<span class="spinner"></span>{/if}
  </label>
  <div class="list" bind:this={list}>
    {#each groups as g (g.route)}
      <div class="gh">{g.name}{#if g.route}<button class="refresh" title="Ask {g.name} for its models again" onclick={() => app.loadModels(g.route, true)}><Icon name="refresh" size={12} /></button>{/if}</div>
      {#each g.rows as { row, i } (row.route + row.model.id)}
        <button class="opt" class:on={i === active} onmouseenter={() => (active = i)} onclick={() => onpick(row.route, row.model.id)}>
          <span class="name">{row.model.name ?? row.model.id}{#if row.model.name && row.route}<span class="id">{row.model.id}</span>{/if}</span>
          <span class="badges">
            {#if row.isDefault}<span class="b def">default</span>{/if}
            {#if row.model.tools === false}<span class="b warn" title="It can't call tools: Claude Code can only chat with it">chat only</span>{/if}
            {#if row.model.reasoning}<span class="b">reasoning</span>{/if}
            {#if row.model.vision}<span class="b">vision</span>{/if}
            {#if row.model.context}<span class="b">{k(row.model.context)}</span>{/if}
            {#if price(row.model)}<span class="b price">{price(row.model)}</span>{/if}
          </span>
          {#if current.route === row.route && current.model === row.model.id}<span class="check"><Icon name="check" size={14} /></span>{/if}
        </button>
      {/each}
    {:else}
      <p class="none">{loading ? 'Asking the providers…' : 'No model matches.'}</p>
    {/each}
  </div>
  <div class="foot"><kbd>↑↓</kbd> choose <kbd>Enter</kbd> pick <kbd>Esc</kbd> close <span class="grow"></span>Prices per million tokens, in/out</div>
</div>

<style>
  .picker-panel {
    position: absolute; left: 0; bottom: calc(100% + 8px); width: min(520px, 100%); z-index: 40;
    background: var(--card); border-radius: var(--r-card); box-shadow: var(--float); display: flex; flex-direction: column;
    animation: rise 120ms var(--ease);
  }
  .picker-panel.below { bottom: auto; top: calc(100% + 8px); }
  .search { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--line); color: var(--ink-4); }
  .search input { flex: 1; border: 0; outline: 0; background: none; font-size: 14px; color: var(--ink); user-select: text; }
  .list { overflow-y: auto; padding: 4px 6px 6px; }
  .gh { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 500; color: var(--ink-4); padding: 10px 10px 4px; }
  .refresh { display: grid; place-items: center; color: var(--ink-4); }
  .refresh:hover { color: var(--ink); }
  .opt { display: flex; align-items: center; gap: 10px; width: 100%; padding: 7px 10px; border-radius: var(--r-button); text-align: left; }
  .opt.on { background: var(--hover); }
  .name { flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 8px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 14px; }
  .id { font: 11.5px var(--font-mono); color: var(--ink-4); overflow: hidden; text-overflow: ellipsis; }
  .badges { display: flex; gap: 4px; flex: none; }
  .b { font-size: 11px; font-weight: 500; height: 18px; padding: 0 6px; border-radius: var(--r-pill); display: inline-flex; align-items: center; background: var(--hover); color: var(--ink-3); }
  .b.def { background: var(--sky-tint); color: var(--blue); }
  .b.warn { background: var(--status-needs-bg); color: var(--status-needs-fg); }
  .b.price { font-family: var(--font-mono); font-weight: 400; }
  .check { color: var(--blue); display: grid; }
  .none { margin: 0; padding: 14px; color: var(--ink-4); font-size: 13px; }
  .foot { display: flex; align-items: center; gap: 6px; padding: 7px 14px; border-top: 1px solid var(--line); font-size: 11.5px; color: var(--ink-4); }
  .foot .grow { flex: 1; }
</style>
