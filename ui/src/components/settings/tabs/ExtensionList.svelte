<script lang="ts">
  import Section from '../Section.svelte';
  import Icon from '../../Icon.svelte';
  import { call, type Extension } from '../../../lib/ipc';
  import { toast } from '../../../lib/store.svelte';
  import { settingsWindow as sw } from '../../../lib/settings.svelte';

  let { kind }: { kind: 'skills' | 'agents' | 'commands' } = $props();
  const WHERE = {
    skills: { user: '~/.claude/skills/<name>/SKILL.md', project: '.claude/skills/<name>/SKILL.md', what: 'skill' },
    agents: { user: '~/.claude/agents/<name>.md', project: '.claude/agents/<name>.md', what: 'agent' },
    commands: { user: '~/.claude/commands/<name>.md', project: '.claude/commands/<name>.md', what: 'command' },
  } as const;
  const items = $derived(sw.ext?.[kind] ?? []);
  const groups = $derived([
    { scope: 'project' as const, title: 'This project', list: items.filter((i) => i.scope === 'project') },
    { scope: 'user' as const, title: 'Yours, in every project', list: items.filter((i) => i.scope === 'user') },
  ]);
  const open = (x: Extension) => call('open_file', { path: x.path }).catch((e) => toast(String(e), 'error'));
</script>

{#each groups as g}
  <Section title={g.title} caption="Files in {WHERE[kind][g.scope]}.">
    {#each g.list as x (x.path)}
      <div class="ext">
        <div class="t">
          <span class="n">{kind === 'commands' ? '/' : ''}{x.name}</span>
          <span class="d">{x.description || 'No description.'}</span>
        </div>
        <button class="btn sm" title={x.path} onclick={() => open(x)}><Icon name="external" size={13} />Open</button>
      </div>
    {:else}
      <p class="empty">No {WHERE[kind].what}s{g.scope === 'project' && !sw.cc.hasProject ? ' (no project open)' : ''}.</p>
    {/each}
  </Section>
{/each}
<button class="btn outline sm refresh" onclick={() => sw.refresh()}><Icon name="refresh" size={13} />Refresh</button>

<style>
  .ext { display: flex; align-items: center; gap: 12px; padding: 11px 12px 11px 18px; }
  .ext + .ext { border-top: 1px solid var(--line); }
  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .n { font-weight: 500; font-family: var(--font-mono); font-size: 13px; }
  .d { font-size: 13px; color: var(--ink-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { margin: 0; padding: 14px 18px; color: var(--ink-4); font-size: 13px; }
  .refresh { margin-top: 16px; }
</style>
