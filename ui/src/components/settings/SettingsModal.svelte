<script lang="ts">
  // Settings as a window over the app: dimmed backdrop, a centred panel, a
  // left nav with search. Esc, the close button or a click outside closes it.
  import Icon from '../Icon.svelte';
  import { SCOPES, TABS, searchTabs } from '../../lib/ccsettings';
  import { settingsWindow as sw } from '../../lib/settings.svelte';
  import General from './tabs/General.svelte';
  import Models from './tabs/Models.svelte';
  import Providers from './tabs/Providers.svelte';
  import Permissions from './tabs/Permissions.svelte';
  import Environment from './tabs/Environment.svelte';
  import Memory from './tabs/Memory.svelte';
  import Hooks from './tabs/Hooks.svelte';
  import Mcp from './tabs/Mcp.svelte';
  import ExtensionList from './tabs/ExtensionList.svelte';
  import Sandbox from './tabs/Sandbox.svelte';
  import Network from './tabs/Network.svelte';
  import About from './tabs/About.svelte';

  const LEADS: Record<string, string> = {
    general: 'How Forge looks, and how Claude Code behaves in every session.',
    models: 'Which model Claude Code uses, how hard it thinks, and its limits.',
    providers: 'Where requests go: your Claude account, or another endpoint.',
    permissions: 'What Claude Code may do without asking, what it must ask for, and what it never does.',
    environment: 'Variables Claude Code sets for itself and every tool it runs.',
    memory: 'Instructions Claude Code reads at the start of every session.',
    hooks: 'Commands Claude Code runs at points in its work: before a tool, after a reply, at startup.',
    mcp: 'Servers that give Claude Code more tools.',
    skills: 'Packaged know-how Claude Code loads when a task calls for it.',
    agents: 'Focused helpers the conversation can hand work to.',
    commands: 'Your own slash commands, saved prompts you run with /name.',
    sandbox: 'Run shell commands in an OS sandbox that limits what they can touch.',
    network: 'Proxies and certificates for Claude Code’s connections.',
    about: 'Forge Desktop, a Windows app for Claude Code.',
  };

  const shown = $derived(searchTabs(sw.query, TABS));
  const tab = $derived(TABS.find((t) => t.id === sw.tab)!);
  const scope = $derived(SCOPES.find((s) => s.id === sw.scope)!);
  let search = $state<HTMLInputElement>();

  $effect(() => { search?.focus(); });

  function onSearchKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && shown.length) sw.tab = shown[0].id;
    if (e.key === 'ArrowDown' && shown.length) {
      e.preventDefault();
      const i = shown.findIndex((t) => t.id === sw.tab);
      sw.tab = shown[Math.min(shown.length - 1, i + 1)].id;
    }
    if (e.key === 'ArrowUp' && shown.length) {
      e.preventDefault();
      const i = shown.findIndex((t) => t.id === sw.tab);
      sw.tab = shown[Math.max(0, i - 1)].id;
    }
  }
  // While searching, show the best match.
  $effect(() => {
    if (sw.query.trim() && shown.length && !shown.some((t) => t.id === sw.tab)) sw.tab = shown[0].id;
  });
</script>

<div class="backdrop" role="presentation" onmousedown={(e) => { if (e.target === e.currentTarget) sw.close(); }}>
  <div class="panel" role="dialog" aria-modal="true" aria-label="Settings">
    <nav class="nav">
      <div class="title">Settings</div>
      <label class="search">
        <Icon name="search" size={14} />
        <input bind:this={search} bind:value={sw.query} placeholder="Search settings" spellcheck="false" onkeydown={onSearchKey} />
        {#if sw.query}<button class="clear" title="Clear" onclick={() => (sw.query = '')}><Icon name="x" size={12} /></button>{/if}
      </label>
      <div class="tabs">
        {#each shown as t (t.id)}
          <button class="tab" class:on={sw.tab === t.id} onclick={() => (sw.tab = t.id)}><Icon name={t.icon} />{t.label}</button>
        {:else}
          <p class="nores">No setting matches “{sw.query}”.</p>
        {/each}
      </div>
    </nav>

    <div class="main">
      <header class="head">
        <div class="ht">
          <h1>{tab.label}</h1>
          <p class="lead">{LEADS[tab.id]}</p>
        </div>
        <button class="icon-btn close" title="Close (Esc)" onclick={() => sw.close()}><Icon name="x" /></button>
      </header>
      {#if tab.scoped}
        <div class="scopebar">
          <div class="seg" role="radiogroup" aria-label="Settings scope">
            {#each SCOPES as s}
              <button role="radio" aria-checked={sw.scope === s.id} class:on={sw.scope === s.id}
                disabled={s.id !== 'user' && !sw.cc.hasProject} title={s.id !== 'user' && !sw.cc.hasProject ? 'Open a project first' : s.file}
                onclick={() => (sw.scope = s.id)}>{s.label}</button>
            {/each}
          </div>
          <span class="file">Editing <span class="mono">{tab.id === 'memory' ? { user: '~/.claude/CLAUDE.md', project: 'CLAUDE.md', local: 'CLAUDE.local.md' }[sw.scope] : scope.file}</span>
            · {sw.scope === 'user' ? 'every project' : sw.scope === 'project' ? 'shared with the team' : 'just you, this project'}</span>
        </div>
      {/if}
      <div class="scroll">
        <div class="body">
          {#if sw.cc.errors.length}
            <div class="errors card">{#each sw.cc.errors as e}<p><Icon name="alert" size={14} />{e}</p>{/each}<p class="sub">Forge shows these files but won't write them until they're fixed.</p></div>
          {/if}
          {#if !sw.loaded}
            <div class="loading"><span class="spinner"></span></div>
          {:else if sw.tab === 'general'}<General />
          {:else if sw.tab === 'models'}<Models />
          {:else if sw.tab === 'providers'}<Providers />
          {:else if sw.tab === 'permissions'}<Permissions />
          {:else if sw.tab === 'environment'}<Environment />
          {:else if sw.tab === 'memory'}<Memory />
          {:else if sw.tab === 'hooks'}<Hooks />
          {:else if sw.tab === 'mcp'}<Mcp />
          {:else if sw.tab === 'skills'}<ExtensionList kind="skills" />
          {:else if sw.tab === 'agents'}<ExtensionList kind="agents" />
          {:else if sw.tab === 'commands'}<ExtensionList kind="commands" />
          {:else if sw.tab === 'sandbox'}<Sandbox />
          {:else if sw.tab === 'network'}<Network />
          {:else}<About />
          {/if}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 50; background: rgba(15, 12, 10, 0.32); display: grid; place-items: center;
    animation: fade 160ms var(--ease);
  }
  :global(:root[data-theme='dark']) .backdrop { background: rgba(0, 0, 0, 0.55); }
  .panel {
    width: min(1040px, calc(100vw - 64px)); height: min(760px, calc(100vh - 64px)); display: grid; grid-template-columns: 236px 1fr;
    background: var(--canvas); border-radius: var(--r-card); box-shadow: var(--float); overflow: hidden; animation: rise 180ms var(--ease);
  }
  .nav { display: flex; flex-direction: column; min-height: 0; padding: 18px 10px 12px; border-right: 1px solid var(--line); }
  .title { font-size: 15px; font-weight: 600; padding: 0 10px 12px; letter-spacing: -0.01em; }
  .search { display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 10px; margin: 0 2px 10px; border-radius: var(--r-button); background: var(--card); box-shadow: inset 0 0 0 1px var(--line-strong); color: var(--ink-4); }
  .search:focus-within { box-shadow: inset 0 0 0 1px var(--blue), 0 0 0 3px var(--focus); }
  .search input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font-size: 13px; color: var(--ink); user-select: text; }
  .search input::placeholder { color: var(--ink-4); }
  .clear { display: grid; place-items: center; color: var(--ink-4); }
  .tabs { flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 1px; }
  .tab { display: flex; align-items: center; gap: 10px; height: 32px; padding: 0 10px; border-radius: var(--r-button); color: color-mix(in srgb, var(--ink) 60%, transparent); font-weight: 500; text-align: left; flex: none; }
  .tab:hover { background: var(--hover); color: var(--ink); }
  .tab.on { background: var(--card); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line); }
  .nores { font-size: 13px; color: var(--ink-4); padding: 6px 10px; margin: 0; }
  .main { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .head { display: flex; align-items: flex-start; gap: 16px; padding: 22px 20px 0 36px; }
  .ht { flex: 1; min-width: 0; }
  h1 { margin: 0 0 4px; font-size: 24px; font-weight: 600; letter-spacing: -0.02em; }
  .lead { margin: 0; font: 400 16px/1.5 var(--font-serif); color: var(--graphite); max-width: 640px; }
  .close { flex: none; }
  .scopebar { display: flex; align-items: center; gap: 12px; padding: 16px 36px 0; flex-wrap: wrap; }
  .file { font-size: 12px; color: var(--ink-4); }
  .mono { font-family: var(--font-mono); font-size: 11.5px; color: var(--ink-3); }
  .seg { display: inline-flex; padding: 3px; gap: 2px; border-radius: var(--r-button); background: var(--hover); }
  .seg button { height: 26px; padding: 0 12px; border-radius: 6px; font-size: 13px; font-weight: 500; color: var(--ink-3); }
  .seg button.on { background: var(--card); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line); }
  .seg button:disabled { opacity: 0.4; cursor: default; }
  .scroll { flex: 1; overflow-y: auto; min-height: 0; }
  .body { padding: 20px 36px 48px; max-width: 820px; }
  .errors { padding: 12px 16px; margin-bottom: 20px; color: var(--status-error-fg); font-size: 13px; }
  .errors p { margin: 2px 0; display: flex; gap: 6px; align-items: center; }
  .errors .sub { color: var(--ink-3); }
  .loading { display: grid; place-items: center; padding: 60px; }
  @keyframes fade { from { opacity: 0; } }
</style>
