// Claude Code's settings files, as the Settings window sees them: three
// scopes, where the CLI applies local over project (shown as "Workspace") over
// user. Pure functions, so they are tested without a window.

import type { CcSettings, Scope } from './ipc';

export const SCOPES: { id: Scope; label: string; file: string }[] = [
  { id: 'user', label: 'User', file: '~/.claude/settings.json' },
  { id: 'project', label: 'Workspace', file: '.claude/settings.json' },
  { id: 'local', label: 'Local', file: '.claude/settings.local.json' },
];

/** Highest precedence first. */
export const PRECEDENCE: Scope[] = ['local', 'project', 'user'];

export const scopeLabel = (s: Scope) => SCOPES.find((x) => x.id === s)!.label;

/** A dotted key's value in one settings object; undefined when absent. */
export function getPath(obj: unknown, key: string): unknown {
  let at: any = obj;
  for (const part of key.split('.')) {
    if (at == null || typeof at !== 'object' || Array.isArray(at)) return undefined;
    at = at[part];
  }
  return at === null ? undefined : at;
}

export type Effective = { value: unknown; from: Scope | null };

/** The value the CLI uses, and the scope it comes from. */
export function effective(all: CcSettings, key: string): Effective {
  for (const s of PRECEDENCE) {
    const v = getPath(all[s], key);
    if (v !== undefined) return { value: v, from: s };
  }
  return { value: undefined, from: null };
}

/** Scopes that win over `scope` and set `key`, i.e. would hide a value written there. */
export function overriddenBy(all: CcSettings, key: string, scope: Scope): Scope | null {
  for (const s of PRECEDENCE) {
    if (s === scope) return null;
    if (getPath(all[s], key) !== undefined) return s;
  }
  return null;
}

/** List settings (permission rules, directories) are merged across scopes by the CLI. */
export function mergedList(all: CcSettings, key: string): { item: string; from: Scope }[] {
  const out: { item: string; from: Scope }[] = [];
  for (const s of PRECEDENCE) {
    const v = getPath(all[s], key);
    if (Array.isArray(v)) for (const item of v) if (typeof item === 'string') out.push({ item, from: s });
  }
  return out;
}

/** Environment variables, merged per variable: the winning value and its scope. */
export function mergedEnv(all: CcSettings): { name: string; value: string; from: Scope; shadowed: Scope[] }[] {
  const out = new Map<string, { name: string; value: string; from: Scope; shadowed: Scope[] }>();
  for (const s of PRECEDENCE) {
    const env = getPath(all[s], 'env');
    if (!env || typeof env !== 'object') continue;
    for (const [name, value] of Object.entries(env as Record<string, unknown>)) {
      const had = out.get(name);
      if (had) had.shadowed.push(s);
      else out.set(name, { name, value: String(value), from: s, shadowed: [] });
    }
  }
  return [...out.values()].sort((a, b) => a.name.localeCompare(b.name));
}

// ---------------------------------------------------------------- tabs and search

export type TabId =
  | 'general' | 'models' | 'providers' | 'permissions' | 'environment' | 'memory' | 'hooks'
  | 'mcp' | 'skills' | 'agents' | 'commands' | 'sandbox' | 'network' | 'about';

export const TABS: { id: TabId; label: string; icon: string; scoped: boolean }[] = [
  { id: 'general', label: 'General', icon: 'gear', scoped: true },
  { id: 'models', label: 'Models', icon: 'sparkle', scoped: true },
  { id: 'providers', label: 'Providers', icon: 'cpu', scoped: false },
  { id: 'permissions', label: 'Permissions', icon: 'shield', scoped: true },
  { id: 'environment', label: 'Environment', icon: 'terminal', scoped: true },
  { id: 'memory', label: 'Memory & Rules', icon: 'brain', scoped: true },
  { id: 'hooks', label: 'Hooks', icon: 'bolt', scoped: true },
  { id: 'mcp', label: 'MCP Servers', icon: 'plug', scoped: true },
  { id: 'skills', label: 'Skills', icon: 'book', scoped: false },
  { id: 'agents', label: 'Agents', icon: 'bot', scoped: false },
  { id: 'commands', label: 'Slash Commands', icon: 'slash', scoped: false },
  { id: 'sandbox', label: 'Sandbox', icon: 'box', scoped: true },
  { id: 'network', label: 'Network', icon: 'globe', scoped: true },
  { id: 'about', label: 'About', icon: 'info', scoped: false },
];

/** What the search box matches, per tab: setting labels plus the words someone would type. */
export const SETTINGS_INDEX: Record<TabId, readonly string[]> = {
  general: [
    'theme', 'dark', 'light', 'appearance', 'permission mode for new sessions', 'model for new sessions',
    'language', 'output style', 'respect .gitignore', 'gitignore', 'teammate mode', 'plans directory',
    'show turn duration', 'commit message', 'pr description', 'git attribution', 'co-authored-by',
    'cleanup period', 'chat history', 'updates channel', 'login method', 'api key helper',
  ],
  models: [
    'model', 'default model', 'opus', 'sonnet', 'haiku', 'always thinking', 'thinking', 'effort level', 'effort',
    'model routing', 'subagent model', 'max thinking tokens', 'max output tokens', 'tokens',
  ],
  providers: ['provider', 'endpoint', 'profile', 'api key', 'key', 'ollama', 'lm studio', 'openrouter', 'base url', 'gateway', 'claude account'],
  permissions: ['permission mode', 'default mode', 'deny', 'ask', 'allow', 'rules', 'permission rules', 'additional directories', 'directories', 'bypass'],
  environment: ['environment variables', 'env', 'variable', 'environment'],
  memory: ['memory', 'claude.md', 'claude.local.md', 'user memory', 'project memory', 'local memory', 'rules', 'instructions', 'announcements', 'company announcements'],
  hooks: ['hooks', 'hook', 'pretooluse', 'posttooluse', 'userpromptsubmit', 'stop', 'notification', 'sessionstart', 'lifecycle', 'disable all hooks'],
  mcp: ['mcp', 'mcp servers', 'server', '.mcp.json', 'approved servers', 'rejected servers', 'tool timeout', 'server timeout', 'auto-approve', 'tool search'],
  skills: ['skill', 'skills', 'skill.md'],
  agents: ['agent', 'agents', 'subagent', 'subagents'],
  commands: ['slash commands', 'commands', 'custom commands', 'slash'],
  sandbox: ['sandbox', 'isolation', 'excluded commands', 'unsandboxed', 'unix sockets', 'local binding', 'proxy port', 'socks'],
  network: ['proxy', 'http proxy', 'https proxy', 'no proxy', 'mtls', 'client certificate', 'certificate', 'client key', 'ca', 'tls'],
  about: ['about', 'version', 'claude code', 'shortcuts', 'keyboard', 'logs'],
};

/**
 * Tabs matching the query, best first: the tab's own name, then a setting
 * whose name equals or starts with the query, then any setting containing it.
 * An empty query returns every tab, in order.
 */
export function searchTabs<T extends { id: string; label: string }>(query: string, tabs: readonly T[]): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...tabs];
  return tabs
    .map((tab, order) => {
      const label = tab.label.toLowerCase();
      const words = SETTINGS_INDEX[tab.id as TabId] ?? [];
      let score = -1;
      if (label === q) score = 0;
      else if (label.startsWith(q)) score = 1;
      else if (words.some((w) => w === q)) score = 2;
      else if (label.includes(q) || words.some((w) => w.startsWith(q))) score = 3;
      else if (words.some((w) => w.includes(q))) score = 4;
      return { tab, score, order };
    })
    .filter((x) => x.score >= 0)
    .sort((a, b) => a.score - b.score || a.order - b.order)
    .map((x) => x.tab);
}
