<script lang="ts">
  import Section from '../Section.svelte';
  import Field from '../Field.svelte';
  import { call, type Scope } from '../../../lib/ipc';
  import { toast } from '../../../lib/store.svelte';
  import { scopeLabel } from '../../../lib/ccsettings';
  import { settingsWindow as sw } from '../../../lib/settings.svelte';

  const FILES: Record<Scope, { name: string; desc: string }> = {
    user: { name: '~/.claude/CLAUDE.md', desc: 'Your instructions for every project.' },
    project: { name: 'CLAUDE.md', desc: 'Instructions for this project, committed and shared with everyone who works on it.' },
    local: { name: 'CLAUDE.local.md', desc: 'Your own instructions for this project. Keep it out of git.' },
  };
  let text = $state('');
  let saved = $state('');
  let loading = $state(true);
  let loadedFor: Scope | null = null;

  $effect(() => {
    const which = sw.scope;
    if (which === loadedFor) return;
    loadedFor = which;
    loading = true;
    call('memory_read', { which }).then((t) => { if (sw.scope === which) { text = saved = t; loading = false; } })
      .catch((e) => { toast(String(e), 'error'); loading = false; });
  });

  async function save() {
    try {
      await call('memory_write', { which: sw.scope, text });
      saved = text;
      toast(`${FILES[sw.scope].name} saved`);
    } catch (e) { toast(String(e), 'error'); }
  }
  const lines = $derived(text ? text.split('\n').length : 0);
</script>

<Section title="{scopeLabel(sw.scope)} memory · {FILES[sw.scope].name}" caption={FILES[sw.scope].desc}>
  <div class="editor">
    <textarea class="mono" bind:value={text} disabled={loading} spellcheck="false"
      placeholder={loading ? 'Loading…' : '# Instructions\n\n- Use pnpm, not npm.\n- Run the tests before saying you are done.'}
      onkeydown={(e) => { if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); save(); } }}></textarea>
    <div class="bar">
      <span class="info">{lines} lines{text !== saved ? ' · unsaved' : ''}</span>
      <button class="btn sm" disabled={text === saved} onclick={() => (text = saved)}>Revert</button>
      <button class="btn primary sm" disabled={text === saved} onclick={save}>Save</button>
    </div>
  </div>
</Section>

<Section title="Company announcements">
  <Field key="companyAnnouncements" type="list" label="Announcements" desc="Messages Claude Code shows at startup, one per entry." placeholder="Deploy freeze until Friday" />
</Section>

<style>
  .editor { display: flex; flex-direction: column; }
  textarea { min-height: 300px; resize: vertical; border: 0; outline: 0; padding: 14px 18px; background: none; font: 13px/1.6 var(--font-mono); color: var(--ink); user-select: text; }
  .bar { display: flex; align-items: center; gap: 8px; padding: 8px 12px 8px 18px; border-top: 1px solid var(--line); }
  .info { flex: 1; font-size: 12px; color: var(--ink-4); }
</style>
