<script lang="ts">
  import Icon from './Icon.svelte';
  import { app, toast } from '../lib/store.svelte';
  import { call, type FileDiff } from '../lib/ipc';
  import { diffLines, renderDiff } from '../lib/diff';

  let selected = $state<string | null>(null);
  let diff = $state<FileDiff | null>(null);
  let confirming = $state(false);

  const files = $derived(app.git.files);
  const add = $derived(files.reduce((n, f) => n + f.additions, 0));
  const del = $derived(files.reduce((n, f) => n + f.deletions, 0));
  const letter: Record<string, string> = { modified: 'M', added: 'A', untracked: 'U', deleted: 'D', renamed: 'R', conflicted: '!' };

  $effect(() => {
    if (!files.some((f) => f.path === selected)) selected = files[0]?.path ?? null;
  });
  $effect(() => {
    const path = selected;
    files; // refresh when the change list does
    confirming = false;
    if (!path) { diff = null; return; }
    call('git_diff', { path }).then((d) => { if (selected === path) diff = d; }).catch((e) => toast(String(e), 'error'));
  });

  async function discard() {
    if (!selected) return;
    try { await call('git_discard', { path: selected }); await app.refreshGit(); } catch (e) { toast(String(e), 'error'); }
    confirming = false;
  }
  const name = (p: string) => p.split('/').pop();
  const dir = (p: string) => p.split('/').slice(0, -1).join('/');
</script>

<aside class="pane">
  <header class="head">
    <h2>Changes</h2>
    <span class="n">{files.length} file{files.length === 1 ? '' : 's'}</span>
    <span class="counts"><span class="add">+{add}</span><span class="del">−{del}</span></span>
    <span class="grow"></span>
    <button class="icon-btn" title="Refresh" onclick={() => app.refreshGit()}><Icon name="refresh" size={14} /></button>
    <button class="icon-btn" title="Close" onclick={() => (app.changes = false)}><Icon name="x" size={14} /></button>
  </header>
  {#if files.length}
    <div class="files">
      {#each files as f (f.path)}
        <button class="file" class:sel={f.path === selected} title={f.path} onclick={() => (selected = f.path)}>
          <span class="st {f.status}">{letter[f.status] ?? '?'}</span>
          <span class="fn">{name(f.path)}</span>
          <span class="dir">{dir(f.path)}</span>
          <span class="counts">{#if f.binary}bin{:else}<span class="add">+{f.additions}</span>{#if f.deletions}<span class="del">−{f.deletions}</span>{/if}{/if}</span>
        </button>
      {/each}
    </div>
    {#if diff}
      <div class="bar">
        <span class="path">{diff.path}</span>
        <button class="btn sm" onclick={() => call('reveal', { path: diff!.path }).catch((e) => toast(String(e), 'error'))}>Show in folder</button>
        {#if confirming}
          <button class="btn sm danger" onclick={discard}>Discard changes</button>
          <button class="btn sm" onclick={() => (confirming = false)}>Cancel</button>
        {:else}
          <button class="btn sm" onclick={() => (confirming = true)}>Discard…</button>
        {/if}
      </div>
      <div class="view">
        {#if diff.tooLargeOrBinary}<p class="empty">Binary or too large to show.</p>
        {:else}{@html renderDiff(diffLines(diff.original, diff.modified), 3)}{/if}
      </div>
    {/if}
  {:else}
    <p class="empty">No changes against HEAD. Edits Claude makes show up here.</p>
  {/if}
</aside>

<style>
  .pane { width: var(--changes-w); flex: none; display: flex; flex-direction: column; min-height: 0; background: var(--card); box-shadow: inset 1px 0 0 var(--line); }
  .head { height: 44px; flex: none; display: flex; align-items: center; gap: 10px; padding: 0 8px 0 18px; }
  h2 { margin: 0; font-size: 15px; font-weight: 600; }
  .n { font-size: 13px; color: var(--ink-3); }
  .grow { flex: 1; }
  .files { max-height: 36%; overflow-y: auto; padding: 0 8px 8px; border-bottom: 1px solid var(--line); flex: none; }
  .file { display: flex; align-items: center; gap: 8px; width: 100%; height: 30px; padding: 0 10px; border-radius: var(--r-button); text-align: left; font-size: 14px; }
  .file:hover { background: var(--hover); }
  .file.sel { background: var(--sky-tint); }
  .st { width: 14px; font: 600 11px var(--font-mono); text-align: center; flex: none; }
  .st.modified { color: var(--status-needs-fg); } .st.added, .st.untracked { color: var(--add-fg); } .st.deleted { color: var(--del-fg); } .st.renamed { color: var(--blue); }
  .fn { white-space: nowrap; }
  .dir { flex: 1; min-width: 0; color: var(--ink-4); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .bar { display: flex; align-items: center; gap: 6px; padding: 8px 10px 8px 18px; border-bottom: 1px solid var(--line); flex: none; }
  .path { flex: 1; min-width: 0; font: 12px var(--font-mono); color: var(--ink-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .view { flex: 1; overflow: auto; min-height: 0; padding: 6px 0; }
  .empty { color: var(--ink-3); font-size: 14px; padding: 32px 24px; text-align: center; margin: 0; }
</style>
