<script lang="ts">
  // A Claude Code setting bound to a dotted key in the chosen scope. Shows the
  // effective value (local > workspace > user) when the scope doesn't set it.
  import Row from './Row.svelte';
  import Toggle from './Toggle.svelte';
  import Icon from '../Icon.svelte';
  import { effective, getPath, mergedList, scopeLabel } from '../../lib/ccsettings';
  import { settingsWindow as sw } from '../../lib/settings.svelte';

  type Opt = { value: string; label: string };
  let {
    key, label, desc = '', type = 'text', placeholder = '', options = [], fallback = undefined,
    mono = false, min = undefined, max = undefined, secret = false,
  }: {
    key: string; label: string; desc?: string;
    /** `env` is a string value under `env.NAME`; `list` is merged across scopes. */
    type?: 'text' | 'bool' | 'choice' | 'number' | 'list' | 'env';
    placeholder?: string; options?: Opt[]; fallback?: unknown; mono?: boolean; min?: number; max?: number; secret?: boolean;
  } = $props();

  const here = $derived(getPath(sw.cc[sw.scope], key));
  const shown = $derived(here ?? effective(sw.cc, key).value ?? fallback);
  const others = $derived(type === 'list' ? mergedList(sw.cc, key).filter((x) => x.from !== sw.scope) : []);
  const mine = $derived(Array.isArray(here) ? (here as string[]) : []);
  let draft = $state('');

  function text(v: string) {
    const t = v.trim();
    sw.set(key, t === '' ? null : t);
  }
  function num(v: string) {
    if (v.trim() === '') return sw.set(key, null);
    const n = Number(v);
    if (!Number.isInteger(n)) return;
    sw.set(key, n);
  }
  async function add() {
    const t = draft.trim();
    if (!t || mine.includes(t)) return;
    if (await sw.set(key, [...mine, t])) draft = '';
  }
  function remove(item: string) {
    const rest = mine.filter((x) => x !== item);
    sw.set(key, rest.length ? rest : null);
  }
</script>

<Row {label} {desc} {key} stacked={type === 'list'}>
  {#if type === 'bool'}
    <Toggle checked={shown === true} {label} onchange={(v) => sw.set(key, v)} />
  {:else if type === 'choice'}
    <select class="input" value={(shown as string) ?? ''} onchange={(e) => sw.set(key, e.currentTarget.value || null)} aria-label={label}>
      {#if fallback === undefined}<option value="">Not set</option>{/if}
      {#each options as o}<option value={o.value}>{o.label}</option>{/each}
    </select>
  {:else if type === 'number'}
    <input class="input num" type="number" {min} {max} {placeholder} aria-label={label}
      value={here !== undefined ? here : ''} onchange={(e) => num(e.currentTarget.value)} />
  {:else if type === 'list'}
    <div class="chips">
      {#each mine as item (item)}
        <span class="chip mine"><span class="v">{item}</span><button title="Remove" aria-label="Remove {item}" onclick={() => remove(item)}><Icon name="x" size={12} /></button></span>
      {/each}
      {#each others as o (o.from + o.item)}
        <span class="chip" title="From {scopeLabel(o.from)} settings; edit it there"><span class="v">{o.item}</span><span class="from">{scopeLabel(o.from)}</span></span>
      {/each}
      {#if !mine.length && !others.length}<span class="none">None</span>{/if}
    </div>
    <form class="add" onsubmit={(e) => { e.preventDefault(); add(); }}>
      <input class="input mono" bind:value={draft} {placeholder} aria-label="Add to {label}" />
      <button class="btn outline sm" disabled={!draft.trim()}>Add to {scopeLabel(sw.scope)}</button>
    </form>
  {:else}
    <input class="input" class:mono class:inherit={here === undefined && shown !== undefined} type={secret ? 'password' : 'text'} spellcheck="false" aria-label={label}
      placeholder={here === undefined && shown !== undefined ? String(shown) : placeholder}
      value={here !== undefined ? String(here) : ''} onchange={(e) => text(e.currentTarget.value)} />
  {/if}
</Row>

<style>
  .input { width: 260px; }
  .num { width: 120px; }
  .mono { font-family: var(--font-mono); font-size: 13px; }
  select.input { width: 200px; }
  .inherit::placeholder { color: var(--ink-2); }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 4px 0 10px; border-radius: var(--r-small); background: var(--inset); box-shadow: inset 0 0 0 1px var(--line); font: 12.5px var(--font-mono); max-width: 100%; }
  .chip .v { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip button { display: grid; place-items: center; width: 20px; height: 20px; border-radius: var(--r-small); color: var(--ink-3); }
  .chip button:hover { background: var(--hover); color: var(--ink); }
  .chip:not(.mine) { color: var(--ink-3); padding-right: 6px; }
  .from { font: 500 11px var(--font-sans); color: var(--ink-4); }
  .none { font-size: 13px; color: var(--ink-4); }
  .add { display: flex; gap: 8px; }
  .add .input { flex: 1; width: auto; }
</style>
