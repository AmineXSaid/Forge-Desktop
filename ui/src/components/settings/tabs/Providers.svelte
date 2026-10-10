<script lang="ts">
  // Providers: the Claude account, or any provider through the Forge gateway.
  // Connect: pick a tile (or a server found on this PC), paste a key, Check
  // (Forge lists the models with it), pick the models, Save. Keys go to the
  // keychain; the CLI never sees them.
  import Icon from '../../Icon.svelte';
  import Toggle from '../Toggle.svelte';
  import { app, toast } from '../../../lib/store.svelte';
  import { call, type Detected, type ModelInfo, type Preset, type Profile } from '../../../lib/ipc';

  type Draft = Profile & { key: string; isNew: boolean; error: string; checking: boolean; models: ModelInfo[] | null; advanced: boolean; keyUrl?: string };

  let presets = $state<Preset[]>([]);
  let detected = $state<Detected[] | null>(null);
  let edit = $state<Draft | null>(null);

  // Once per visit: the catalog, and a look for local servers (not polled).
  $effect(() => {
    call('provider_catalog').then((p) => (presets = p)).catch((e) => toast(String(e), 'error'));
    call('providers_detect').then((d) => (detected = d)).catch(() => (detected = []));
  });

  const connected = $derived(app.settings?.profiles ?? []);
  const found = $derived((detected ?? []).filter((d) => !connected.some((p) => p.baseUrl.replace(/\/$/, '') === d.baseUrl.replace(/\/$/, ''))));
  const tiles = $derived(presets.filter((p) => p.kind !== 'local'));
  const localTiles = $derived(presets.filter((p) => p.kind === 'local'));

  function freeId(base: string) {
    const ids = new Set(connected.map((p) => p.id));
    const root = base.toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '') || 'provider';
    let id = root;
    for (let n = 2; ids.has(id); n++) id = `${root}-${n}`;
    return id;
  }

  function fromPreset(p: Preset, baseUrl?: string, models?: string[]) {
    edit = {
      id: freeId(p.kind === 'custom' ? 'custom' : p.id), name: p.kind === 'custom' ? '' : p.name, preset: p.id, wire: p.wire,
      baseUrl: baseUrl ?? p.baseUrl, model: models?.[0] ?? '', fastModel: '', auth: p.auth,
      maxTokensField: p.maxTokensField ?? null, reasoning: p.reasoning, hasKey: false,
      key: '', isNew: true, error: '', checking: false, models: models ? models.map((id) => ({ id })) : null,
      advanced: p.kind === 'custom', keyUrl: p.keyUrl,
    };
    if (p.auth === 'none' && !models) check();
  }

  function editProfile(p: Profile) {
    edit = { ...p, key: '', isNew: false, error: '', checking: false, models: null, advanced: false, keyUrl: presets.find((x) => x.id === p.preset)?.keyUrl };
    check();
  }

  function profileOf(d: Draft): Profile {
    const { key: _k, isNew: _n, error: _e, checking: _c, models: _m, advanced: _a, keyUrl: _u, ...p } = d;
    return p;
  }

  async function check() {
    if (!edit) return;
    const d = edit;
    d.checking = true;
    d.error = '';
    try {
      d.models = await call('provider_check', { profile: profileOf(d), key: d.key.trim() || null });
      if (!d.model && d.models.length) d.model = d.models[0].id;
    } catch (e) {
      d.models = null;
      d.error = String(e);
    }
    d.checking = false;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!edit || !app.settings) return;
    const p = profileOf(edit);
    const profiles = edit.isNew ? [...app.settings.profiles, p] : app.settings.profiles.map((x) => (x.id === p.id ? p : x));
    try {
      app.settings = await call('settings_save', { settings: { ...app.settings, profiles, activeProfile: edit.isNew && !app.settings.activeProfile ? p.id : app.settings.activeProfile } });
      if (edit.key.trim()) await call('profile_key', { id: p.id, key: edit.key.trim() });
      app.settings = await call('settings_get');
      app.providerModels[p.id] = edit.models ?? [];
      toast(`${p.name} connected`);
      edit = null;
    } catch (err) {
      if (edit) edit.error = String(err);
    }
  }

  async function remove(p: Profile) {
    if (!app.settings) return;
    await call('profile_key', { id: p.id, key: null }).catch(() => {});
    await app.saveSettings({ profiles: app.settings.profiles.filter((x) => x.id !== p.id), activeProfile: app.settings.activeProfile === p.id ? null : app.settings.activeProfile });
  }

  const badge = (m: ModelInfo) => [m.context ? `${Math.round(m.context / 1000)}k` : '', m.tools === false ? 'chat only' : '', m.vision ? 'vision' : '', m.reasoning ? 'reasoning' : ''].filter(Boolean).join(' · ');
  const chosen = $derived(edit?.models?.find((m) => m.id === edit?.model));
  const accents = ['sky', 'marigold', 'coral', 'mocha', 'midnight'];
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
          <span class="pt">
            <span class="pn">{p.name}<span class="wire">{p.wire === 'anthropic' ? 'Anthropic API' : 'OpenAI API'}</span>{#if p.auth !== 'none' && !p.hasKey}<span class="pill error">No key</span>{/if}</span>
            <span class="pu">{p.model ? `${p.id}/${p.model}` : 'No model chosen'} · {p.baseUrl}</span>
          </span>
        </button>
        <button class="btn sm" onclick={() => editProfile(p)}>Edit</button>
        <button class="icon-btn" title="Remove" onclick={() => remove(p)}><Icon name="trash" size={14} /></button>
      </div>
    {/each}
  </div>

  {#if edit}
    <h3>{edit.isNew ? `Connect ${edit.name || 'a provider'}` : `Edit ${edit.name}`}</h3>
    <form class="card form" onsubmit={save} autocomplete="off">
      <label for="pn">Name</label><input id="pn" class="input" bind:value={edit.name} placeholder="My server" required />
      <label for="pb">Address</label><input id="pb" class="input mono" bind:value={edit.baseUrl} placeholder={edit.wire === 'anthropic' ? 'https://host (without /v1)' : 'https://host/v1'} required />
      {#if edit.auth !== 'none'}
        <label for="pk">Key</label>
        <div class="line">
          <input id="pk" class="input" type="password" bind:value={edit.key} placeholder={edit.hasKey ? 'Saved in Credential Manager · type to replace' : 'Paste the key; it goes to Windows Credential Manager'} />
          {#if edit.keyUrl}<button type="button" class="btn sm" onclick={() => call('open_url', { url: edit!.keyUrl! })}>Get a key<Icon name="external" size={12} /></button>{/if}
        </div>
      {/if}
      <span></span>
      <div class="line">
        <button type="button" class="btn ghost-blue sm" disabled={edit.checking || !edit.baseUrl} onclick={check}>{#if edit.checking}<span class="spinner"></span>{/if}Check</button>
        {#if edit.error}<span class="err">{edit.error}</span>
        {:else if edit.models}<span class="ok"><Icon name="check" size={13} />Answers: {edit.models.length} model{edit.models.length === 1 ? '' : 's'}</span>
        {:else}<span class="hint">Forge lists the provider's models with this key.</span>{/if}
      </div>
      <label for="pm">Model</label>
      <div class="line">
        <input id="pm" class="input mono" list="pm-list" bind:value={edit.model} placeholder="Pick or type a model" />
        {#if chosen && badge(chosen)}<span class="hint">{badge(chosen)}</span>{/if}
      </div>
      {#if chosen?.tools === false}<span></span><p class="warn">This model can't call tools: Claude Code can only chat with it.</p>{/if}
      <label for="pf">Fast model</label>
      <input id="pf" class="input mono" list="pm-list" bind:value={edit.fastModel} placeholder="Titles and quick tasks · empty: the model above" />
      <datalist id="pm-list">{#each edit.models ?? [] as m}<option value={m.id}>{badge(m)}</option>{/each}</datalist>
      <span></span>
      <button type="button" class="adv" onclick={() => edit && (edit.advanced = !edit.advanced)}><Icon name="chevron" size={12} />Advanced</button>
      {#if edit.advanced}
        <label for="pw">API</label>
        <select id="pw" class="input" bind:value={edit.wire}><option value="anthropic">Anthropic Messages</option><option value="openai-chat">OpenAI chat completions</option></select>
        <label for="pa">Authentication</label>
        <select id="pa" class="input" bind:value={edit.auth}><option value="none">None (local server)</option><option value="apiKey">API key (x-api-key)</option><option value="bearer">Bearer token</option></select>
        {#if edit.wire === 'openai-chat'}
          <label for="pt">Token limit field</label>
          <select id="pt" class="input" value={edit.maxTokensField ?? 'max_tokens'} onchange={(e) => edit && (edit.maxTokensField = e.currentTarget.value)}><option value="max_tokens">max_tokens</option><option value="max_completion_tokens">max_completion_tokens</option></select>
          <span class="lab">Reasoning effort</span>
          <div class="line"><Toggle checked={edit.reasoning} label="Send reasoning effort" onchange={(v) => edit && (edit.reasoning = v)} /><span class="hint">Map Claude's thinking budget to reasoning_effort</span></div>
        {/if}
      {/if}
      <div class="actions"><button type="button" class="btn" onclick={() => (edit = null)}>Cancel</button><button class="btn primary" disabled={!edit.name.trim() || !edit.baseUrl}>Save</button></div>
    </form>
  {:else}
    {#if found.length}
      <h3>Found on this PC</h3>
      <div class="card group">
        {#each found as d (d.preset)}
          <div class="prow">
            <span class="pt"><span class="pn">{d.name}</span><span class="pu">{d.baseUrl} · {d.models.length} model{d.models.length === 1 ? '' : 's'}</span></span>
            <button class="btn ghost-blue sm" onclick={() => { const p = presets.find((x) => x.id === d.preset); if (p) fromPreset(p, d.baseUrl, d.models); }}>Connect</button>
          </div>
        {/each}
      </div>
    {/if}
    <h3>Add a provider</h3>
    <div class="tiles">
      {#each tiles as p, i (p.id)}
        <button class="tile a-{p.kind === 'custom' ? 'plain' : accents[i % accents.length]}" onclick={() => fromPreset(p)}>
          <span class="tn">{p.name}</span>
          <span class="td">{p.note ?? (p.kind === 'custom' ? 'Any server that speaks this API' : p.wire === 'anthropic' ? 'Anthropic API' : 'OpenAI API')}</span>
        </button>
      {/each}
    </div>
    <h3>Local servers</h3>
    <p class="cap">{detected === null ? 'Looking on this PC…' : found.length ? 'Not running or not found:' : 'None answered on its usual port. Start one, or connect it by address:'}</p>
    <div class="chips">
      {#each localTiles.filter((p) => !found.some((d) => d.preset === p.id)) as p (p.id)}
        <button class="chip" onclick={() => fromPreset(p)}>{p.name}<span class="port">{p.baseUrl.replace('http://127.0.0.1', '')}</span></button>
      {/each}
    </div>
  {/if}
{/if}

<style>
  h3 { margin: 0 0 8px; font-size: 13px; font-weight: 600; color: var(--ink-2); }
  h3 ~ h3 { margin-top: 26px; }
  .cap { margin: -4px 0 10px; font-size: 13px; color: var(--ink-3); }
  .group { overflow: hidden; }
  .prow { display: flex; align-items: center; gap: 10px; width: 100%; padding: 12px 16px; text-align: left; }
  .prow + .prow { border-top: 1px solid var(--line); }
  .pick { flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; text-align: left; }
  .radio { width: 18px; height: 18px; border-radius: 50%; box-shadow: inset 0 0 0 1.5px var(--ink-5); flex: none; display: grid; place-items: center; }
  .on .radio { box-shadow: inset 0 0 0 1.5px var(--blue); }
  .on .radio::after { content: ''; width: 9px; height: 9px; border-radius: 50%; background: var(--blue); }
  .pt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .pn { font-weight: 500; display: flex; align-items: center; gap: 8px; }
  .wire { font-size: 11px; font-weight: 500; color: var(--ink-4); }
  .pu { font: 12px var(--font-mono); color: var(--ink-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .form { display: grid; grid-template-columns: 130px 1fr; gap: 12px 16px; align-items: center; padding: 20px; }
  .form label, .lab { font-size: 14px; color: var(--ink-3); }
  .form .input { width: 100%; }
  .mono { font-family: var(--font-mono); font-size: 13px; }
  .line { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .line .input { flex: 1; }
  .hint { font-size: 12px; color: var(--ink-4); white-space: nowrap; }
  .ok { display: inline-flex; align-items: center; gap: 5px; font-size: 13px; color: var(--add-fg); }
  .err { color: var(--status-error-fg); font-size: 13px; }
  .warn { margin: 0; font-size: 13px; color: var(--status-needs-fg); }
  .adv { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; color: var(--ink-3); justify-self: start; }
  .actions { grid-column: 1 / -1; display: flex; justify-content: flex-end; gap: 8px; }
  .tiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 10px; }
  .tile { display: flex; flex-direction: column; gap: 4px; align-items: flex-start; padding: 14px 16px; border-radius: var(--r-card); text-align: left; min-height: 78px; transition: filter var(--t) var(--ease); }
  .tile:hover { filter: brightness(0.97); }
  .tn { font-weight: 600; }
  .td { font-size: 12px; opacity: 0.75; }
  /* Sticky-note tiles: the accent cast, washed so the text stays readable. */
  .a-sky { background: color-mix(in srgb, var(--sky) 22%, var(--card)); }
  .a-marigold { background: color-mix(in srgb, var(--marigold) 24%, var(--card)); }
  .a-coral { background: color-mix(in srgb, var(--coral) 16%, var(--card)); }
  .a-mocha { background: color-mix(in srgb, var(--mocha) 22%, var(--card)); }
  .a-midnight { background: color-mix(in srgb, var(--midnight) 10%, var(--card)); }
  .a-plain { background: var(--card); box-shadow: inset 0 0 0 1px var(--line); }
  .chips { display: flex; flex-wrap: wrap; gap: 8px; }
  .chip { display: inline-flex; align-items: center; gap: 8px; height: 30px; padding: 0 12px; border-radius: var(--r-pill); background: var(--card); box-shadow: inset 0 0 0 1px var(--line); font-size: 13px; }
  .chip:hover { box-shadow: inset 0 0 0 1px var(--line-strong); }
  .port { font: 11px var(--font-mono); color: var(--ink-4); }
</style>
