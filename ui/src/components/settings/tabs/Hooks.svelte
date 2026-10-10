<script lang="ts">
  import Section from '../Section.svelte';
  import Field from '../Field.svelte';
  import Icon from '../../Icon.svelte';
  import { call, type Scope } from '../../../lib/ipc';
  import { toast } from '../../../lib/store.svelte';
  import { PRECEDENCE, scopeLabel } from '../../../lib/ccsettings';
  import { settingsWindow as sw } from '../../../lib/settings.svelte';

  const EVENTS = [
    { id: 'PreToolUse', desc: 'Before a tool runs; can block it.', matcher: true },
    { id: 'PostToolUse', desc: 'After a tool succeeds.', matcher: true },
    { id: 'UserPromptSubmit', desc: 'When you send a message.', matcher: false },
    { id: 'Stop', desc: 'When Claude finishes replying.', matcher: false },
    { id: 'SubagentStop', desc: 'When a subagent finishes.', matcher: false },
    { id: 'Notification', desc: 'When Claude Code sends a notification.', matcher: false },
    { id: 'SessionStart', desc: 'When a session starts or resumes.', matcher: true },
    { id: 'SessionEnd', desc: 'When a session ends.', matcher: false },
    { id: 'PreCompact', desc: 'Before the conversation is compacted.', matcher: true },
  ];

  type HookRow = { scope: Scope; event: string; matcher: string; command: string; timeout: number | null; group: number; index: number };
  const rows = $derived.by(() => {
    const out: HookRow[] = [];
    for (const scope of PRECEDENCE) {
      const hooks = sw.cc[scope]?.hooks;
      if (!hooks || typeof hooks !== 'object') continue;
      for (const [event, groups] of Object.entries(hooks as Record<string, any[]>)) {
        if (!Array.isArray(groups)) continue;
        groups.forEach((g, group) => (g?.hooks ?? []).forEach((h: any, index: number) =>
          out.push({ scope, event, matcher: g.matcher ?? '', command: String(h.command ?? h.type ?? ''), timeout: h.timeout ?? null, group, index })));
      }
    }
    return out;
  });
  const mine = $derived(rows.filter((r) => r.scope === sw.scope));
  const others = $derived(rows.filter((r) => r.scope !== sw.scope));

  let event = $state('PreToolUse');
  let matcher = $state('');
  let command = $state('');
  let timeout = $state('');
  const ev = $derived(EVENTS.find((e) => e.id === event)!);

  async function add(e: SubmitEvent) {
    e.preventDefault();
    try {
      const t = timeout.trim() ? Number(timeout) : null;
      const added = await call('hook_add', { scope: sw.scope, event, matcher: ev.matcher ? matcher.trim() : '', command: command.trim(), timeout: t });
      if (added) { command = ''; matcher = ''; timeout = ''; toast('Hook added'); }
      await sw.refresh();
    } catch (err) { toast(String(err), 'error'); }
  }
  async function remove(r: HookRow) {
    try {
      await call('hook_remove', { scope: r.scope, event: r.event, group: r.group, index: r.index });
      await sw.refresh();
    } catch (err) { toast(String(err), 'error'); }
  }
</script>

<Section title="All hooks">
  <Field key="disableAllHooks" type="bool" fallback={false} label="Disable all hooks" desc="No hook runs, from any settings file or plugin." />
</Section>

<Section title="Hooks in {scopeLabel(sw.scope)}">
  {#each mine as r (r.event + r.group + ':' + r.index)}
    <div class="hook">
      <span class="ev">{r.event}{#if r.matcher}<span class="m">{r.matcher}</span>{/if}</span>
      <code title={r.command}>{r.command}</code>
      {#if r.timeout}<span class="to">{r.timeout}s</span>{/if}
      <button class="icon-btn" title="Remove this hook" onclick={() => remove(r)}><Icon name="trash" size={14} /></button>
    </div>
  {:else}
    <p class="empty">No hooks in {scopeLabel(sw.scope)} settings.</p>
  {/each}
</Section>

<Section title="Add a hook" caption="Forge shows you the exact command and asks before saving it.">
  <form class="form" onsubmit={add} autocomplete="off">
    <label for="hev">When</label>
    <div class="line">
      <select id="hev" class="input" bind:value={event}>{#each EVENTS as e}<option value={e.id}>{e.id}</option>{/each}</select>
      <span class="hint">{ev.desc}</span>
    </div>
    {#if ev.matcher}
      <label for="hm">Matcher</label>
      <input id="hm" class="input mono" bind:value={matcher} placeholder={event === 'SessionStart' ? 'startup, resume, clear or compact' : 'Tool name or pattern, e.g. Edit|Write. Empty: all'} />
    {/if}
    <label for="hc">Command</label>
    <input id="hc" class="input mono" bind:value={command} placeholder={'npx prettier --write "$CLAUDE_PROJECT_DIR"'} required />
    <label for="ht">Timeout</label>
    <div class="line"><input id="ht" class="input num" type="number" min="1" max="3600" bind:value={timeout} placeholder="60" /><span class="hint">seconds</span></div>
    <div class="actions"><button class="btn primary" disabled={!command.trim()}>Add hook…</button></div>
  </form>
</Section>

{#if others.length}
  <Section title="From other scopes" caption="Claude Code runs these too. Switch scope above to change them.">
    {#each others as r (r.scope + r.event + r.group + ':' + r.index)}
      <div class="hook ro">
        <span class="ev">{r.event}{#if r.matcher}<span class="m">{r.matcher}</span>{/if}</span>
        <code title={r.command}>{r.command}</code>
        <span class="sc">{scopeLabel(r.scope)}</span>
      </div>
    {/each}
  </Section>
{/if}

<style>
  .hook { display: flex; align-items: center; gap: 12px; padding: 10px 12px 10px 18px; }
  .hook + .hook { border-top: 1px solid var(--line); }
  .ev { width: 190px; flex: none; font-weight: 500; font-size: 13px; display: flex; flex-direction: column; }
  .m { font: 12px var(--font-mono); color: var(--ink-3); font-weight: 400; }
  code { flex: 1; min-width: 0; font: 12.5px var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; user-select: text; }
  .ro code { color: var(--ink-3); }
  .to, .sc { font-size: 12px; color: var(--ink-4); }
  .empty { margin: 0; padding: 14px 18px; color: var(--ink-4); font-size: 13px; }
  .form { display: grid; grid-template-columns: 110px 1fr; gap: 12px 16px; align-items: center; padding: 18px; }
  .form label { font-size: 13px; color: var(--ink-3); }
  .line { display: flex; align-items: center; gap: 10px; }
  .hint { font-size: 12px; color: var(--ink-4); }
  .mono { font: 13px var(--font-mono); }
  .num { width: 100px; }
  .actions { grid-column: 1 / -1; display: flex; justify-content: flex-end; }
</style>
