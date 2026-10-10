<script lang="ts">
  import Icon from '../../Icon.svelte';
  import { app, toast } from '../../../lib/store.svelte';
  import { call, type Profile } from '../../../lib/ipc';


  let edit = $state<(Profile & { key: string; isNew: boolean; error: string }) | null>(null);

  function addProfile() {
    edit = { id: `ep-${Date.now().toString(36)}`, name: '', baseUrl: 'http://127.0.0.1:11434', model: '', auth: 'none', hasKey: false, key: '', isNew: true, error: '' };
  }

  async function saveProfile(e: SubmitEvent) {
    e.preventDefault();
    if (!edit || !app.settings) return;
    const { key, isNew, error: _, ...p } = edit;
    const profiles = isNew ? [...app.settings.profiles, p] : app.settings.profiles.map((x) => (x.id === p.id ? p : x));
    try {
      app.settings = await call('settings_save', { settings: { ...app.settings, profiles } });
      if (key.trim()) await call('profile_key', { id: p.id, key });
      app.settings = await call('settings_get');
      edit = null;
      toast('Provider saved');
    } catch (err) {
      if (edit) edit.error = String(err);
    }
  }

  async function remove(p: Profile) {
    if (!app.settings) return;
    await call('profile_key', { id: p.id, key: null }).catch(() => {});
    await app.saveSettings({ profiles: app.settings.profiles.filter((x) => x.id !== p.id), activeProfile: app.settings.activeProfile === p.id ? null : app.settings.activeProfile });
  }
</script>

{#if app.settings}
  {@const s = app.settings}
  <h3>Use for new sessions</h3>
  <div class="card group">
    <button class="prow" class:on={!s.activeProfile} onclick={() => app.saveSettings({ activeProfile: null })}>
      <span class="radio"></span>
      <span class="pt"><span class="pn">Claude account</span><span class="pu">claude login, or ANTHROPIC_API_KEY</span></span>
    </button>
    {#each s.profiles as p (p.id)}
      <div class="prow" class:on={s.activeProfile === p.id}>
        <button class="pick" onclick={() => app.saveSettings({ activeProfile: p.id })}>
          <span class="radio"></span>
          <span class="pt"><span class="pn">{p.name}{#if p.auth !== 'none' && !p.hasKey}<span class="pill error">No key</span>{/if}</span><span class="pu">{p.baseUrl}{p.model ? ` · ${p.model}` : ''}</span></span>
        </button>
        <button class="btn sm" onclick={() => (edit = { ...p, key: '', isNew: false, error: '' })}>Edit</button>
        <button class="icon-btn" title="Remove" onclick={() => remove(p)}><Icon name="trash" size={14} /></button>
      </div>
    {/each}
  </div>
  {#if edit}
    <h3>{edit.isNew ? 'New provider' : `Edit ${edit.name}`}</h3>
    <form class="card form" onsubmit={saveProfile} autocomplete="off">
      <label for="pn">Name</label><input id="pn" class="input" bind:value={edit.name} placeholder="Ollama on this PC" required />
      <label for="pb">Base URL</label><input id="pb" class="input" bind:value={edit.baseUrl} required />
      <label for="pm">Model</label><input id="pm" class="input" bind:value={edit.model} placeholder="qwen3-coder:30b" />
      <label for="pa">Authentication</label>
      <select id="pa" class="input" bind:value={edit.auth}>
        <option value="none">None (local server)</option><option value="apiKey">API key (x-api-key)</option><option value="bearer">Bearer token</option>
      </select>
      {#if edit.auth !== 'none'}
        <label for="pk">Key</label><input id="pk" class="input" type="password" bind:value={edit.key} placeholder={edit.hasKey ? 'Saved in Credential Manager · type to replace' : 'Stored in Windows Credential Manager'} />
      {/if}
      {#if edit.error}<span></span><p class="err">{edit.error}</p>{/if}
      <div class="actions"><button type="button" class="btn" onclick={() => (edit = null)}>Cancel</button><button class="btn primary">Save</button></div>
    </form>
  {:else}
    <button class="btn outline add" onclick={addProfile}><Icon name="plus" size={14} />Add a provider</button>
  {/if}
  <p class="foot">Any Anthropic-compatible server works today (Ollama, LM Studio, a gateway). A provider catalog with OpenAI-style servers and key checks comes with the Forge gateway.</p>
{/if}

<style>
  h3 { margin: 0 0 8px; font-size: 13px; font-weight: 600; color: var(--ink-2); }
  .group { overflow: hidden; }
  .prow { display: flex; align-items: center; gap: 10px; width: 100%; padding: 12px 16px; text-align: left; }
  .prow + .prow { border-top: 1px solid var(--line); }
  .pick { flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; text-align: left; }
  .radio { width: 18px; height: 18px; border-radius: 50%; box-shadow: inset 0 0 0 1.5px var(--ink-5); flex: none; display: grid; place-items: center; }
  .on .radio { box-shadow: inset 0 0 0 1.5px var(--blue); }
  .on .radio::after { content: ''; width: 9px; height: 9px; border-radius: 50%; background: var(--blue); }
  .pt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .pn { font-weight: 500; display: flex; align-items: center; gap: 8px; }
  .pu { font: 12px var(--font-mono); color: var(--ink-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .add { margin-top: 12px; }
  .form { display: grid; grid-template-columns: 140px 1fr; gap: 12px 16px; align-items: center; padding: 20px; }
  .form label { font-size: 14px; color: var(--ink-3); }
  .form .input { width: 100%; }
  .err { margin: 0; color: var(--status-error-fg); font-size: 13px; }
  .actions { grid-column: 1 / -1; display: flex; justify-content: flex-end; gap: 8px; }
  h3 ~ h3 { margin-top: 26px; }
  .foot { font-size: 12px; color: var(--ink-4); margin: 16px 2px 0; }
</style>
