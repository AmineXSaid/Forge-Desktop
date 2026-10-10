<script lang="ts">
  import Icon from './Icon.svelte';
  import { app, MODES, toast } from '../lib/store.svelte';
  import { call, type Profile } from '../lib/ipc';

  const tabs = [
    { id: 'general', label: 'General', icon: 'gear' },
    { id: 'providers', label: 'Providers', icon: 'cpu' },
    { id: 'about', label: 'About', icon: 'info' },
  ];

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

<div class="settings">
  <nav class="nav">
    <button class="back" onclick={() => (app.view = 'chat')}><Icon name="chevronLeft" size={14} />Back</button>
    {#each tabs as t}
      <button class="tab" class:on={app.settingsTab === t.id} onclick={() => { app.settingsTab = t.id; edit = null; }}><Icon name={t.icon} />{t.label}</button>
    {/each}
  </nav>
  <div class="body">
    <div class="inner">
      {#if app.settings}
        {@const s = app.settings}
        {#if app.settingsTab === 'general'}
          <h1>General</h1>
          <p class="lead">How Forge looks, and how new sessions start.</p>
          <h3>Appearance</h3>
          <div class="card group">
            <div class="field">
              <div class="fl"><div class="t">Theme</div><div class="d">Follows Windows unless you pick one.</div></div>
              <div class="seg">
                {#each ['system', 'light', 'dark'] as const as t}
                  <button class:on={s.theme === t} onclick={() => app.saveSettings({ theme: t })}>{t[0].toUpperCase() + t.slice(1)}</button>
                {/each}
              </div>
            </div>
          </div>
          <h3>New sessions</h3>
          <div class="card group">
            <div class="field">
              <div class="fl"><div class="t">Permissions</div><div class="d">{MODES.find((m) => m.id === s.defaultMode)?.desc}</div></div>
              <select class="input" value={s.defaultMode} onchange={(e) => app.saveSettings({ defaultMode: e.currentTarget.value })}>
                {#each MODES as m}<option value={m.id}>{m.label}</option>{/each}
              </select>
            </div>
            <div class="field">
              <div class="fl"><div class="t">Model</div><div class="d">An alias (opus, sonnet, haiku) or a full model name. Empty: Claude Code's default.</div></div>
              <input class="input" placeholder="Default" value={s.defaultModel ?? ''} onchange={(e) => app.saveSettings({ defaultModel: e.currentTarget.value.trim() || null })} />
            </div>
          </div>
        {:else if app.settingsTab === 'providers'}
          <h1>Providers</h1>
          <p class="lead">Where requests go: your Claude account, or any Anthropic-compatible server such as Ollama, LM Studio or a gateway.</p>
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
        {:else}
          <h1>About</h1>
          <p class="lead">Forge Desktop {app.info?.version}, a Windows app for Claude Code.</p>
          <div class="card group">
            <div class="field"><div class="fl"><div class="t">Claude Code</div><div class="d mono">{app.info?.claude ?? 'Not found on PATH'}</div></div></div>
            <div class="field"><div class="fl"><div class="t">Conversations</div><div class="d">Saved by Claude Code under ~/.claude/projects, shared with the terminal.</div></div></div>
            <div class="field"><div class="fl"><div class="t">Shortcuts</div><div class="d">Ctrl+N new session · Ctrl+O open folder · Ctrl+B sidebar · Ctrl+Shift+D changes · Ctrl+, settings · Shift+Tab permissions · Esc stop</div></div></div>
          </div>
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .settings { flex: 1; display: flex; min-height: 0; }
  .nav { width: 220px; flex: none; padding: 16px 12px; display: flex; flex-direction: column; gap: 2px; }
  .back { display: inline-flex; align-items: center; gap: 6px; height: 30px; padding: 0 10px; border-radius: var(--r-button); color: var(--ink-3); font-size: 13px; margin-bottom: 12px; align-self: flex-start; }
  .back:hover { background: var(--hover); color: var(--ink); }
  .tab { display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 12px; border-radius: var(--r-button); color: color-mix(in srgb, var(--ink) 58%, transparent); font-weight: 500; text-align: left; }
  .tab:hover { background: var(--hover); color: var(--ink); }
  .tab.on { background: var(--card); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line); }
  .body { flex: 1; overflow-y: auto; padding: 28px 40px 64px; }
  .inner { max-width: 720px; }
  h1 { margin: 0 0 6px; font-size: 28px; font-weight: 600; letter-spacing: -0.025em; }
  .lead { margin: 0 0 28px; font: 400 18px/1.56 var(--font-serif); color: var(--graphite); }
  h3 { margin: 28px 0 10px; font-size: 14px; font-weight: 600; }
  .group { overflow: hidden; }
  .field { display: flex; align-items: center; gap: 24px; padding: 16px 20px; }
  .field + .field { border-top: 1px solid var(--line); }
  .fl { flex: 1; min-width: 0; }
  .t { font-weight: 500; }
  .d { font-size: 13px; color: var(--ink-3); }
  .mono { font-family: var(--font-mono); font-size: 12px; }
  .seg { display: inline-flex; padding: 3px; gap: 2px; border-radius: var(--r-button); background: var(--hover); }
  .seg button { height: 28px; padding: 0 12px; border-radius: 6px; font-size: 13px; font-weight: 500; color: var(--ink-3); }
  .seg button.on { background: var(--card); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line); }
  select.input, input.input { min-width: 220px; }
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
</style>
