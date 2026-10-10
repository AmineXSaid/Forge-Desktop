<script lang="ts">
  import Icon from './Icon.svelte';
  import ToolRow from './ToolRow.svelte';
  import AskCard from './AskCard.svelte';
  import Composer from './Composer.svelte';
  import { app, type Item, type SessionState } from '../lib/store.svelte';
  import { markdown } from '../lib/markdown';
  import { call } from '../lib/ipc';

  let { session }: { session: SessionState } = $props();

  // Long threads show the newest items first; older ones on request.
  const PAGE = 200;
  let shown = $state(PAGE);
  $effect(() => { session.id; shown = PAGE; });

  type Segment = { key: string; tools?: Extract<Item, { kind: 'tool' }>[]; item?: Item };
  const segments = $derived.by(() => {
    const items = session.items.slice(-shown);
    const out: Segment[] = [];
    for (const it of items) {
      if (it.kind === 'text' && !it.text.trim() && !it.live) continue;
      if (it.kind === 'tool') {
        const last = out.at(-1);
        if (last?.tools) last.tools.push(it);
        else out.push({ key: `g-${it.key}`, tools: [it] });
      } else out.push({ key: it.key, item: it });
    }
    return out;
  });
  const hidden = $derived(Math.max(0, session.items.length - shown));
  const title = $derived(app.list.find((r) => r.id === session.id)?.title ?? session.title ?? session.firstPrompt ?? '');
  const waitingOnModel = $derived(session.busy && !session.items.some((i) => (i.kind === 'text' && i.live) || (i.kind === 'permission' && i.state === 'open')));

  // Stay pinned to the bottom while the reply streams, unless the reader scrolled up.
  let scroller = $state<HTMLDivElement>();
  let content = $state<HTMLDivElement>();
  let pinned = true;
  let scrolled = $state(false);
  function onScroll() {
    if (!scroller) return;
    pinned = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 60;
    scrolled = scroller.scrollTop > 4;
  }
  $effect(() => {
    if (!content || !scroller) return;
    const s = scroller;
    const ro = new ResizeObserver(() => { if (pinned) s.scrollTop = s.scrollHeight; });
    ro.observe(content);
    s.scrollTop = s.scrollHeight;
    return () => ro.disconnect();
  });

  function link(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest('a[data-href]') as HTMLAnchorElement | null;
    if (a) { e.preventDefault(); call('open_url', { url: a.dataset.href! }); }
  }
</script>

<div class="thread">
  <header class="head" class:scrolled>
    <span class="title">{title}</span>
    {#if session.status === 'working'}<span class="pill working">Working</span>{/if}
    <span class="grow"></span>
    {#if session.model}<span class="model">{session.model}</span>{/if}
  </header>

  <div class="scroll" bind:this={scroller} onscroll={onScroll} onclick={link} role="presentation">
    <div class="column" bind:this={content}>
      {#if hidden}<button class="earlier" onclick={() => (shown += PAGE)}>Show {Math.min(hidden, PAGE)} earlier</button>{/if}
      {#if session.loaded === 'loading'}<div class="loading"><span class="spinner"></span></div>{/if}
      {#each segments as seg (seg.key)}
        {#if seg.tools}
          <div class="tools card">{#each seg.tools as t (t.key)}<ToolRow tool={t} live={session.running} />{/each}</div>
        {:else if seg.item}
          {@const it = seg.item}
          {#if it.kind === 'user'}
            <div class="user">{it.text}</div>
          {:else if it.kind === 'text'}
            <div class="prose" class:live={it.live && session.busy}>{@html markdown(it.text)}</div>
          {:else if it.kind === 'thinking'}
            <details class="thinking" bind:open={it.open}>
              <summary><Icon name="brain" size={14} />{it.live && session.busy ? 'Thinking…' : 'Thought process'}<Icon name="chevron" size={12} /></summary>
              <div class="think">{it.text}</div>
            </details>
          {:else if it.kind === 'permission'}
            <AskCard item={it} {session} />
          {:else if it.kind === 'notice'}
            <div class="notice {it.tone}"><Icon name={it.tone === 'error' || it.tone === 'warn' ? 'alert' : 'info'} size={14} /><div>{it.text}{#if it.detail}<pre>{it.detail}</pre>{/if}</div></div>
          {:else if it.kind === 'meta'}
            <div class="meta">{((it.durationMs ?? 0) / 1000).toFixed(1)}s{#if it.costUsd}<span class="dot">·</span>${it.costUsd.toFixed(4)}{/if}</div>
          {/if}
        {/if}
      {/each}
      {#if waitingOnModel}<div class="working"><span class="dots"><i></i><i></i><i></i></span></div>{/if}
    </div>
  </div>

  <div class="dock"><div class="dock-inner"><Composer {session} /></div></div>
</div>

<style>
  .thread { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .head { height: 44px; flex: none; display: flex; align-items: center; gap: 10px; padding: 0 20px 0 24px; border-bottom: 1px solid transparent; transition: border-color var(--t) var(--ease); }
  .head.scrolled { border-bottom-color: var(--line); }
  .title { font-weight: 600; font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .grow { flex: 1; }
  .model { font: 12px var(--font-mono); color: var(--ink-4); }
  .scroll { flex: 1; overflow-y: auto; overflow-x: hidden; }
  .column { max-width: var(--column); margin: 0 auto; padding: 12px 32px 32px; display: flex; flex-direction: column; gap: 16px; }
  .user {
    align-self: flex-end; max-width: 85%; background: var(--card); box-shadow: inset 0 0 0 1px var(--line); border-radius: var(--r-card);
    padding: 10px 14px; font-size: 15px; line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; user-select: text;
  }
  .live > :global(:last-child)::after { content: ''; display: inline-block; width: 8px; height: 8px; margin-left: 4px; border-radius: 50%; background: var(--blue); animation: pulse 1s ease-in-out infinite; vertical-align: 1px; }
  @keyframes pulse { 50% { opacity: 0.25; } }
  .thinking { color: var(--ink-3); font-size: 14px; }
  .thinking summary { list-style: none; display: inline-flex; align-items: center; gap: 7px; cursor: pointer; padding: 2px 0; }
  .thinking summary::-webkit-details-marker { display: none; }
  .thinking summary :global(.icon:last-child) { transform: rotate(-90deg); transition: transform var(--t) var(--ease); }
  .thinking[open] summary :global(.icon:last-child) { transform: none; }
  .think { margin-top: 8px; padding-left: 14px; border-left: 2px solid var(--line); white-space: pre-wrap; font: italic 400 14px/1.6 var(--font-sans); color: var(--graphite); user-select: text; }
  .tools { overflow: hidden; }
  .notice { display: flex; gap: 10px; align-items: flex-start; padding: 10px 14px; border-radius: var(--r-card); background: var(--hover); color: var(--ink-2); font-size: 14px; }
  .notice.error { background: var(--status-error-bg); color: var(--status-error-fg); }
  .notice.warn { background: var(--status-needs-bg); color: var(--status-needs-fg); }
  .notice :global(.icon) { margin-top: 2px; }
  .notice pre { margin: 6px 0 0; white-space: pre-wrap; font: 12px/1.5 var(--font-mono); }
  .meta { font-size: 12px; color: var(--ink-4); margin-top: -8px; }
  .meta .dot { margin: 0 6px; }
  .earlier { align-self: center; font-size: 13px; color: var(--ink-3); padding: 6px 12px; border-radius: var(--r-pill); }
  .earlier:hover { background: var(--hover); }
  .loading { display: grid; place-items: center; padding: 40px; }
  .working { padding: 4px 0; }
  .dots { display: inline-flex; gap: 5px; }
  .dots i { width: 6px; height: 6px; border-radius: 50%; background: var(--ink-4); animation: bounce 1.2s ease-in-out infinite; }
  .dots i:nth-child(2) { animation-delay: 0.15s; } .dots i:nth-child(3) { animation-delay: 0.3s; }
  @keyframes bounce { 0%, 60%, 100% { transform: none; opacity: 0.5; } 30% { transform: translateY(-4px); opacity: 1; } }
  .dock { flex: none; padding: 0 32px 18px; }
  .dock-inner { max-width: calc(var(--column) - 64px); margin: 0 auto; }
</style>
