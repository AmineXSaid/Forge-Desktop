<script lang="ts">
  import Icon from './Icon.svelte';
  import { app, toast } from '../lib/store.svelte';
  import { call, type Summary } from '../lib/ipc';
  import { openMenu } from '../lib/menu.svelte';
  import { when, bucket } from '../lib/time';
  import { settingsWindow } from '../lib/settings.svelte';

  let editing = $state<string | null>(null);
  let editValue = $state('');

  const groups = $derived.by(() => {
    const out: { label: string; rows: Summary[] }[] = [];
    const rows = app.rows;
    const pinned = rows.filter((r) => r.pinned);
    if (pinned.length) out.push({ label: 'Pinned', rows: pinned });
    for (const r of rows.filter((r) => !r.pinned)) {
      const label = bucket(r.updated);
      if (out.at(-1)?.label !== label) out.push({ label, rows: [] });
      out.at(-1)!.rows.push(r);
    }
    return out;
  });

  const profile = $derived(app.settings?.profiles.find((p) => p.id === app.settings?.activeProfile));

  function more(e: MouseEvent, r: Summary) {
    e.stopPropagation();
    openMenu(e.currentTarget as HTMLElement, [
      { label: r.pinned ? 'Unpin' : 'Pin', icon: 'pin', run: () => app.pin(r.id, !r.pinned) },
      { label: 'Rename', icon: 'pencil', run: () => { editing = r.id; editValue = r.title; } },
      ...(app.list.some((x) => x.id === r.id) ? [{ label: 'Fork', desc: 'Continue in a copy', icon: 'branch', run: () => app.fork(r.id) }] : []),
      '-' as const,
      { label: 'Archive', icon: 'archive', run: () => archive(r.id) },
    ], { align: 'right', width: 220 });
  }

  async function archive(id: string) {
    await call('session_stop', { id }).catch(() => {});
    try {
      await call('session_archive', { id, archived: true });
      app.list = app.list.filter((r) => r.id !== id);
      app.sessions.delete(id);
      if (app.current === id) app.current = null;
    } catch (e) { toast(String(e), 'error'); }
  }

  async function commit(id: string) {
    const title = editValue.trim();
    editing = null;
    const row = app.list.find((r) => r.id === id);
    if (!title || title === row?.title) return;
    if (row) {
      try { await call('session_rename', { id, title }); row.title = title; } catch (e) { toast(String(e), 'error'); }
    } else app.session(id).title = title;
  }

  function focus(el: HTMLInputElement) { el.focus(); el.select(); }
</script>

<aside class="side">
  <div class="top">
    <button class="new" onclick={() => app.newSession()}>
      <Icon name="plus" /><span>New session</span><kbd>Ctrl N</kbd>
    </button>
    <label class="search">
      <Icon name="search" size={14} />
      <input placeholder="Search" value={app.search} oninput={(e) => app.searchFor(e.currentTarget.value)} />
    </label>
  </div>

  <nav class="list">
    {#each groups as g (g.label)}
      <div class="label">{g.label}</div>
      {#each g.rows as r (r.id)}
        {@const s = app.sessions.get(r.id)}
        {#if editing === r.id}
          <div class="row sel"><input class="rename" use:focus bind:value={editValue}
            onkeydown={(e) => { if (e.key === 'Enter') commit(r.id); if (e.key === 'Escape') editing = null; }}
            onblur={() => commit(r.id)} /></div>
        {:else}
          <div class="row" class:sel={app.current === r.id} role="button" tabindex="0" title={r.title}
            onclick={() => app.open(r.id)} onkeydown={(e) => e.key === 'Enter' && app.open(r.id)}>
            {#if r.unread && app.current !== r.id}<span class="unread" title="New since you last looked"></span>{/if}
            <span class="t" class:bold={r.unread && app.current !== r.id}>{r.title}</span>
            {#if s?.status === 'needs'}<span class="pill needs">Needs you</span>
            {:else if s?.status === 'working'}<span class="pill working"><span class="pulse"></span>Working</span>
            {:else}<span class="when">{when(r.updated)}</span>{/if}
            <button class="more" title="More" onclick={(e) => more(e, r)}><Icon name="dots" size={14} /></button>
          </div>
        {/if}
      {/each}
    {:else}
      <p class="empty">{app.search ? 'No sessions match.' : 'Sessions you start appear here.'}</p>
    {/each}
  </nav>

  <button class="who" onclick={() => settingsWindow.show('providers')}>
    <span class="mark" class:local={!!profile}><Icon name={profile ? 'cpu' : 'sparkle'} size={14} /></span>
    <span class="who-text">
      <span class="l1">{profile ? profile.name : 'Claude account'}</span>
      <span class="l2">{profile ? (profile.model || profile.baseUrl) : 'Signed in with Claude Code'}</span>
    </span>
    <Icon name="chevron" size={14} />
  </button>
</aside>

<style>
  .side { width: var(--sidebar-w); flex: none; display: flex; flex-direction: column; min-height: 0; background: var(--canvas); }
  .top { padding: 4px 10px 6px; display: flex; flex-direction: column; gap: 4px; }
  .new {
    display: flex; align-items: center; gap: 9px; height: 36px; padding: 0 10px 0 12px; border-radius: var(--r-button);
    background: var(--card); box-shadow: inset 0 0 0 1px var(--line); font-weight: 500; color: var(--ink);
    transition: box-shadow var(--t) var(--ease);
  }
  .new:hover { box-shadow: inset 0 0 0 1px var(--line-strong); }
  .new span { flex: 1; text-align: left; }
  .search { display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 12px; border-radius: var(--r-button); color: var(--ink-4); }
  .search:focus-within { background: var(--card); box-shadow: inset 0 0 0 1px var(--line); }
  .search input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font-size: 14px; color: var(--ink); }
  .search input::placeholder { color: var(--ink-4); }
  .list { flex: 1; overflow-y: auto; padding: 0 10px 12px; }
  .label { font-size: 12px; font-weight: 500; color: var(--ink-4); padding: 16px 12px 6px; letter-spacing: 0.01em; }
  .row {
    display: flex; align-items: center; gap: 8px; height: 34px; padding: 0 6px 0 12px; border-radius: var(--r-button);
    color: color-mix(in srgb, var(--ink) 58%, transparent); cursor: pointer;
    transition: background var(--t) var(--ease), color var(--t) var(--ease);
  }
  .row:hover { background: var(--hover); color: var(--ink); }
  .row.sel { background: var(--card); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line); font-weight: 500; }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-size: 14px; }
  .unread { width: 7px; height: 7px; border-radius: 50%; background: var(--blue); flex: none; margin-left: -4px; }
  .t.bold { color: var(--ink); font-weight: 500; }
  .when { font-size: 12px; color: var(--ink-4); flex: none; }
  .more { display: none; width: 24px; height: 24px; border-radius: 6px; place-items: center; color: var(--ink-3); flex: none; }
  .row:hover .more { display: grid; }
  .row:hover .when { display: none; }
  .more:hover { background: var(--press); }
  .pulse { width: 6px; height: 6px; border-radius: 50%; background: currentColor; animation: pulse 1.4s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.3; } }
  .rename { flex: 1; min-width: 0; height: 26px; border: 0; outline: 0; background: none; font-size: 14px; }
  .empty { color: var(--ink-4); font-size: 13px; padding: 14px 12px; margin: 0; }
  .who {
    display: flex; align-items: center; gap: 10px; margin: 6px 10px 10px; padding: 8px 10px; border-radius: var(--r-button); text-align: left; color: var(--ink-4);
  }
  .who:hover { background: var(--hover); }
  .mark { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; background: var(--sky-tint); color: var(--blue); flex: none; }
  .mark.local { background: var(--status-needs-bg); color: var(--status-needs-fg); }
  .who-text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .l1 { font-size: 13px; font-weight: 500; color: var(--ink); line-height: 1.3; }
  .l2 { font-size: 12px; color: var(--ink-3); line-height: 1.3; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
