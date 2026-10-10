<script lang="ts">
  import Icon from './Icon.svelte';
  import { app, MODES, toast, type SessionState } from '../lib/store.svelte';
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
  const profile = $derived(app.settings?.profiles.find((p) => p.id === app.settings?.activeProfile));
  const modelLabel = $derived.by(() => {
    const m = session?.modelChoice ?? session?.model ?? app.settings?.defaultModel;
    if (profile && !session?.modelChoice) return profile.model || profile.name;
    if (!m) return 'Default model';
    const known = app.models?.find((x) => x.value === m || (x as any).resolvedModel === m);
    return known ? known.displayName.replace(/\s*\(.*\)$/, '') : m;
  });
  const canStop = $derived(!!session?.busy && !draft.trim());

  async function ensure(): Promise<SessionState> {
    if (session) return session;
    app.current = await call('session_new');
    return app.session(app.current);
  }

  async function setMode(id: string) {
    const s = await ensure();
    s.mode = id;
    if (s.running) await call('session_configure', { id: s.id, mode: id, model: null }).catch((e) => toast(String(e), 'error'));
    box?.focus();
  }

  async function setModel(value: string) {
    const s = await ensure();
    s.modelChoice = value;
    if (s.running) await call('session_configure', { id: s.id, mode: null, model: value }).catch((e) => toast(String(e), 'error'));
    box?.focus();
  }

  function modeMenu(e: MouseEvent) {
    openMenu(e.currentTarget as HTMLElement, [
      { header: 'Permissions' },
      ...MODES.map((m) => ({ label: m.label, desc: m.desc, checked: m.id === mode.id, run: () => setMode(m.id) })),
    ], { above: true, width: 300 });
  }

  function modelMenu(e: MouseEvent) {
    const current = session?.modelChoice ?? session?.model ?? 'default';
    const models = app.models ?? [
      { value: 'default', displayName: 'Default', description: 'The recommended model' },
      { value: 'opus', displayName: 'Opus', description: 'Most capable for complex work' },
      { value: 'sonnet', displayName: 'Sonnet', description: 'Fast and capable' },
      { value: 'haiku', displayName: 'Haiku', description: 'Fastest for quick answers' },
    ];
    openMenu(e.currentTarget as HTMLElement, profile
      ? [{ header: profile.name }, { label: profile.model || 'Server default', desc: profile.baseUrl, checked: true, run: () => {} }]
      : [{ header: 'Model' }, ...models.map((m) => ({ label: m.displayName.replace(/\s*\(.*\)$/, ''), desc: m.description, checked: current === m.value, run: () => setModel(m.value) }))],
      { above: true, width: 300 });
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
  <textarea bind:this={box} bind:value={draft} rows="1" {placeholder} spellcheck="true" onkeydown={key}></textarea>
  <div class="row">
    <button class="picker mode-{mode.id}" title="Permissions (Shift+Tab)" onclick={modeMenu}>
      <Icon name={mode.id === 'plan' ? 'plan' : mode.id === 'acceptEdits' ? 'pencil' : mode.id === 'bypassPermissions' ? 'bolt' : mode.id === 'auto' ? 'sparkle' : 'hand'} size={14} />
      {mode.short}<Icon name="chevron" size={12} />
    </button>
    <button class="picker" title="Model" onclick={modelMenu}><Icon name="cpu" size={14} />{modelLabel}<Icon name="chevron" size={12} /></button>
    <span class="grow"></span>
    <button class="send" class:stop={canStop} disabled={!canStop && !draft.trim()} title={canStop ? 'Stop (Esc)' : 'Send (Enter)'} onclick={submit}>
      <Icon name={canStop ? 'stop' : 'up'} size={16} />
    </button>
  </div>
</div>

<style>
  .composer {
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
  .send {
    width: 32px; height: 32px; border-radius: var(--r-button); display: grid; place-items: center;
    background: var(--blue); color: var(--blue-text); transition: background var(--t) var(--ease), opacity var(--t) var(--ease), transform 120ms var(--ease);
  }
  .send:hover { background: var(--blue-hover); }
  .send:active { transform: scale(0.94); }
  .send:disabled { background: var(--press); color: var(--ink-4); cursor: default; }
  .send.stop { background: var(--card); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line-strong); }
</style>
