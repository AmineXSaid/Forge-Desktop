<script lang="ts">
  import Icon from './Icon.svelte';
  import { app, toast } from '../lib/store.svelte';
  import { call } from '../lib/ipc';
  import { openMenu } from '../lib/menu.svelte';

  const add = $derived(app.git.files.reduce((n, f) => n + f.additions, 0));
  const del = $derived(app.git.files.reduce((n, f) => n + f.deletions, 0));

  function projects(e: MouseEvent) {
    const recent = (app.info?.recent ?? []).filter((r) => r.path !== app.info?.workspace);
    openMenu(e.currentTarget as HTMLElement, [
      { header: app.info?.workspace ?? '' },
      ...recent.slice(0, 8).map((r) => ({ label: r.name, desc: r.path, icon: 'folder', run: () => call('open_project', { path: r.path }).catch((e) => toast(String(e), 'error')) })),
      ...(recent.length ? (['-'] as const) : []),
      { label: 'Open folder…', icon: 'plus', hint: 'Ctrl O', run: () => call('open_project', { path: null }) },
    ]);
  }

  async function win(action: 'minimize' | 'toggleMaximize' | 'close') {
    app.maximized = await call('window_action', { action });
  }
</script>

<header class="bar">
  <button class="icon-btn" title="Sidebar (Ctrl+B)" onclick={() => (app.sidebar = !app.sidebar)}><Icon name="sidebar" /></button>
  <button class="project" title="Switch project" onclick={projects}>
    <span class="name">{app.info?.name ?? 'Open a folder'}</span>
    <Icon name="chevron" size={14} />
  </button>
  {#if app.git.isRepo && app.git.branch}
    <span class="branch" title="Git branch"><Icon name="branch" size={13} />{app.git.branch}</span>
  {/if}

  <div class="drag" data-tauri-drag-region ondblclick={() => win('toggleMaximize')} role="presentation"></div>

  {#if app.git.isRepo}
    <button class="tb" class:on={app.changes && app.view === 'chat'} title="Changes (Ctrl+Shift+D)"
      onclick={() => { if (app.view !== 'chat') { app.view = 'chat'; app.changes = true; } else app.changes = !app.changes; }}>
      <Icon name="diff" />
      <span>Changes</span>
      {#if app.git.files.length}<span class="counts"><span class="add">+{add}</span><span class="del">−{del}</span></span>{/if}
    </button>
  {/if}
  <button class="icon-btn" class:on={app.view === 'settings'} title="Settings (Ctrl+,)" onclick={() => (app.view = app.view === 'settings' ? 'chat' : 'settings')}>
    <Icon name="gear" />
  </button>

  <div class="win">
    <button title="Minimize" onclick={() => win('minimize')}><Icon name="min" size={14} /></button>
    <button title={app.maximized ? 'Restore' : 'Maximize'} onclick={() => win('toggleMaximize')}><Icon name={app.maximized ? 'restore' : 'max'} size={14} /></button>
    <button class="close" title="Close" onclick={() => win('close')}><Icon name="x" size={14} /></button>
  </div>
</header>

<style>
  .bar { height: var(--titlebar); display: flex; align-items: center; gap: 2px; padding-left: 8px; background: var(--canvas); }
  .drag { flex: 1; align-self: stretch; }
  .project {
    display: inline-flex; align-items: center; gap: 4px; height: 28px; padding: 0 8px 0 10px; margin-left: 2px;
    border-radius: var(--r-button); font-weight: 600; font-size: 14px; letter-spacing: -0.01em; color: var(--ink);
  }
  .project:hover { background: var(--hover); }
  .project :global(.icon) { color: var(--ink-4); }
  .name { max-width: 240px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .branch {
    display: inline-flex; align-items: center; gap: 5px; height: 22px; padding: 0 9px; margin-left: 4px; border-radius: var(--r-pill);
    background: var(--hover); color: var(--ink-3); font: 500 12px/1 var(--font-mono); max-width: 240px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .tb {
    height: 28px; padding: 0 10px; border-radius: var(--r-button); display: inline-flex; align-items: center; gap: 7px;
    color: var(--ink-3); font-size: 13px; font-weight: 500; margin-right: 2px;
  }
  .tb:hover { background: var(--hover); color: var(--ink); }
  .tb.on { background: var(--press); color: var(--ink); }
  .win { display: flex; align-self: stretch; margin-left: 8px; }
  .win button { width: 46px; display: grid; place-items: center; color: var(--ink-3); transition: background var(--t) var(--ease); }
  .win button:hover { background: var(--hover); color: var(--ink); }
  .win .close:hover { background: var(--win-close); color: var(--blue-text); }
</style>
