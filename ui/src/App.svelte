<script lang="ts">
  import TitleBar from './components/TitleBar.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import Welcome from './components/Welcome.svelte';
  import Thread from './components/Thread.svelte';
  import ChangesPane from './components/ChangesPane.svelte';
  import SettingsModal from './components/settings/SettingsModal.svelte';
  import MenuHost from './components/MenuHost.svelte';
  import Toasts from './components/Toasts.svelte';
  import { app, toast } from './lib/store.svelte';
  import { call, connect } from './lib/ipc';
  import { settingsWindow } from './lib/settings.svelte';
  import { tick } from 'svelte';

  let error = $state<string | null>(null);
  const systemDark = matchMedia('(prefers-color-scheme: dark)');
  let prefersDark = $state(systemDark.matches);
  systemDark.addEventListener('change', (e) => (prefersDark = e.matches));

  // "system" follows Windows; the resolved theme goes on <html data-theme>.
  $effect(() => {
    const t = app.settings?.theme ?? 'system';
    app.dark = t === 'dark' || (t === 'system' && prefersDark);
    document.documentElement.dataset.theme = app.dark ? 'dark' : 'light';
  });
  $effect(() => {
    const s = app.settings;
    if (s?.sidebarWidth) document.documentElement.style.setProperty('--sidebar-w', `${s.sidebarWidth}px`);
    if (s?.changesWidth) document.documentElement.style.setProperty('--changes-w', `${s.changesWidth}px`);
  });

  async function start() {
    const backend = await connect();
    await backend.listen(app.onBatch);
    [app.info, app.settings] = await Promise.all([call('app_info'), call('settings_get')]);
    for (const id of app.info.running) app.session(id).running = true;
    await Promise.all([app.refreshList(), app.refreshGit()]);
    app.maximized = await call('window_action', { action: 'isMaximized' }).catch(() => false);
    // Shown only once drawn: no white flash, no half-drawn first frame.
    // (Not requestAnimationFrame: a hidden window never gets a frame.)
    await tick();
    call('window_ready').catch(() => {});
    setInterval(() => app.refreshGit(), 4000);
    addEventListener('focus', () => { app.refreshGit(); app.refreshList(); });
  }
  start().catch((e) => { console.error(e); error = String(e); call('window_ready').catch(() => {}); });

  function keys(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === 'Escape' && settingsWindow.open) { e.preventDefault(); settingsWindow.close(); return; }
    if (e.key === 'Escape' && app.currentSession?.busy) { app.stop(); return; }
    if (!mod) return;
    const k = e.key.toLowerCase();
    if (k === 'n') { e.preventDefault(); app.newSession(); }
    else if (k === 'b') { e.preventDefault(); app.sidebar = !app.sidebar; }
    else if (k === 'd' && e.shiftKey) { e.preventDefault(); app.changes = !app.changes; }
    else if (e.key === '/') { e.preventDefault(); app.pickerOpen = !app.pickerOpen; }
    else if (e.key === ',') { e.preventDefault(); if (settingsWindow.open) settingsWindow.close(); else settingsWindow.show(); }
    else if (k === 'o') { e.preventDefault(); call('open_project', { path: null }).catch((err) => toast(String(err), 'error')); }
  }
</script>

<svelte:window onkeydown={keys} />

<div class="shell">
  <TitleBar />
  <div class="body">
    {#if app.sidebar}<Sidebar />{/if}
    <main class="main">
      {#if error}
        <div class="fatal"><h2>Forge could not start</h2><p>{error}</p></div>
      {:else if app.currentSession?.items.length || app.currentSession?.loaded === 'loading'}
        <Thread session={app.currentSession} />
      {:else if app.info}
        <Welcome />
      {/if}
    </main>
    {#if app.changes && app.git.isRepo}<ChangesPane />{/if}
  </div>
</div>
{#if settingsWindow.open}<SettingsModal />{/if}
<MenuHost />
<Toasts />

<style>
  .shell { display: grid; grid-template-rows: var(--titlebar) 1fr; height: 100vh; }
  .body { display: flex; min-height: 0; }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; background: var(--canvas); position: relative; }
  .fatal { margin: auto; max-width: 480px; }
</style>
