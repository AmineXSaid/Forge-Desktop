<script lang="ts">
  import Section from '../Section.svelte';
  import Field from '../Field.svelte';
  import Icon from '../../Icon.svelte';
  import { call, type McpServer } from '../../../lib/ipc';
  import { toast } from '../../../lib/store.svelte';
  import { effective, getPath, mergedList, scopeLabel } from '../../../lib/ccsettings';
  import { settingsWindow as sw } from '../../../lib/settings.svelte';

  const servers = $derived(sw.ext?.mcpServers ?? []);
  const projectServers = $derived(servers.filter((s) => s.scope === 'project'));
  const otherServers = $derived(servers.filter((s) => s.scope !== 'project'));
  const approved = $derived(mergedList(sw.cc, 'enabledMcpjsonServers'));
  const rejected = $derived(mergedList(sw.cc, 'disabledMcpjsonServers'));
  const allApproved = $derived(effective(sw.cc, 'enableAllProjectMcpServers').value === true);

  function status(s: McpServer): 'approved' | 'rejected' | 'pending' {
    if (rejected.some((r) => r.item === s.name)) return 'rejected';
    if (allApproved || approved.some((a) => a.item === s.name)) return 'approved';
    return 'pending';
  }
  const listIn = (key: string) => (getPath(sw.cc[sw.scope], key) as string[] | undefined) ?? [];

  async function approve(s: McpServer) {
    try {
      if (await call('mcp_approve', { scope: sw.scope, name: s.name })) toast(`${s.name} approved`);
      await sw.refresh();
    } catch (e) { toast(String(e), 'error'); }
  }
  async function reject(s: McpServer) {
    const mine = listIn('enabledMcpjsonServers').filter((n) => n !== s.name);
    if (listIn('enabledMcpjsonServers').includes(s.name)) await sw.set('enabledMcpjsonServers', mine.length ? mine : null);
    if (!listIn('disabledMcpjsonServers').includes(s.name)) await sw.set('disabledMcpjsonServers', [...listIn('disabledMcpjsonServers'), s.name]);
  }
  async function unreject(s: McpServer) {
    const rest = listIn('disabledMcpjsonServers').filter((n) => n !== s.name);
    await sw.set('disabledMcpjsonServers', rest.length ? rest : null);
  }
</script>

<Section title="This project's servers · .mcp.json" caption="Shared with the team. Claude Code starts one only after you approve it.">
  {#each projectServers as s (s.name)}
    {@const st = status(s)}
    <div class="srv">
      <span class="tr">{s.transport}</span>
      <div class="t"><span class="n">{s.name}</span><code title={s.target}>{s.target}</code></div>
      <span class="pill" class:working={st === 'approved'} class:error={st === 'rejected'} class:needs={st === 'pending'}>{st === 'approved' ? 'Approved' : st === 'rejected' ? 'Rejected' : 'Not approved'}</span>
      {#if st === 'rejected'}
        {#if listIn('disabledMcpjsonServers').includes(s.name)}<button class="btn sm" onclick={() => unreject(s)}>Undo</button>{/if}
      {:else}
        {#if st === 'pending'}<button class="btn ghost-blue sm" onclick={() => approve(s)}>Approve…</button>{/if}
        <button class="btn sm" onclick={() => reject(s)}>Reject</button>
      {/if}
    </div>
  {:else}
    <p class="empty">{sw.cc.hasProject ? 'This project has no .mcp.json servers.' : 'Open a project to see its servers.'}</p>
  {/each}
  {#if allApproved}
    <Field key="enableAllProjectMcpServers" type="bool" label="Approve every project server" desc="On: every server in .mcp.json starts without asking. Forge can turn this off, not on." />
  {/if}
</Section>

<Section title="Your servers" caption="From ~/.claude.json: user servers run in every project, local ones only in this one. Add them with claude mcp add.">
  {#each otherServers as s (s.scope + s.name)}
    <div class="srv">
      <span class="tr">{s.transport}</span>
      <div class="t"><span class="n">{s.name}</span><code title={s.target}>{s.target}</code></div>
      <span class="sc">{s.scope === 'user' ? 'User' : 'Local'}</span>
    </div>
  {:else}
    <p class="empty">None.</p>
  {/each}
</Section>

<Section title="Limits" caption="Environment variables, written to the scope above.">
  <Field key="env.MCP_TIMEOUT" type="env" label="Startup timeout" desc="Milliseconds a server may take to start." placeholder="10000" mono />
  <Field key="env.MCP_TOOL_TIMEOUT" type="env" label="Tool timeout" desc="Milliseconds one tool call may take." placeholder="Default" mono />
  <Field key="env.MAX_MCP_OUTPUT_TOKENS" type="env" label="Max output tokens" desc="The most tokens a tool's answer may hold." placeholder="25000" mono />
  <Field key="env.ENABLE_TOOL_SEARCH" type="env" label="Tool search" desc="Load MCP tools on demand: auto, auto:N, true or false." placeholder="auto" mono />
</Section>

<style>
  .srv { display: flex; align-items: center; gap: 12px; padding: 10px 12px 10px 18px; }
  .srv + .srv { border-top: 1px solid var(--line); }
  .tr { width: 44px; flex: none; font: 500 11px var(--font-mono); color: var(--ink-4); text-transform: uppercase; }
  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .n { font-weight: 500; }
  code { font: 12px var(--font-mono); color: var(--ink-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; user-select: text; }
  .sc { font-size: 12px; color: var(--ink-4); }
  .empty { margin: 0; padding: 14px 18px; color: var(--ink-4); font-size: 13px; }

</style>
