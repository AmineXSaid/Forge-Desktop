// A stand-in backend for `pnpm ui:dev` in a plain browser: demo sessions, a
// streamed reply with a permission prompt, git changes. Only loaded in dev
// builds outside Tauri (see ipc.ts); never part of the app.

import type { Backend, CcSettings, Envelope, Scope, SessionEvent, Settings } from './ipc';

const now = Date.now();
const H = 3600_000;
const DEMO = '9b1c2d3e-0000-4000-8000-00000000000a';

const list = [
  { id: DEMO, title: 'Fix the greeting and add a test', updated: now - 4 * 60_000, branch: 'main' },
  { id: '9b1c2d3e-0000-4000-8000-00000000000b', title: 'Explain the hermes review pipeline', updated: now - 2 * H, branch: 'fix/hermes_upgrade' },
  { id: '9b1c2d3e-0000-4000-8000-00000000000c', title: 'Speed up the session store', updated: now - 5 * H, branch: 'main' },
  { id: '9b1c2d3e-0000-4000-8000-00000000000d', title: 'Write a README for the CLI', updated: now - 26 * H, branch: 'main' },
  { id: '9b1c2d3e-0000-4000-8000-00000000000e', title: 'Why does cargo test hang on Windows?', updated: now - 30 * H, branch: 'main' },
  { id: '9b1c2d3e-0000-4000-8000-00000000000f', title: 'Migrate settings to the new schema', updated: now - 4 * 24 * H, branch: 'main' },
  { id: '9b1c2d3e-0000-4000-8000-000000000010', title: 'Genesis redesign: landing page', updated: now - 12 * 24 * H, branch: 'design' },
];

const saved: SessionEvent[] = [
  { kind: 'user', text: 'Fix the greeting in src/main.rs so it says Hello, Forge, and add a test for it.' },
  { kind: 'thinking', block: 'm1:b1', text: 'The greeting is printed directly in main. To test it I should move the string into a function that main calls, then assert on that function.' },
  { kind: 'text', block: 'm1:b2', text: "I'll move the greeting into a small function so it can be tested, then update `main`." },
  { kind: 'tool', id: 't1', name: 'Read', input: { file_path: 'C:\\dev\\kryptonite\\src\\main.rs' }, done: true },
  { kind: 'toolResult', id: 't1', output: 'fn main() {\n    println!("Hello, world");\n}\n', isError: false },
  { kind: 'tool', id: 't2', name: 'Edit', input: { file_path: 'C:\\dev\\kryptonite\\src\\main.rs', old_string: 'fn main() {\n    println!("Hello, world");\n}', new_string: 'pub fn greeting() -> &\'static str {\n    "Hello, Forge"\n}\n\nfn main() {\n    println!("{}", greeting());\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn greets_by_name() {\n        assert_eq!(super::greeting(), "Hello, Forge");\n    }\n}' }, done: true },
  { kind: 'toolResult', id: 't2', output: 'The file was updated.', isError: false },
  { kind: 'tool', id: 't3', name: 'Bash', input: { command: 'cargo test', description: 'Run the tests' }, done: true },
  { kind: 'toolResult', id: 't3', output: 'running 1 test\ntest tests::greets_by_name ... ok\n\ntest result: ok. 1 passed; 0 failed', isError: false },
  { kind: 'text', block: 'm2:b1', text: 'Done. The greeting now lives in `greeting()` and `main` prints it.\n\n**What changed**\n- `src/main.rs`: new `greeting()` function, used by `main`.\n- A unit test, `greets_by_name`, checks the text.\n\n```sh\ncargo test\n```\n\nAll tests pass.' },
  { kind: 'turnEnd', ok: true, durationMs: 18400, costUsd: 0.0412, error: null },
];

let settings: Settings = {
  profiles: [
    { id: 'ollama', name: 'Ollama on this PC', baseUrl: 'http://127.0.0.1:11434', model: 'qwen3-coder:30b', auth: 'none', hasKey: false },
    { id: 'openrouter', name: 'OpenRouter', baseUrl: 'https://openrouter.ai/api', model: 'anthropic/claude-sonnet-4.5', auth: 'bearer', hasKey: true },
  ],
  activeProfile: null,
  defaultMode: 'default',
  defaultModel: null,
  theme: 'system',
  archived: [],
  sidebarWidth: null,
  changesWidth: null,
};

const files = [
  { path: 'src/main.rs', originalPath: null, status: 'modified' as const, additions: 12, deletions: 3, binary: false },
  { path: 'src/hermes/review.ts', originalPath: null, status: 'modified' as const, additions: 91, deletions: 5, binary: false },
  { path: 'src/hermes/store.ts', originalPath: null, status: 'modified' as const, additions: 28, deletions: 0, binary: false },
  { path: 'docs/genesis-redesign.html', originalPath: null, status: 'untracked' as const, additions: 1156, deletions: 0, binary: false },
];

// Claude Code's settings files, one object per scope.
const cc: CcSettings = {
  user: {
    model: 'opus',
    language: 'english',
    permissions: { allow: ['Read', 'Bash(git status)'], deny: ['Bash(rm -rf:*)'] },
    env: { ANTHROPIC_DEFAULT_HAIKU_MODEL: 'claude-haiku-5-5', HTTPS_PROXY: 'http://proxy.corp:8080', GITHUB_TOKEN: 'ghp_demo' },
    hooks: { Notification: [{ matcher: '', hooks: [{ type: 'command', command: 'powershell -c "[console]::beep()"' }] }] },
  },
  project: {
    model: 'sonnet',
    respectGitignore: true,
    permissions: { defaultMode: 'acceptEdits', allow: ['Bash(cargo test:*)', 'Bash(pnpm test)'], ask: ['Bash(git push:*)'] },
    env: { RUST_LOG: 'info', HTTPS_PROXY: 'http://proxy.team:3128' },
    hooks: { PostToolUse: [{ matcher: 'Edit|Write', hooks: [{ type: 'command', command: 'cargo fmt', timeout: 30 }] }] },
    enabledMcpjsonServers: ['docs'],
  },
  local: { sandbox: { enabled: true, excludedCommands: ['docker'] }, effortLevel: 'high' },
  hasProject: true,
  errors: [],
};
const memory: Record<Scope, string> = {
  user: '# My preferences\n\n- Explain the plan before large changes.\n- British spelling.\n',
  project: '# kryptonite\n\nRust workspace. Run `cargo test --workspace` before saying you are done.\n\n## Style\n- No unwrap() outside tests.\n- Keep functions short.\n',
  local: '',
};
const ext = {
  skills: [
    { name: 'review', description: 'Review a diff for correctness bugs and missing tests', scope: 'project' as const, path: 'C:\\dev\\kryptonite\\.claude\\skills\\review\\SKILL.md' },
    { name: 'release-notes', description: 'Draft release notes from the commits since the last tag', scope: 'user' as const, path: 'C:\\Users\\you\\.claude\\skills\\release-notes\\SKILL.md' },
  ],
  agents: [{ name: 'test-runner', description: 'Runs the tests and reports failures with the smallest repro', scope: 'project' as const, path: 'C:\\dev\\kryptonite\\.claude\\agents\\test-runner.md' }],
  commands: [
    { name: 'ship', description: 'Bump the version, tag and push', scope: 'project' as const, path: 'C:\\dev\\kryptonite\\.claude\\commands\\ship.md' },
    { name: 'standup', description: '', scope: 'user' as const, path: 'C:\\Users\\you\\.claude\\commands\\standup.md' },
  ],
  mcpServers: [
    { name: 'docs', transport: 'http', target: 'https://docs.example.com/mcp', scope: 'project' as const },
    { name: 'postgres', transport: 'stdio', target: 'npx -y @modelcontextprotocol/server-postgres postgres://localhost/dev', scope: 'project' as const },
    { name: 'github', transport: 'http', target: 'https://api.githubcopilot.com/mcp/', scope: 'user' as const },
  ],
};

function setPath(obj: Record<string, any>, parts: string[], value: unknown) {
  const [first, ...rest] = parts;
  if (!rest.length) {
    if (value === null) delete obj[first];
    else obj[first] = value;
    return;
  }
  if (typeof obj[first] !== 'object' || obj[first] === null || Array.isArray(obj[first])) obj[first] = {};
  setPath(obj[first], rest, value);
  if (!Object.keys(obj[first]).length) delete obj[first];
}

export function mockBackend(): Backend {
  let emit: (b: Envelope[]) => void = () => {};
  const send = (session: string, events: SessionEvent[], gap = 40) =>
    events.reduce((p, event) => p.then(() => new Promise<void>((r) => setTimeout(() => { emit([{ session, event }]); r(); }, gap))), Promise.resolve());
  const waiting = new Map<string, string>();

  const handlers: Record<string, (args: any) => unknown> = {
    app_info: () => ({ workspace: 'C:\\dev\\kryptonite', name: 'kryptonite', recent: [{ path: 'C:\\dev\\kryptonite', name: 'kryptonite' }, { path: 'C:\\dev\\forge-desktop', name: 'forge-desktop' }, { path: 'C:\\work\\genesis', name: 'genesis' }], platform: 'windows', version: '0.1.0', claude: 'C:\\Users\\you\\.local\\bin\\claude.exe', running: [] }),
    settings_get: () => settings,
    settings_save: ({ settings: s }) => (settings = s),
    sessions_list: () => list,
    session_load: ({ id }) => (id === DEMO ? saved : [{ kind: 'user', text: list.find((l) => l.id === id)?.title ?? '' }, { kind: 'text', block: 'x:1', text: 'This is an older conversation.' }, { kind: 'turnEnd', ok: true, durationMs: 3200, costUsd: 0.004, error: null }]),
    session_new: () => crypto.randomUUID(),
    session_send: ({ id, text }) => {
      const words = `Let me look at how the project is laid out before changing anything. ${/test/i.test(text) ? "I'll run the test suite first." : "I'll start with the build."}`.split(/(?<= )/);
      send(id, [
        { kind: 'started', model: 'claude-opus-5-5', mode: 'default' },
        ...words.map((w): SessionEvent => ({ kind: 'textDelta', block: 'live:1', text: w })),
        { kind: 'blockDone', block: 'live:1' },
        { kind: 'tool', id: 'lt1', name: 'Bash', input: { command: 'cargo test --workspace', description: 'Run the test suite' }, done: true },
        { kind: 'permission', requestId: 'req-1', tool: 'Bash', input: { command: 'cargo test --workspace', description: 'Run the test suite' }, description: null, toolUseId: 'lt1', canAlwaysAllow: true },
      ], 35).then(() => waiting.set('req-1', id));
    },
    session_respond: ({ id, requestId, answer }) => {
      waiting.delete(requestId);
      const ok = answer.kind !== 'deny';
      send(id, ok ? [
        { kind: 'toolResult', id: 'lt1', output: 'running 21 tests\n...\ntest result: ok. 21 passed; 0 failed', isError: false },
        ...'All 21 tests pass, so the build is healthy and we can start on the change.'.split(/(?<= )/).map((w): SessionEvent => ({ kind: 'textDelta', block: 'live:2', text: w })),
        { kind: 'blockDone', block: 'live:2' },
        { kind: 'turnEnd', ok: true, durationMs: 9100, costUsd: 0.0123, error: null },
      ] : [
        { kind: 'toolResult', id: 'lt1', output: 'The user declined this.', isError: true },
        { kind: 'textDelta', block: 'live:3', text: "Understood, I won't run it. Tell me what you'd like to do instead." },
        { kind: 'blockDone', block: 'live:3' },
        { kind: 'turnEnd', ok: true, durationMs: 2100, costUsd: 0.002, error: null },
      ], 30);
    },
    git_status: () => ({ isRepo: true, branch: 'fix/hermes_upgrade', files }),
    git_diff: ({ path }) => ({
      path, status: 'modified',
      original: 'fn main() {\n    println!("Hello, world");\n}\n',
      modified: 'pub fn greeting() -> &\'static str {\n    "Hello, Forge"\n}\n\nfn main() {\n    println!("{}", greeting());\n}\n',
      tooLargeOrBinary: false,
    }),
    window_action: () => false,
    cc_settings_get: () => structuredClone(cc),
    cc_settings_set: ({ scope, key, value }: { scope: Scope; key: string; value: unknown }) => {
      if (/^env\.(NODE_OPTIONS|PATH|LD_PRELOAD)$/i.test(key)) throw new Error(`${key.slice(4)} can make tools run other code, so Forge does not set it`);
      setPath(cc[scope], key.split('.'), value);
    },
    hook_add: ({ scope, event, matcher, command, timeout }) => {
      if (!confirm(`Claude Code will run this command on ${event}:\n\n${command}`)) return false;
      const hooks = (cc[scope as Scope].hooks ??= {});
      (hooks[event] ??= []).push({ matcher, hooks: [{ type: 'command', command, ...(timeout ? { timeout } : {}) }] });
      return true;
    },
    hook_remove: ({ scope, event, group, index }) => {
      const groups = cc[scope as Scope].hooks?.[event];
      groups?.[group]?.hooks.splice(index, 1);
      if (groups?.[group]?.hooks.length === 0) groups.splice(group, 1);
    },
    mcp_approve: ({ scope, name }) => {
      const s = cc[scope as Scope];
      s.enabledMcpjsonServers = [...(s.enabledMcpjsonServers ?? []), name];
      s.disabledMcpjsonServers = (s.disabledMcpjsonServers ?? []).filter((n: string) => n !== name);
      return true;
    },
    memory_read: ({ which }) => memory[which as Scope],
    memory_write: ({ which, text }) => { memory[which as Scope] = text; },
    extensions_list: () => ext,
    open_file: () => {},
  };

  return {
    async call(cmd, args) {
      const h = handlers[cmd];
      await new Promise((r) => setTimeout(r, 15));
      return (h ? h(args) : undefined) as any;
    },
    async listen(onBatch) {
      emit = onBatch;
    },
  };
}
