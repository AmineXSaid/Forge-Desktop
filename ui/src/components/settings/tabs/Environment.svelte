<script lang="ts">
  import Section from '../Section.svelte';
  import Icon from '../../Icon.svelte';
  import { mergedEnv, scopeLabel } from '../../../lib/ccsettings';
  import { settingsWindow as sw } from '../../../lib/settings.svelte';

  const vars = $derived(mergedEnv(sw.cc));
  const mine = $derived((sw.cc[sw.scope]?.env ?? {}) as Record<string, string>);
  let name = $state('');
  let value = $state('');
  let reveal = $state<Record<string, boolean>>({});
  const secret = (n: string) => /KEY|TOKEN|SECRET|PASSWORD|PASSPHRASE/i.test(n);

  async function add(e: SubmitEvent) {
    e.preventDefault();
    const n = name.trim();
    if (!n) return;
    if (await sw.set(`env.${n}`, value)) { name = ''; value = ''; }
  }
</script>

<Section title="Variables" caption="Every scope's variables are combined; for the same name, Local wins over Workspace over User.">
  {#each vars as v (v.name)}
    {@const here = v.name in mine}
    <div class="var">
      <span class="name">{v.name}</span>
      {#if here}
        <input class="input val" type={secret(v.name) && !reveal[v.name] ? 'password' : 'text'} spellcheck="false" value={mine[v.name]} aria-label="Value of {v.name}"
          onchange={(e) => sw.set(`env.${v.name}`, e.currentTarget.value)} />
      {:else}
        <span class="val ro">{secret(v.name) && !reveal[v.name] ? '••••••••' : v.value}</span>
      {/if}
      {#if secret(v.name)}<button class="icon-btn" title={reveal[v.name] ? 'Hide' : 'Show'} onclick={() => (reveal[v.name] = !reveal[v.name])}><Icon name="eye" size={14} /></button>{/if}
      <span class="from" class:here>{scopeLabel(v.from)}{#if v.shadowed.length}<span class="sh"> · hides {v.shadowed.map(scopeLabel).join(', ')}</span>{/if}</span>
      {#if here}
        <button class="icon-btn" title="Remove from {scopeLabel(sw.scope)}" onclick={() => sw.reset(`env.${v.name}`)}><Icon name="trash" size={14} /></button>
      {:else}
        <button class="btn sm" title="Set a value for {v.name} in {scopeLabel(sw.scope)} settings" onclick={() => sw.set(`env.${v.name}`, v.value)}>Override</button>
      {/if}
    </div>
  {:else}
    <p class="empty">No variables yet.</p>
  {/each}
  <form class="var add" onsubmit={add} autocomplete="off">
    <input class="input name-in" placeholder="NAME" bind:value={name} spellcheck="false" aria-label="Variable name" />
    <input class="input val" placeholder="value" bind:value={value} spellcheck="false" aria-label="Variable value" />
    <button class="btn outline sm" disabled={!name.trim()}>Add to {scopeLabel(sw.scope)}</button>
  </form>
</Section>
<p class="foot">Variables that make tools load other code (PATH, NODE_OPTIONS, LD_PRELOAD and the like) are refused here; set those in the file yourself.</p>

<style>
  .var { display: flex; align-items: center; gap: 10px; padding: 9px 14px 9px 18px; }
  .var + .var { border-top: 1px solid var(--line); }
  .name { width: 220px; flex: none; font: 500 12.5px var(--font-mono); overflow: hidden; text-overflow: ellipsis; user-select: text; }
  .val { flex: 1; min-width: 0; font: 12.5px var(--font-mono); }
  .ro { color: var(--ink-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; user-select: text; }
  .input.val, .name-in { height: 30px; }
  .name-in { width: 220px; flex: none; font: 12.5px var(--font-mono); text-transform: uppercase; }
  .from { font-size: 12px; color: var(--ink-4); width: 190px; text-align: right; flex: none; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .from.here { color: var(--blue); }
  .sh { color: var(--ink-4); }
  .empty { margin: 0; padding: 14px 18px; color: var(--ink-4); font-size: 13px; }
  .foot { font-size: 12px; color: var(--ink-4); margin: 10px 2px 0; }
</style>
