// App state. One reducer (`apply`) turns SessionEvents, live or replayed from
// disk, into the items a thread shows. Svelte's fine-grained reactivity means
// a streamed delta re-renders only the block it lands in.

import { SvelteMap } from 'svelte/reactivity';
import { call, type Envelope, type GitStatus, type Info, type SessionEvent, type Settings, type Summary } from './ipc';

export type Item =
  | { kind: 'user'; key: string; text: string }
  | { kind: 'text'; key: string; text: string; live: boolean }
  | { kind: 'thinking'; key: string; text: string; live: boolean; open: boolean }
  | { kind: 'tool'; key: string; id: string; name: string; input: any; result: string | null; isError: boolean; done: boolean; open: boolean }
  | {
      kind: 'permission';
      key: string;
      requestId: string;
      tool: string;
      input: any;
      description: string | null;
      canAlwaysAllow: boolean;
      state: 'open' | 'answered' | 'closed';
      verdict: string;
      answers: Record<string, string[]>;
    }
  | { kind: 'notice'; key: string; tone: string; text: string; detail?: string }
  | { kind: 'meta'; key: string; durationMs: number | null; costUsd: number | null };

export type Status = 'idle' | 'working' | 'needs' | 'error';

export const MODES = [
  { id: 'default', label: 'Ask before acting', short: 'Ask', desc: 'Asks before editing files or running commands' },
  { id: 'acceptEdits', label: 'Accept edits', short: 'Accept edits', desc: 'Edits go through; commands still ask' },
  { id: 'plan', label: 'Plan', short: 'Plan', desc: 'Explores and proposes a plan without changing anything' },
  { id: 'auto', label: 'Auto', short: 'Auto', desc: 'Claude Code decides what is safe to run without asking' },
  { id: 'bypassPermissions', label: 'Bypass permissions', short: 'Bypass', desc: 'Runs everything without asking' },
] as const;

let seq = 0;
const key = () => `k${++seq}`;

export class SessionState {
  id: string;
  items: Item[] = $state([]);
  busy = $state(false);
  running = $state(false);
  failed = $state(false);
  loaded = $state<'no' | 'loading' | 'yes'>('no');
  model = $state<string | null>(null);
  modelChoice = $state<string | null>(null);
  mode = $state('default');
  draft = $state('');
  title = $state<string | null>(null);
  stopping = false;
  // Lookups into `items` (the same proxied objects), not reactive themselves.
  private blocks = new Map<string, Extract<Item, { kind: 'text' | 'thinking' }>>();
  private tools = new Map<string, Extract<Item, { kind: 'tool' }>>();

  constructor(id: string, mode: string) {
    this.id = id;
    this.mode = mode;
  }

  get status(): Status {
    if (this.items.some((i) => i.kind === 'permission' && i.state === 'open')) return 'needs';
    if (this.busy) return 'working';
    if (this.failed) return 'error';
    return 'idle';
  }

  get firstPrompt(): string | null {
    const u = this.items.find((i) => i.kind === 'user');
    return u ? u.text.split('\n')[0].slice(0, 80) : null;
  }

  private push<T extends Item>(item: T): T {
    this.items.push(item);
    return this.items[this.items.length - 1] as T;
  }

  addUser(text: string) {
    this.push({ kind: 'user', key: key(), text });
    this.busy = true;
    this.failed = false;
  }

  apply(e: SessionEvent) {
    switch (e.kind) {
      case 'started':
        this.running = true;
        this.model = e.model;
        if (e.mode) this.mode = e.mode;
        break;
      case 'user':
        this.push({ kind: 'user', key: key(), text: e.text });
        break;
      case 'textDelta':
      case 'thinkingDelta': {
        let b = this.blocks.get(e.block);
        if (!b) {
          b = e.kind === 'textDelta'
            ? this.push({ kind: 'text', key: e.block, text: '', live: true })
            : this.push({ kind: 'thinking', key: e.block, text: '', live: true, open: false });
          this.blocks.set(e.block, b);
        }
        b.text += e.text;
        break;
      }
      case 'text':
        this.blocks.set(e.block, this.push({ kind: 'text', key: e.block, text: e.text, live: false }));
        break;
      case 'thinking':
        this.blocks.set(e.block, this.push({ kind: 'thinking', key: e.block, text: e.text, live: false, open: false }));
        break;
      case 'blockDone': {
        const b = this.blocks.get(e.block);
        if (b) b.live = false;
        break;
      }
      case 'tool': {
        const t = this.tools.get(e.id);
        if (t) {
          if (e.done) t.input = e.input;
          t.name = e.name || t.name;
        } else {
          this.tools.set(e.id, this.push({ kind: 'tool', key: e.id, id: e.id, name: e.name, input: e.input ?? {}, result: null, isError: false, done: false, open: false }));
        }
        break;
      }
      case 'toolResult': {
        const t = this.tools.get(e.id);
        if (t) {
          t.result = e.output;
          t.isError = e.isError;
          t.done = true;
        }
        break;
      }
      case 'permission':
        this.push({
          kind: 'permission', key: e.requestId, requestId: e.requestId, tool: e.tool, input: e.input ?? {},
          description: e.description, canAlwaysAllow: e.canAlwaysAllow, state: 'open', verdict: '', answers: {},
        });
        break;
      case 'permissionClosed': {
        const p = this.items.find((i) => i.kind === 'permission' && i.requestId === e.requestId);
        if (p && p.kind === 'permission' && p.state === 'open') {
          p.state = 'closed';
          p.verdict = 'Cancelled';
        }
        break;
      }
      case 'turnEnd':
        this.busy = false;
        this.finishLive();
        if (!e.ok) {
          this.failed = true;
          this.push({ kind: 'notice', key: key(), tone: 'error', text: e.error ?? 'The turn ended with an error.' });
        } else if (e.durationMs != null) {
          this.push({ kind: 'meta', key: key(), durationMs: e.durationMs, costUsd: e.costUsd });
        }
        break;
      case 'notice':
        this.push({ kind: 'notice', key: key(), tone: e.tone, text: e.text });
        break;
      case 'capabilities':
        app.capabilities(e.models, e.commands);
        break;
      case 'exited':
        this.running = false;
        this.busy = false;
        this.finishLive();
        for (const i of this.items) {
          if (i.kind === 'permission' && i.state === 'open') { i.state = 'closed'; i.verdict = 'Session ended'; }
        }
        if (e.code && !this.stopping) {
          this.failed = true;
          this.push({ kind: 'notice', key: key(), tone: 'error', text: `Claude Code stopped (exit code ${e.code}).`, detail: e.stderr });
        }
        this.stopping = false;
        break;
    }
  }

  private finishLive() {
    for (const i of this.items) {
      if ((i.kind === 'text' || i.kind === 'thinking') && i.live) i.live = false;
      if (i.kind === 'tool' && !i.done) i.done = true;
    }
  }
}

export type Model = { value: string; displayName: string; description?: string };

class App {
  info = $state<Info | null>(null);
  settings = $state<Settings | null>(null);
  list = $state<Summary[]>([]);
  sessions = new SvelteMap<string, SessionState>();
  current = $state<string | null>(null);
  view = $state<'chat' | 'settings'>('chat');
  settingsTab = $state('general');
  sidebar = $state(true);
  changes = $state(false);
  git = $state<GitStatus>({ isRepo: false, branch: null, files: [] });
  models = $state<Model[] | null>(null);
  commands = $state<{ name: string; description: string }[]>([]);
  search = $state('');
  maximized = $state(false);
  dark = $state(false);

  session(id: string): SessionState {
    let s = this.sessions.get(id);
    if (!s) {
      s = new SessionState(id, this.settings?.defaultMode ?? 'default');
      this.sessions.set(id, s);
    }
    return s;
  }

  get currentSession(): SessionState | null {
    return this.current ? (this.sessions.get(this.current) ?? null) : null;
  }

  capabilities(models: unknown, commands: unknown) {
    if (Array.isArray(models)) this.models = models as Model[];
    if (Array.isArray(commands)) this.commands = commands as { name: string; description: string }[];
  }

  onBatch = (batch: Envelope[]) => {
    for (const env of batch) this.session(env.session).apply(env.event);
    if (batch.some((e) => e.event.kind === 'turnEnd')) {
      this.refreshList();
      this.refreshGit();
    }
  };

  /** Sessions in the sidebar: saved ones, plus ones started here that have no file yet. */
  get rows(): Summary[] {
    const rows = [...this.list];
    for (const s of this.sessions.values()) {
      if (s.items.length && !rows.some((r) => r.id === s.id)) rows.unshift({ id: s.id, title: s.title ?? s.firstPrompt ?? 'New session', updated: Date.now(), branch: null });
    }
    const q = this.search.trim().toLowerCase();
    return q ? rows.filter((r) => r.title.toLowerCase().includes(q)) : rows;
  }

  async refreshList() {
    try { this.list = await call('sessions_list'); } catch (e) { console.error(e); }
  }

  async refreshGit() {
    try {
      const st = await call('git_status');
      if (JSON.stringify(st) !== JSON.stringify(this.git)) this.git = st;
    } catch (e) { console.error(e); }
  }

  async open(id: string) {
    this.view = 'chat';
    this.current = id;
    const s = this.session(id);
    if (s.loaded === 'no' && !s.items.length) {
      s.loaded = 'loading';
      try {
        for (const e of await call('session_load', { id })) s.apply(e);
      } catch (e) { toast(String(e), 'error'); }
      s.loaded = 'yes';
    }
  }

  newSession() {
    this.view = 'chat';
    this.current = null;
  }

  async send(text: string) {
    const t = text.trim();
    if (!t) return;
    if (!this.current) this.current = await call('session_new');
    const s = this.session(this.current);
    s.loaded = 'yes';
    s.addUser(t);
    s.draft = '';
    try {
      await call('session_send', { id: s.id, text: t, model: s.modelChoice, mode: s.mode });
    } catch (e) {
      s.busy = false;
      s.failed = true;
      s.apply({ kind: 'notice', tone: 'error', text: String(e) });
    }
  }

  async stop() {
    const s = this.currentSession;
    if (s?.busy) await call('session_interrupt', { id: s.id }).catch((e) => toast(String(e), 'error'));
  }

  async saveSettings(patch: Partial<Settings>) {
    if (!this.settings) return;
    try {
      this.settings = await call('settings_save', { settings: { ...this.settings, ...patch } });
    } catch (e) { toast(String(e), 'error'); }
  }
}

export const app = new App();

// ---------------------------------------------------------------- toasts
export type Toast = { id: number; text: string; tone: '' | 'error' };
export const toasts = $state<Toast[]>([]);
let toastSeq = 0;
export function toast(text: string, tone: '' | 'error' = '') {
  const id = ++toastSeq;
  toasts.push({ id, text, tone });
  setTimeout(() => {
    const i = toasts.findIndex((t) => t.id === id);
    if (i >= 0) toasts.splice(i, 1);
  }, tone === 'error' ? 6000 : 3000);
}
