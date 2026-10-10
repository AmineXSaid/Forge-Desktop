<script lang="ts">
  import Icon from './Icon.svelte';
  import ModelPicker from './ModelPicker.svelte';
  import { app, MODES, THINKING, toast, type SessionState } from '../lib/store.svelte';
  import type { Thinking } from '../lib/ipc';
  import { call } from '../lib/ipc';
  import { openMenu } from '../lib/menu.svelte';

  let { session, placeholder = 'Reply…', autofocus = false }: { session: SessionState | null; placeholder?: string; autofocus?: boolean } = $props();

  let draft = $state('');
  let box = $state<HTMLTextAreaElement>();

  // Each session keeps its own unsent text.
  $effect(() => { draft = session?.draft ?? ''; });
  $effect(() => { if (session) session.draft = draft; });
  $effect(() => { draft; if (box) { box.style.height = 'auto'; box.style.height = `${Math.min(box.scrollHeight, innerHeight * 0.4)}px`; } });
  $effect(() => { if (autofocus) box?.focus(); });

  // The mode the CLI reports, never a guess: an unknown one shows by name.
  const mode = $derived.by(() => {
    const id = session?.mode ?? app.settings?.defaultMode ?? 'default';
    return MODES.find((m) => m.id === id) ?? { id, label: id, short: id, desc: '' };
  });
  const route = $derived(app.routeOf(session));
  const profile = $derived(app.settings?.profiles.find((p) => p.id === route));
  const modelLabel = $derived.by(() => {
    if (profile) return session?.modelChoice ?? (profile.model || profile.name);
    const m = session?.modelChoice ?? session?.model ?? app.settings?.defaultModel;
    if (!m) return 'Default model';
    const known = app.models?.find((x) => x.value === m || (x as any).resolvedModel === m);
    return known ? known.displayName.replace(/\s*\(.*\)$/, '') : m;
  });
  const canStop = $derived(!!session?.busy && !draft.trim());

  async function ensure(): Promise<SessionState> {
    if (session) return session;
    app.current = app.draft ?? (await call('session_new'));
    app.draft = null;
    return app.session(app.current);
  }

  const thinking = $derived(THINKING.find((t) => t.id === (session?.thinking ?? 'auto'))!);
  const ctx = $derived(session?.context ?? null);
  const fmt = (n: number) => (n >= 1000 ? `${Math.round(n / 1000)}k` : String(n));

  async function setThinking(id: Thinking) {
    const s = await ensure();
    s.thinking = id;
    if (s.running) await call('session_configure', { id: s.id, mode: null, model: null, thinking: id }).catch((e) => toast(String(e), 'error'));
    box?.focus();
  }

  function thinkingMenu(e: MouseEvent) {
    openMenu(e.currentTarget as HTMLElement, [
      { header: 'Thinking' },
      ...THINKING.map((t) => ({ label: t.label, desc: t.desc, checked: t.id === thinking.id, run: () => setThinking(t.id) })),
    ], { above: true, width: 240 });
  }

  async function setMode(id: string) {
    const s = await ensure();
    s.mode = id;
    if (s.running) await call('session_configure', { id: s.id, mode: id, model: null }).catch((e) => toast(String(e), 'error'));
    box?.focus();
  }

  function modeMenu(e: MouseEvent) {
    openMenu(e.currentTarget as HTMLElement, [
      { header: 'Permissions' },
      ...MODES.map((m) => ({ label: m.label, desc: m.desc, checked: m.id === mode.id, run: () => setMode(m.id) })),
    ], { above: true, width: 300 });
  }

  async function pick(r: string, model: string) {
    app.pickerOpen = false;
    const s = await ensure();
    await app.pickModel(s, r, model);
    box?.focus();
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      submit();
    } else if (e.key === 'Tab' && e.shiftKey) {
      e.preventDefault();
      const order = ['default', 'acceptEdits', 'plan'];
      setMode(order[(order.indexOf(mode.id) + 1) % order.length]);
    }
  }

  function submit() {
    if (canStop) return app.stop();
    const text = draft;
    if (!text.trim()) return;
    draft = '';
    app.send(text);
  }

  export function fill(text: string) { draft = text; box?.focus(); }
</script>

<div class="composer">
  {#if app.pickerOpen}
    <ModelPicker current={{ route, model: session?.modelChoice ?? profile?.model ?? null }} onpick={pick} onclose={() => (app.pickerOpen = false)} />
  {/if}
  <textarea bind:this={box} bind:value={draft} rows="1" {placeholder} spellcheck="true" onkeydown={key} onfocus={() => app.prewarm()}></textarea>
  <div class="row">
    <button class="picker mode-{mode.id}" title="Permissions (Shift+Tab)" onclick={modeMenu}>
      <Icon name={mode.id === 'plan' ? 'plan' : mode.id === 'acceptEdits' ? 'pencil' : mode.id === 'bypassPermissions' ? 'bolt' : mode.id === 'auto' ? 'sparkle' : 'hand'} size={14} />
      {mode.short}<Icon name="chevron" size={12} />
    </button>
    <button class="picker model-pill" title="Model (Ctrl+/)" onclick={() => (app.pickerOpen = !app.pickerOpen)}><Icon name="cpu" size={14} />{#if profile}<span class="via">{profile.name}</span>{/if}{modelLabel}<Icon name="chevron" size={12} /></button>
    <button class="picker" class:on={thinking.id !== 'auto'} title="Thinking" onclick={thinkingMenu}><Icon name="brain" size={14} />{thinking.id === 'auto' ? 'Thinking' : thinking.label}<Icon name="chevron" size={12} /></button>
    <span class="grow"></span>
    {#if ctx}
      <span class="ctx" class:high={ctx.percent >= 80} title="Context: {ctx.used.toLocaleString()} of {ctx.max.toLocaleString()} tokens ({Math.round(ctx.percent)}%)">
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6" class="track" /><circle cx="8" cy="8" r="6" class="fill" stroke-dasharray="{(Math.min(100, ctx.percent) / 100) * 37.7} 37.7" /></svg>
        {fmt(ctx.used)} / {fmt(ctx.max)}
      </span>
    {/if}
    {#if session?.state === 'warming'}<span class="warm" title="Starting Claude Code"><span class="spinner"></span></span>{/if}
    <button class="send" class:stop={canStop} disabled={!canStop && !draft.trim()} title={canStop ? 'Stop (Esc)' : 'Send (Enter)'} onclick={submit}>
      <Icon name={canStop ? 'stop' : 'up'} size={16} />
    </button>
  </div>
</div>

<style>
  .composer {
    position: relative;
    background: var(--card); border-radius: var(--r-card); box-shadow: inset 0 0 0 1px var(--line-strong);
    transition: box-shadow var(--t) var(--ease);
  }
  .composer:focus-within { box-shadow: inset 0 0 0 1px var(--blue), 0 0 0 3px var(--focus); }
  textarea {
    display: block; width: 100%; border: 0; outline: 0; resize: none; background: none; padding: 14px 16px 4px;
    font-size: 15px; line-height: 1.5; min-height: 50px; max-height: 40vh; color: var(--ink); user-select: text;
  }
  textarea::placeholder { color: var(--ink-4); }
  .row { display: flex; align-items: center; gap: 2px; padding: 6px 8px 8px 8px; }
  .grow { flex: 1; }
  .picker {
    height: 28px; padding: 0 9px; border-radius: var(--r-pill); display: inline-flex; align-items: center; gap: 6px;
    font-size: 13px; font-weight: 500; color: var(--ink-3); transition: background var(--t) var(--ease), color var(--t) var(--ease);
  }
  .picker:hover { background: var(--hover); color: var(--ink); }
  .picker.mode-plan { background: var(--status-plan-bg); color: var(--status-plan-fg); }
  .picker.mode-acceptEdits { color: var(--add-fg); }
  .picker.mode-bypassPermissions { background: var(--status-error-bg); color: var(--status-error-fg); }
  .picker.on { color: var(--ink); }
  .via { color: var(--ink-4); font-weight: 400; }
  .via::after { content: ' ·'; }
  .ctx { display: inline-flex; align-items: center; gap: 6px; font: 500 12px var(--font-mono); color: var(--ink-4); margin-right: 8px; }
  .ctx svg { transform: rotate(-90deg); }
  .ctx circle { fill: none; stroke-width: 2.2; }
  .ctx .track { stroke: var(--line-strong); }
  .ctx .fill { stroke: var(--ink-3); stroke-linecap: round; }
  .ctx.high { color: var(--status-needs-fg); }
  .ctx.high .fill { stroke: var(--marigold); }
  .warm { display: grid; place-items: center; width: 24px; margin-right: 4px; }
  .send {
    width: 32px; height: 32px; border-radius: var(--r-button); display: grid; place-items: center;
    background: var(--blue); color: var(--blue-text); transition: background var(--t) var(--ease), opacity var(--t) var(--ease), transform 120ms var(--ease);
  }
  .send:hover { background: var(--blue-hover); }
  .send:active { transform: scale(0.94); }
  .send:disabled { background: var(--press); color: var(--ink-4); cursor: default; }
  .send.stop { background: var(--card); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line-strong); }
</style>
