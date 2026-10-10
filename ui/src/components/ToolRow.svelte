<script lang="ts">
  import Icon from './Icon.svelte';
  import { app, type Item } from '../lib/store.svelte';
  import { TOOL_ICON, toolArg, toolName } from '../lib/tools';
  import { diffLines, renderDiff, counts } from '../lib/diff';
  import { esc } from '../lib/markdown';

  let { tool, live }: { tool: Extract<Item, { kind: 'tool' }>; live: boolean } = $props();

  const arg = $derived(toolArg(tool.name, tool.input, app.info?.workspace));
  const edit = $derived(['Edit', 'MultiEdit', 'Write'].includes(tool.name));
  const ops = $derived.by(() => {
    const i = tool.input ?? {};
    if (tool.name === 'Write' && typeof i.content === 'string') return diffLines('', i.content.split('\n').slice(0, 400).join('\n'));
    if (tool.name === 'Edit' && typeof i.old_string === 'string') return diffLines(i.old_string, i.new_string ?? '');
    if (tool.name === 'MultiEdit' && Array.isArray(i.edits)) return i.edits.flatMap((e: any) => diffLines(e.old_string ?? '', e.new_string ?? ''));
    return null;
  });
  const c = $derived(ops ? counts(ops) : null);

  const detail = $derived.by(() => {
    if (!tool.open) return '';
    const i = tool.input ?? {};
    const parts: string[] = [];
    if (ops) parts.push(renderDiff(ops, tool.name === 'Write' ? 0 : 2));
    else if (tool.name === 'Bash') parts.push(`<pre class="cmd">$ ${esc(i.command ?? '')}</pre>`);
    else if (tool.name === 'TodoWrite') parts.push(`<pre>${esc((i.todos ?? []).map((t: any) => `${t.status === 'completed' ? '✓' : t.status === 'in_progress' ? '•' : '○'}  ${t.content}`).join('\n'))}</pre>`);
    else if (Object.keys(i).length) parts.push(`<pre>${esc(JSON.stringify(i, null, 2))}</pre>`);
    if (tool.result != null && !(edit && !tool.isError)) {
      const r = tool.result.length > 20000 ? `${tool.result.slice(0, 20000)}\n… ${tool.result.length - 20000} more characters` : tool.result;
      parts.push(`<pre class="out${tool.isError ? ' err' : ''}">${esc(r)}</pre>`);
    }
    return parts.join('');
  });
</script>

<div class="tool" class:open={tool.open}>
  <button class="head" onclick={() => (tool.open = !tool.open)} aria-expanded={tool.open}>
    <span class="ic"><Icon name={TOOL_ICON[tool.name] ?? 'cpu'} size={14} /></span>
    <span class="name">{toolName(tool.name, tool.done || !live)}</span>
    <span class="arg" class:mono={tool.name === 'Bash' || tool.name === 'Grep' || tool.name === 'Glob'}>{arg}</span>
    {#if c && !live}<span class="counts"><span class="add">+{c.add}</span>{#if c.del}<span class="del">−{c.del}</span>{/if}</span>{/if}
    {#if live && !tool.done}<span class="spinner"></span>
    {:else if tool.isError}<span class="pill error">Failed</span>{/if}
    <span class="chev"><Icon name="chevron" size={12} /></span>
  </button>
  {#if tool.open}<div class="body">{@html detail}</div>{/if}
</div>

<style>
  .tool:not(:first-child) { border-top: 1px solid var(--line); }
  .head { display: flex; align-items: center; gap: 10px; width: 100%; min-height: 38px; padding: 7px 12px 7px 14px; text-align: left; }
  .head:hover { background: var(--hover); }
  .ic { color: var(--ink-4); display: grid; }
  .name { font-weight: 500; font-size: 14px; color: var(--ink); flex: none; }
  .arg { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--ink-3); font-size: 14px; }
  .arg.mono { font: 13px var(--font-mono); }
  .chev { color: var(--ink-4); display: grid; transition: transform var(--t) var(--ease); transform: rotate(-90deg); }
  .open .chev { transform: none; }
  .body { border-top: 1px solid var(--line); max-height: 420px; overflow: auto; background: var(--inset); }
  .body :global(pre) { margin: 0; padding: 10px 14px; font: 12.5px/1.6 var(--font-mono); white-space: pre-wrap; overflow-wrap: anywhere; color: var(--ink-2); user-select: text; }
  .body :global(pre + pre) { border-top: 1px solid var(--line); }
  .body :global(pre.err) { color: var(--status-error-fg); }
  .body :global(.diff) { padding: 6px 0; }
</style>
