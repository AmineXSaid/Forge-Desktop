// The contract with Rust (src-tauri/src/desktop.rs, crates/forge-core/src/events.rs).
// `cargo test` checks that every command and every SessionEvent kind is named here.

export type SessionEvent =
  | { kind: 'started'; model: string | null; mode: string | null }
  | { kind: 'user'; text: string }
  | { kind: 'textDelta'; block: string; text: string }
  | { kind: 'thinkingDelta'; block: string; text: string }
  | { kind: 'text'; block: string; text: string }
  | { kind: 'thinking'; block: string; text: string }
  | { kind: 'blockDone'; block: string }
  | { kind: 'tool'; id: string; name: string; input: unknown; done: boolean }
  | { kind: 'toolResult'; id: string; output: string; isError: boolean }
  | {
      kind: 'permission';
      requestId: string;
      tool: string;
      input: unknown;
      description: string | null;
      toolUseId: string | null;
      canAlwaysAllow: boolean;
    }
  | { kind: 'permissionClosed'; requestId: string; reason: string }
  | { kind: 'turnEnd'; ok: boolean; durationMs: number | null; costUsd: number | null; error: string | null }
  | { kind: 'notice'; tone: string; text: string }
  | { kind: 'capabilities'; models: unknown; commands: unknown }
  | { kind: 'exited'; code: number | null; stderr: string };

export type Envelope = { session: string; event: SessionEvent };

export type RecentProject = { path: string; name: string };
export type Info = {
  workspace: string | null;
  name: string | null;
  recent: RecentProject[];
  platform: string;
  version: string;
  claude: string | null;
  running: string[];
};
export type Summary = { id: string; title: string; updated: number; branch: string | null };
export type Profile = { id: string; name: string; baseUrl: string; model: string; auth: 'apiKey' | 'bearer' | 'none'; hasKey: boolean };
export type Settings = {
  profiles: Profile[];
  activeProfile: string | null;
  defaultMode: string;
  defaultModel: string | null;
  theme: 'system' | 'light' | 'dark';
  archived: string[];
  sidebarWidth: number | null;
  changesWidth: number | null;
};
export type ChangedFile = {
  path: string;
  originalPath: string | null;
  status: 'added' | 'modified' | 'deleted' | 'renamed' | 'untracked' | 'conflicted';
  additions: number;
  deletions: number;
  binary: boolean;
};
export type GitStatus = { isRepo: boolean; branch: string | null; files: ChangedFile[] };
export type FileDiff = { path: string; status: string; original: string; modified: string; tooLargeOrBinary: boolean };
export type Answer = { kind: 'allow' | 'always' | 'deny' | 'answer'; message?: string; answers?: Record<string, string> };
export type Scope = 'user' | 'project' | 'local';
export type CcSettings = { user: Record<string, any>; project: Record<string, any>; local: Record<string, any>; hasProject: boolean; errors: string[] };
export type Extension = { name: string; description: string; scope: 'user' | 'project'; path: string };
export type McpServer = { name: string; transport: string; target: string; scope: 'user' | 'project' | 'local' };
export type Extensions = { skills: Extension[]; agents: Extension[]; commands: Extension[]; mcpServers: McpServer[] };
export type WindowAction = 'minimize' | 'toggleMaximize' | 'close' | 'isMaximized';

/** Every command, with its arguments and result. */
export interface Commands {
  events_attach: [{ channel: unknown }, void];
  app_info: [{}, Info];
  open_project: [{ path: string | null }, void];
  sessions_list: [{}, Summary[]];
  session_load: [{ id: string }, SessionEvent[]];
  session_new: [{}, string];
  session_send: [{ id: string; text: string; model: string | null; mode: string | null }, void];
  session_respond: [{ id: string; requestId: string; answer: Answer }, void];
  session_interrupt: [{ id: string }, void];
  session_configure: [{ id: string; mode: string | null; model: string | null }, void];
  session_stop: [{ id: string }, void];
  session_rename: [{ id: string; title: string }, void];
  session_archive: [{ id: string; archived: boolean }, void];
  settings_get: [{}, Settings];
  settings_save: [{ settings: Settings }, Settings];
  profile_key: [{ id: string; key: string | null }, void];
  git_status: [{}, GitStatus];
  git_diff: [{ path: string }, FileDiff];
  git_discard: [{ path: string }, void];
  window_action: [{ action: WindowAction }, boolean];
  window_ready: [{}, void];
  notify: [{ title: string; body: string }, void];
  open_url: [{ url: string }, void];
  reveal: [{ path: string }, void];
  cc_settings_get: [{}, CcSettings];
  cc_settings_set: [{ scope: Scope; key: string; value: unknown }, void];
  /** Shows a native confirmation with the command; false when the person cancels. */
  hook_add: [{ scope: Scope; event: string; matcher: string; command: string; timeout: number | null }, boolean];
  hook_remove: [{ scope: Scope; event: string; group: number; index: number }, void];
  memory_read: [{ which: Scope }, string];
  memory_write: [{ which: Scope; text: string }, void];
  extensions_list: [{}, Extensions];
  open_file: [{ path: string }, void];
}

export interface Backend {
  call<K extends keyof Commands>(cmd: K, args: Commands[K][0]): Promise<Commands[K][1]>;
  listen(onBatch: (batch: Envelope[]) => void): Promise<void>;
}

/** The real backend: Tauri's IPC. */
async function tauriBackend(): Promise<Backend> {
  const { invoke, Channel } = await import('@tauri-apps/api/core');
  return {
    call: (cmd, args) => invoke(cmd, args as Record<string, unknown>),
    async listen(onBatch) {
      const channel = new Channel<Envelope[]>();
      channel.onmessage = onBatch;
      await invoke('events_attach', { channel });
    },
  };
}

let backend: Backend | null = null;

export async function connect(): Promise<Backend> {
  if (backend) return backend;
  if ('__TAURI_INTERNALS__' in window) backend = await tauriBackend();
  else if (import.meta.env.DEV) backend = (await import('./mock')).mockBackend();
  else throw new Error('Forge must run inside its window.');
  return backend;
}

export function call<K extends keyof Commands>(cmd: K, args: Commands[K][0] = {} as Commands[K][0]): Promise<Commands[K][1]> {
  if (!backend) throw new Error('not connected');
  return backend.call(cmd, args);
}
