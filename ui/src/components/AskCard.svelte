<script lang="ts">
  import Icon from './Icon.svelte';
  import { app, toast, type Item, type SessionState } from '../lib/store.svelte';
  import { call, type Answer } from '../lib/ipc';
  import { permissionTitle, relPath } from '../lib/tools';
  import { diffLines, renderDiff } from '../lib/diff';
  import { markdown, esc } from '../lib/markdown';

  let { item, session }: { item: Extract<Item, { kind: 'permission' }>; session: SessionState } = $props();

  const open = $derived(item.state === 'open');
  const question = $derived(item.tool === 'AskUserQuestion');
  const plan = $derived(item.tool === 'ExitPlanMode');
  const qs = $derived<{ question: string; header?: string; multiSelect?: boolean; options?: { label: string; description?: string }[] }[]>(item.input?.questions ?? []);
  const answered = $derived(qs.length > 0 && qs.every((q) => (item.answers[q.question] ?? []).length > 0));
  let busy = $state(false);

  const detail = $derived.by(() => {
    const i = item.input ?? {};
    if (plan) return `<div class="prose">${markdown(i.plan ?? '')}</div>`;
    if (item.tool === 'Bash') return `<pre class="cmd">$ ${esc(i.command ?? '')}</pre>`;
    if (item.tool === 'Edit' && typeof i.old_string === 'string') return renderDiff(diffLines(i.old_string, i.new_string ?? ''), 2);
    if (item.tool === 'MultiEdit' && Array.isArray(i.edits)) return i.edits.map((e: any) => renderDiff(diffLines(e.old_string ?? '', e.new_string ?? ''), 2)).join('');
    if (item.tool === 'Write' && typeof i.content === 'string') return renderDiff(diffLines('', i.content.split('\n').slice(0, 200).join('\n')), 0);
    if (item.tool === 'WebFetch' || item.tool === 'WebSearch') return '';
    return Object.keys(i).length ? `<pre>${esc(JSON.stringify(i, null, 2))}</pre>` : '';
  });
  const note = $derived(item.tool === 'Bash' ? (item.input?.description ?? null) : item.description && item.description !== relPath(item.input?.file_path, app.info?.workspace) ? item.description : null);

  function pick(q: (typeof qs)[number], label: string) {
    if (!open) return;
    const cur = item.answers[q.question] ?? [];
    item.answers[q.question] = q.multiSelect ? (cur.includes(label) ? cur.filter((x) => x !== label) : [...cur, label]) : [label];
  }

  async function respond(kind: Answer['kind']) {
    if (!open || busy) return;
    busy = true;
    const answer: Answer = { kind };
    if (kind === 'answer') answer.answers = Object.fromEntries(Object.entries(item.answers).map(([q, v]) => [q, v.join(', ')]));
    if (kind === 'deny' && plan) answer.message = 'The user wants to keep planning. Ask what to change.';
    try {
      await call('session_respond', { id: session.id, requestId: item.requestId, answer });
      item.state = 'answered';
      item.verdict = { allow: plan ? 'Approved' : 'Allowed', always: plan ? 'Approved, accepting edits' : 'Always allowed', deny: plan ? 'Kept planning' : question ? 'Skipped' : 'Denied', answer: 'Answered' }[kind];
      if (plan && kind !== 'deny') session.mode = kind === 'always' ? 'acceptEdits' : 'default';
    } catch (e) {
      toast(String(e), 'error');
    }
    busy = false;
  }

  function keys(e: KeyboardEvent) {
    if (!open || question || (e.target as HTMLElement)?.tagName === 'TEXTAREA') return;
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); respond('allow'); }
  }
</script>

<svelte:window onkeydown={keys} />

<div class="ask" class:done={!open}>
  <div class="head">
    {#if open}
      <span class="pill {plan ? 'plan' : 'needs'}">{plan ? 'Plan' : question ? 'Question' : 'Needs you'}</span>
    {:else}
      <span class="pill quiet"><Icon name={item.verdict.startsWith('Denied') || item.verdict === 'Skipped' || item.verdict === 'Kept planning' ? 'x' : 'check'} size={12} />{item.verdict}</span>
    {/if}
    <span class="title">{question ? (qs.length > 1 ? 'Claude has a few questions' : 'Claude has a question') : permissionTitle(item.tool, item.input, app.info?.workspace)}</span>
  </div>
  {#if note}<p class="note">{note}</p>{/if}

  {#if question}
    {#each qs as q}
      <div class="q">
        {#if q.header}<div class="qh">{q.header}</div>{/if}
        <div class="qt">{q.question}</div>
        <div class="opts">
          {#each q.options ?? [] as o}
            <button class="opt" class:on={(item.answers[q.question] ?? []).includes(o.label)} disabled={!open} onclick={() => pick(q, o.label)}>
              <span class="mark" class:multi={q.multiSelect}><Icon name="check" size={11} /></span>
              <span><span class="ol">{o.label}</span>{#if o.description}<span class="od">{o.description}</span>{/if}</span>
            </button>
          {/each}
        </div>
      </div>
    {/each}
  {:else if detail && open}
    <div class="detail" class:plan>{@html detail}</div>
  {/if}

  {#if open}
    <div class="actions">
      {#if question}
        <button class="btn" onclick={() => respond('deny')}>Skip</button>
        <span class="grow"></span>
        <button class="btn primary" disabled={!answered || busy} onclick={() => respond('answer')}>Submit</button>
      {:else}
        <button class="btn" onclick={() => respond('deny')}>{plan ? 'Keep planning' : 'Deny'}</button>
        <span class="grow"></span>
        {#if item.canAlwaysAllow}<button class="btn ghost-blue" disabled={busy} onclick={() => respond('always')}>{plan ? 'Approve and accept edits' : 'Always allow'}</button>{/if}
        <button class="btn primary" disabled={busy} onclick={() => respond('allow')}>{plan ? 'Approve' : 'Allow'}<kbd class="k">Ctrl ↵</kbd></button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .ask { background: var(--card); border-radius: var(--r-card); box-shadow: inset 0 0 0 1px var(--line-strong); animation: rise 180ms var(--ease); }
  .ask.done { box-shadow: inset 0 0 0 1px var(--line); }
  .head { display: flex; align-items: center; gap: 10px; padding: 14px 16px 0; }
  .done .head { padding-bottom: 12px; }
  .title { font-weight: 600; font-size: 15px; letter-spacing: -0.01em; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .done .title { font-weight: 500; color: var(--ink-3); }
  .note { margin: 6px 16px 0; color: var(--ink-3); font-size: 14px; }
  .detail { margin: 12px 16px 0; border-radius: var(--r-button); box-shadow: inset 0 0 0 1px var(--line); background: var(--inset); max-height: 300px; overflow: auto; }
  .detail.plan { background: var(--card); max-height: 420px; padding: 14px 16px; }
  .detail :global(pre) { margin: 0; padding: 10px 12px; font: 12.5px/1.6 var(--font-mono); white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
  .detail :global(.diff) { padding: 6px 0; }
  .actions { display: flex; align-items: center; gap: 8px; padding: 14px 16px 14px; }
  .grow { flex: 1; }
  .k { color: inherit; opacity: 0.7; margin-left: 4px; font-size: 11px; }
  .q { padding: 12px 16px 0; }
  .qh { font-size: 12px; font-weight: 500; color: var(--ink-4); margin-bottom: 2px; }
  .qt { font-size: 15px; font-weight: 500; margin-bottom: 10px; }
  .opts { display: flex; flex-direction: column; gap: 6px; }
  .opt {
    display: flex; align-items: flex-start; gap: 10px; text-align: left; padding: 10px 12px; border-radius: var(--r-button);
    box-shadow: inset 0 0 0 1px var(--line); transition: box-shadow var(--t) var(--ease), background var(--t) var(--ease);
  }
  .opt:hover:not(:disabled) { box-shadow: inset 0 0 0 1px var(--line-strong); }
  .opt.on { box-shadow: inset 0 0 0 1.5px var(--blue); background: var(--sky-tint); }
  .opt:disabled { cursor: default; }
  .mark { width: 16px; height: 16px; border-radius: 50%; box-shadow: inset 0 0 0 1.5px var(--ink-5); display: grid; place-items: center; color: transparent; flex: none; margin-top: 2px; }
  .mark.multi { border-radius: 4px; }
  .on .mark { background: var(--blue); box-shadow: none; color: var(--blue-text); }
  .ol { display: block; font-weight: 500; }
  .od { display: block; font-size: 13px; color: var(--ink-3); }
</style>
