// Forge Desktop's window. Plain modules, no build step.
//
// Rust owns everything outside the page (desktop.rs): the project, git, the
// `claude` processes, settings and the keychain. The page calls its commands
// and receives one stream of events: CLI transcript events, permission prompts
// and session exits, each tagged with its session id.

import { markdown, esc } from './markdown.js';
import { diffLines, renderDiff, counts } from './diff.js';

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);

// ---------------------------------------------------------------- icons
const P = {
  sidebar: '<rect x="3" y="4" width="18" height="16" rx="3"/><path d="M9 4v16"/>',
  chevron: '<path d="m7 10 5 5 5-5"/>',
  chevronLeft: '<path d="m14 7-5 5 5 5"/>',
  branch: '<circle cx="6" cy="5.5" r="2"/><circle cx="6" cy="18.5" r="2"/><circle cx="18" cy="8" r="2"/><path d="M6 7.5v9M18 10c0 4-6 3-11.2 7"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-4.2-4.2"/>',
  gear: '<path d="M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
  min: '<path d="M5 12h14"/>',
  max: '<rect x="5.5" y="5.5" width="13" height="13" rx="1.5"/>',
  restore: '<rect x="5.5" y="8.5" width="10" height="10" rx="1.5"/><path d="M8.5 5.5h8.5a1.5 1.5 0 0 1 1.5 1.5v8.5"/>',
  up: '<path d="M12 19V5M6 11l6-6 6 6"/>',
  stop: '<rect x="7" y="7" width="10" height="10" rx="2" fill="currentColor" stroke="none"/>',
  file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
  pencil: '<path d="M4 20h4L19 9l-4-4L4 16z"/><path d="m13.5 6.5 4 4"/>',
  terminal: '<path d="m5 8 4 4-4 4M12 16h7"/>',
  eye: '<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="2.8"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.6 3.8 5.6 3.8 9s-1.3 6.4-3.8 9c-2.5-2.6-3.8-5.6-3.8-9S9.5 5.6 12 3Z"/>',
  check: '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
  alert: '<path d="M12 4 2.8 19.5h18.4z"/><path d="M12 10v4.5M12 17.2v.1"/>',
  dots: '<circle cx="5.5" cy="12" r="1.2" fill="currentColor"/><circle cx="12" cy="12" r="1.2" fill="currentColor"/><circle cx="18.5" cy="12" r="1.2" fill="currentColor"/>',
  refresh: '<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4v7h-7"/>',
  trash: '<path d="M4 7h16M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13"/>',
  archive: '<rect x="3.5" y="4" width="17" height="4.5" rx="1"/><path d="M5 8.5V19a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8.5M10 12.5h4"/>',
  folder: '<path d="M3.5 7a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>',
  diff: '<path d="M8 3v8M4 7h8M4 19h8M16 5l4 4-4 4M20 9H14"/>',
  shield: '<path d="M12 3 5 6v5.5c0 4.5 3 8 7 9.5 4-1.5 7-5 7-9.5V6z"/>',
  sparkle: '<path d="M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5 18 18M6 18l2.5-2.5M15.5 8.5 18 6"/>',
  list: '<path d="M9 6h11M9 12h11M9 18h11"/><circle cx="4.5" cy="6" r="1" fill="currentColor"/><circle cx="4.5" cy="12" r="1" fill="currentColor"/><circle cx="4.5" cy="18" r="1" fill="currentColor"/>',
  plan: '<path d="M8 4h8a2 2 0 0 1 2 2v14l-6-3-6 3V6a2 2 0 0 1 2-2Z"/>',
  bolt: '<path d="M13 3 5 13.5h6L10 21l8-10.5h-6z"/>',
  hand: '<path d="M8 13V6.5a1.5 1.5 0 0 1 3 0V11M11 10.5v-6a1.5 1.5 0 0 1 3 0V11M14 10.5V6a1.5 1.5 0 0 1 3 0v8.5c0 3.5-2.5 6.5-6 6.5-2.5 0-4-1-5.3-3L3.5 14a1.6 1.6 0 0 1 2.6-1.8L8 14"/>',
  cpu: '<rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9.5 9.5h5v5h-5zM9 2.5V6M15 2.5V6M9 18v3.5M15 18v3.5M2.5 9H6M2.5 15H6M18 9h3.5M18 15h3.5"/>',
  copy: '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>',
  key: '<circle cx="8" cy="15" r="4"/><path d="m11 12 9-9M17 6l3 3"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.1"/>',
  brain: '<path d="M9 4.5a3 3 0 0 0-3 3 3 3 0 0 0-2 5.2A3 3 0 0 0 7 18a3 3 0 0 0 5 1.5V6a2.5 2.5 0 0 0-3-1.5ZM15 4.5a3 3 0 0 1 3 3 3 3 0 0 1 2 5.2A3 3 0 0 1 17 18a3 3 0 0 1-5 1.5"/>',
  question: '<circle cx="12" cy="12" r="9"/><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6v.6M12 17.2v.1"/>',
};
const icon = (name, cls = '') => `<svg class="svg ${cls}" viewBox="0 0 24 24" aria-hidden="true">${P[name] ?? ''}</svg>`;

// ---------------------------------------------------------------- state
const S = {
  info: null,
  settings: null,
  list: [],
  sessions: {},       // id -> session state
  current: null,      // selected session id
  view: 'chat',       // 'chat' | 'settings'
  settingsTab: 'general',
  sidebar: true,
  changes: false,
  git: { isRepo: false, branch: null, files: [] },
  changesSel: null,
  search: '',
  models: null,       // from the CLI's initialize answer
  maximized: false,
  editing: null,      // session id being renamed
  profileEdit: null,  // profile form state
};

const MODES = [
  { id: 'default', label: 'Ask before acting', short: 'Ask', icon: 'hand', desc: 'Claude asks before editing files or running commands' },
  { id: 'acceptEdits', label: 'Accept edits', short: 'Accept edits', icon: 'pencil', desc: 'File edits go through; commands still ask' },
  { id: 'plan', label: 'Plan mode', short: 'Plan', icon: 'plan', desc: 'Claude explores and proposes a plan, without changing anything' },
  { id: 'bypassPermissions', label: 'Bypass permissions', short: 'Bypass', icon: 'bolt', desc: 'Everything runs without asking. Use with care.' },
];
const modeOf = (id) => MODES.find((m) => m.id === id) ?? MODES[0];

function session(id) {
  if (!S.sessions[id]) {
    S.sessions[id] = {
      id, items: [], loaded: false, busy: false, running: false, exitedOk: true,
      mode: S.settings?.defaultMode ?? 'default', model: null, streamed: new Set(), curMsg: null,
      blocks: {}, draft: '', resultAt: null, title: null,
    };
  }
  return S.sessions[id];
}

// ---------------------------------------------------------------- helpers
const $ = (sel, root = document) => root.querySelector(sel);
const app = document.getElementById('app');
const workspace = () => S.info?.workspace ?? '';
const rel = (p) => {
  if (!p) return '';
  const ws = workspace().replace(/[\\/]+$/, '');
  if (ws && p.toLowerCase().startsWith(ws.toLowerCase())) return p.slice(ws.length).replace(/^[\\/]+/, '') || p;
  return p;
};
const baseName = (p) => String(p).split(/[\\/]/).pop();
const dirName = (p) => { const parts = String(p).split(/[\\/]/); parts.pop(); return parts.join('/'); };
function when(ms) {
  const d = new Date(ms), now = new Date();
  const mins = Math.round((now - d) / 60000);
  if (mins < 1) return 'now';
  if (mins < 60) return `${mins}m`;
  if (d.toDateString() === now.toDateString()) return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  const days = Math.floor((now.setHours(0, 0, 0, 0) - new Date(ms).setHours(0, 0, 0, 0)) / 86400000);
  if (days < 7) return d.toLocaleDateString([], { weekday: 'short' });
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
}
function bucket(ms) {
  const start = new Date(); start.setHours(0, 0, 0, 0);
  const t = start.getTime();
  if (ms >= t) return 'Today';
  if (ms >= t - 86400000) return 'Yesterday';
  if (ms >= t - 7 * 86400000) return 'Previous 7 days';
  if (ms >= t - 30 * 86400000) return 'Previous 30 days';
  return 'Older';
}
function toast(text, kind = '') {
  let box = $('.toasts');
  if (!box) { box = document.createElement('div'); box.className = 'toasts'; document.body.append(box); }
  const t = document.createElement('div');
  t.className = `toast ${kind}`;
  t.textContent = text;
  box.append(t);
  setTimeout(() => t.remove(), kind === 'error' ? 6000 : 3000);
}
const fail = (e) => toast(String(e?.message ?? e), 'error');
const textOf = (content) => {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content.map((b) => (b.type === 'text' ? b.text : b.type === 'image' ? '[image]' : '')).filter(Boolean).join('\n');
};

// ---------------------------------------------------------------- menus & modals
let openMenu = null;
function closeMenu() { openMenu?.el.remove(); openMenu?.onClose?.(); openMenu = null; }
function menu(anchor, items, { align = 'left', above = false, onClose } = {}) {
  closeMenu();
  const el = document.createElement('div');
  el.className = 'menu';
  el.innerHTML = items.map((it, i) => {
    if (it === '-') return '<div class="ms"></div>';
    if (it.header) return `<div class="mh">${esc(it.header)}</div>`;
    return `<button class="mi ${it.danger ? 'danger' : ''}" data-i="${i}">${it.icon ? icon(it.icon) : ''}<span style="flex:1;min-width:0"><span>${esc(it.label)}</span>${it.desc ? `<span class="md">${esc(it.desc)}</span>` : ''}</span>${it.checked ? icon('check', 'check sm') : ''}</button>`;
  }).join('');
  document.body.append(el);
  const r = anchor.getBoundingClientRect();
  const w = el.offsetWidth, h = el.offsetHeight;
  let left = align === 'right' ? r.right - w : r.left;
  left = Math.max(8, Math.min(left, innerWidth - w - 8));
  let top = above ? r.top - h - 6 : r.bottom + 6;
  if (!above && top + h > innerHeight - 8) top = r.top - h - 6;
  el.style.left = `${left}px`;
  el.style.top = `${Math.max(8, top)}px`;
  el.addEventListener('click', (e) => {
    const b = e.target.closest('.mi');
    if (!b) return;
    const it = items[+b.dataset.i];
    closeMenu();
    it.run?.();
  });
  openMenu = { el, anchor, onClose };
}
document.addEventListener('mousedown', (e) => {
  if (openMenu && !openMenu.el.contains(e.target) && !openMenu.anchor.contains(e.target)) closeMenu();
});

function confirmModal({ title, body, ok = 'OK', danger = false }) {
  return new Promise((resolve) => {
    const bg = document.createElement('div');
    bg.className = 'modal-bg';
    bg.innerHTML = `<div class="modal" role="dialog"><h3>${esc(title)}</h3><p>${esc(body)}</p><div class="row"><button class="btn ghost" data-a="no">Cancel</button><button class="btn ${danger ? 'primary' : 'primary'}" data-a="yes">${esc(ok)}</button></div></div>`;
    const done = (v) => { bg.remove(); resolve(v); };
    bg.addEventListener('click', (e) => {
      if (e.target === bg) done(false);
      const a = e.target.closest('[data-a]')?.dataset.a;
      if (a) done(a === 'yes');
    });
    bg.addEventListener('keydown', (e) => { if (e.key === 'Escape') done(false); });
    document.body.append(bg);
    bg.querySelector('[data-a="yes"]').focus();
  });
}

// ---------------------------------------------------------------- layout
function shell() {
  app.innerHTML = `
    <header class="titlebar" id="titlebar"></header>
    <div class="body" id="body">
      <aside class="sidebar" id="sidebar"></aside>
      <main class="main" id="main"></main>
      <aside class="changes" id="changes" hidden></aside>
    </div>`;
}

function applyLayout() {
  document.body.classList.toggle('sidebar-hidden', !S.sidebar);
  $('#changes').hidden = !S.changes || S.view !== 'chat';
  if (S.settings?.sidebarWidth) document.documentElement.style.setProperty('--sidebar-w', `${S.settings.sidebarWidth}px`);
  if (S.settings?.changesWidth) document.documentElement.style.setProperty('--changes-w', `${S.settings.changesWidth}px`);
  const theme = S.settings?.theme ?? 'system';
  if (theme === 'system') delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
}

// ---------------------------------------------------------------- title bar
function renderTitlebar() {
  const add = S.git.files.reduce((n, f) => n + (f.additions || 0), 0);
  const del = S.git.files.reduce((n, f) => n + (f.deletions || 0), 0);
  $('#titlebar').innerHTML = `
    <button class="tb-btn" data-act="sidebar" title="Toggle sidebar (Ctrl+B)">${icon('sidebar')}</button>
    <button class="tb-btn project-btn" data-act="project" title="Switch project">${esc(S.info?.name ?? 'Open a folder')}${icon('chevron', 'sm')}</button>
    ${S.git.isRepo && S.git.branch ? `<span class="branch" title="Git branch">${icon('branch', 'sm')}${esc(S.git.branch)}</span>` : ''}
    <div class="drag" data-tauri-drag-region></div>
    ${S.git.isRepo ? `<button class="tb-btn ${S.changes && S.view === 'chat' ? 'on' : ''}" data-act="changes" title="Changes (Ctrl+Shift+D)">${icon('diff')}<span>Changes</span>${S.git.files.length ? `<span class="counts"><span class="add">+${add}</span><span class="del">−${del}</span></span>` : ''}</button>` : ''}
    <button class="tb-btn ${S.view === 'settings' ? 'on' : ''}" data-act="settings" title="Settings (Ctrl+,)">${icon('gear')}</button>
    <div class="winctl">
      <button data-act="min" title="Minimize">${icon('min', 'sm')}</button>
      <button data-act="max" title="${S.maximized ? 'Restore' : 'Maximize'}">${icon(S.maximized ? 'restore' : 'max', 'sm')}</button>
      <button data-act="close" class="close" title="Close">${icon('x', 'sm')}</button>
    </div>`;
}

app.addEventListener('dblclick', (e) => {
  if (e.target.closest('.drag')) invoke('window_action', { action: 'toggleMaximize' }).then(syncMax);
});
async function syncMax() {
  S.maximized = await invoke('window_action', { action: 'isMaximized' }).catch(() => false);
  renderTitlebar();
}

function projectMenu(anchor) {
  const recent = (S.info?.recent ?? []).filter((r) => r.path !== workspace());
  menu(anchor, [
    { header: S.info?.workspace ?? '' },
    ...recent.slice(0, 8).map((r) => ({ label: r.name, desc: r.path, icon: 'folder', run: () => invoke('open_project', { path: r.path }).catch(fail) })),
    ...(recent.length ? ['-'] : []),
    { label: 'Open folder…', icon: 'plus', run: () => invoke('open_project', { path: null }).catch(fail) },
  ]);
}

// ---------------------------------------------------------------- sidebar
function sessionRows() {
  const rows = [...S.list];
  // Sessions started here that have no saved file yet.
  for (const s of Object.values(S.sessions)) {
    if (s.items.length && !rows.some((r) => r.id === s.id)) rows.unshift({ id: s.id, title: s.title ?? firstUserText(s) ?? 'New session', updated: Date.now() });
  }
  const q = S.search.trim().toLowerCase();
  return q ? rows.filter((r) => r.title.toLowerCase().includes(q)) : rows;
}
const firstUserText = (s) => s.items.find((i) => i.kind === 'user')?.text?.split('\n')[0]?.slice(0, 80);

function renderSidebar() {
  const rows = sessionRows();
  let html = '', last = null;
  for (const r of rows) {
    const g = bucket(r.updated);
    if (g !== last) { html += `<div class="group-label">${g}</div>`; last = g; }
    const s = S.sessions[r.id];
    const dot = s?.items.some((i) => i.kind === 'permission' && !i.done) ? '<span class="dot wait" title="Waiting for you"></span>' : s?.busy ? '<span class="dot run" title="Working"></span>' : '';
    if (S.editing === r.id) {
      html += `<div class="srow sel"><input id="rename-input" value="${esc(r.title)}" maxlength="120" /></div>`;
      continue;
    }
    html += `<div class="srow ${S.current === r.id && S.view === 'chat' ? 'sel' : ''}" data-id="${r.id}" title="${esc(r.title)}">${dot}<span class="t">${esc(r.title)}</span><span class="when">${when(r.updated)}</span><button class="more" data-more="${r.id}" title="More">${icon('dots', 'sm')}</button></div>`;
  }
  if (!rows.length) html = `<div class="side-empty">${S.search ? 'No matching sessions.' : 'No sessions yet. Start one and it shows up here.'}</div>`;
  const prof = S.settings?.profiles?.find((p) => p.id === S.settings.activeProfile);
  $('#sidebar').innerHTML = `
    <div class="side-top">
      <button class="new-btn" data-act="new">${icon('plus')}New session<kbd>Ctrl N</kbd></button>
      <label class="search">${icon('search', 'sm')}<input id="search" placeholder="Search sessions" value="${esc(S.search)}" /></label>
    </div>
    <div class="sessions" id="session-list">${html}</div>
    <div class="side-foot">
      <button class="who" data-act="endpoints">
        <span class="avatar">${prof ? icon('cpu', 'sm') : icon('sparkle', 'sm')}</span>
        <span style="min-width:0"><div class="l1">${esc(prof ? prof.name : 'Claude account')}</div><div class="l2">${esc(prof ? (prof.model || prof.baseUrl) : 'Signed in through Claude Code')}</div></span>
      </button>
    </div>
    <div class="resizer" data-resize="sidebar"></div>`;
  const input = $('#rename-input');
  if (input) { input.focus(); input.select(); }
}

function sessionMenu(anchor, id) {
  anchor.classList.add('open');
  menu(anchor, [
    { label: 'Rename', icon: 'pencil', run: () => { S.editing = id; renderSidebar(); } },
    { label: 'Archive', icon: 'archive', run: () => archive(id) },
  ], { align: 'right', onClose: () => anchor.classList.remove('open') });
}

async function archive(id) {
  await invoke('session_stop', { id }).catch(() => {});
  await invoke('session_archive', { id, archived: true }).catch(fail);
  S.list = S.list.filter((r) => r.id !== id);
  delete S.sessions[id];
  if (S.current === id) S.current = null;
  S.settings = await invoke('settings_get');
  renderAll();
}

async function commitRename(id, value) {
  S.editing = null;
  const title = value.trim();
  const row = S.list.find((r) => r.id === id);
  if (title && row && title !== row.title) {
    try {
      await invoke('session_rename', { id, title });
      row.title = title;
    } catch (e) { fail(e); }
  } else if (title && !row) {
    session(id).title = title;
  }
  renderSidebar();
  renderMain();
}

async function refreshList() {
  try { S.list = await invoke('sessions_list'); } catch (e) { console.error(e); }
  renderSidebar();
}

// ---------------------------------------------------------------- transcript model
function pushItem(s, item) { s.items.push(item); return item; }
function findTool(s, id) { return s.items.find((i) => i.kind === 'tool' && i.id === id); }

function applyAssistantBlocks(s, msgId, blocks, live) {
  for (const b of blocks ?? []) {
    if (b.type === 'tool_use') {
      const t = findTool(s, b.id);
      if (t) { t.input = b.input; t.name = b.name; }
      else pushItem(s, { kind: 'tool', id: b.id, name: b.name, input: b.input, result: null, pending: live });
    } else if (live && msgId && s.streamed.has(msgId)) {
      continue; // already shown from the stream
    } else if (b.type === 'text' && b.text?.trim()) {
      pushItem(s, { kind: 'text', text: b.text });
    } else if (b.type === 'thinking' && b.thinking?.trim()) {
      pushItem(s, { kind: 'thinking', text: b.thinking });
    }
  }
}

function applyToolResults(s, blocks) {
  for (const b of blocks) {
    if (b.type !== 'tool_result') continue;
    const t = findTool(s, b.tool_use_id);
    if (t) { t.result = typeof b.content === 'string' ? b.content : textOf(b.content); t.error = !!b.is_error; t.pending = false; }
  }
}

function fromSaved(s, messages) {
  s.items = [];
  for (const m of messages) {
    if (m.role === 'user') {
      if (typeof m.content === 'string') pushItem(s, { kind: 'user', text: m.content });
      else if (Array.isArray(m.content)) {
        if (m.content.some((b) => b.type === 'tool_result')) applyToolResults(s, m.content);
        else { const t = textOf(m.content); if (t.trim() && !t.trim().startsWith('<')) pushItem(s, { kind: 'user', text: t }); }
      }
    } else {
      applyAssistantBlocks(s, null, m.content, false);
    }
  }
  for (const i of s.items) if (i.kind === 'tool') i.pending = false;
  s.loaded = true;
}

function onEvent(ev) {
  const s = session(ev.session);
  switch (ev.type) {
    case 'cli': onCli(s, ev.event); break;
    case 'permission':
      s.busy = true;
      pushItem(s, { kind: 'permission', requestId: ev.requestId, tool: ev.tool, input: ev.input, description: ev.description, canAlways: ev.canAlwaysAllow, done: false, answers: {} });
      notify(s, ev.tool === 'AskUserQuestion' ? 'Claude has a question' : 'Claude needs your permission', ev.tool === 'AskUserQuestion' ? (ev.input?.questions?.[0]?.question ?? '') : permTitle(ev.tool, ev.input));
      break;
    case 'permission_cancelled': {
      const p = s.items.find((i) => i.kind === 'permission' && i.requestId === ev.requestId);
      if (p) { p.done = true; p.verdict = 'Cancelled'; }
      break;
    }
    case 'control':
      if (ev.response?.subtype === 'success' && ev.response.response?.models) S.models = ev.response.response.models;
      if (ev.response?.subtype === 'error') console.warn('control error', ev.response.error);
      break;
    case 'exit':
      s.running = false;
      s.busy = false;
      for (const i of s.items) { if (i.kind === 'tool' && i.pending) i.pending = false; if (i.kind === 'permission' && !i.done) { i.done = true; i.verdict = 'Session ended'; } }
      if (ev.code && !s.stopping) pushItem(s, { kind: 'notice', tone: 'error', text: `Claude Code stopped (exit code ${ev.code}).`, detail: ev.stderr });
      s.stopping = false;
      break;
  }
  scheduleRender(s.id);
}

function onCli(s, e) {
  switch (e.type) {
    case 'system':
      if (e.subtype === 'init') { s.running = true; s.model = e.model; s.mode = e.permissionMode ?? s.mode; }
      else if (e.subtype === 'compact_boundary') pushItem(s, { kind: 'notice', text: 'Earlier messages were summarized to free up context.' });
      else if (e.subtype === 'api_retry' || e.subtype === 'api_error') pushItem(s, { kind: 'notice', tone: 'warn', text: e.message ?? 'Retrying the request…' });
      break;
    case 'stream_event': {
      const se = e.event;
      if (se.type === 'message_start') { s.curMsg = se.message?.id; s.streamed.add(s.curMsg); s.blocks = {}; }
      else if (se.type === 'content_block_start') {
        const b = se.content_block;
        if (b.type === 'text') s.blocks[se.index] = pushItem(s, { kind: 'text', text: b.text ?? '', live: true });
        else if (b.type === 'thinking') s.blocks[se.index] = pushItem(s, { kind: 'thinking', text: '', live: true });
        else if (b.type === 'tool_use' && !findTool(s, b.id)) s.blocks[se.index] = pushItem(s, { kind: 'tool', id: b.id, name: b.name, input: {}, result: null, pending: true, partial: '' });
      } else if (se.type === 'content_block_delta') {
        const it = s.blocks[se.index];
        if (!it) break;
        if (se.delta.type === 'text_delta') it.text += se.delta.text;
        else if (se.delta.type === 'thinking_delta') it.text += se.delta.thinking;
        else if (se.delta.type === 'input_json_delta') it.partial = (it.partial ?? '') + se.delta.partial_json;
      } else if (se.type === 'content_block_stop') {
        const it = s.blocks[se.index];
        if (it) { it.live = false; if (it.kind === 'text' && !it.text.trim()) s.items.splice(s.items.indexOf(it), 1); }
      }
      break;
    }
    case 'assistant': applyAssistantBlocks(s, e.message?.id, e.message?.content, true); break;
    case 'user': if (Array.isArray(e.message?.content)) applyToolResults(s, e.message.content); break;
    case 'result': {
      s.busy = false;
      for (const i of s.items) if (i.kind === 'text' || i.kind === 'thinking') i.live = false;
      if (e.is_error || (e.subtype && e.subtype !== 'success')) {
        const msg = e.result || (e.subtype === 'error_max_turns' ? 'Stopped: the turn limit was reached.' : e.errors?.join('\n') || 'The turn ended with an error.');
        pushItem(s, { kind: 'notice', tone: 'error', text: msg });
      } else if (e.duration_ms) {
        pushItem(s, { kind: 'meta', duration: e.duration_ms, cost: e.total_cost_usd });
      }
      notify(s, 'Claude finished', s.title ?? firstUserText(s) ?? '');
      refreshList();
      refreshGit();
      break;
    }
  }
}

function notify(s, title, body) {
  if (document.hasFocus()) return;
  invoke('notify', { title, body: String(body ?? '') }).catch(() => {});
}

// ---------------------------------------------------------------- rendering a thread
const TOOL_ICONS = { Bash: 'terminal', Read: 'eye', Write: 'file', Edit: 'pencil', MultiEdit: 'pencil', NotebookEdit: 'pencil', Grep: 'search', Glob: 'search', WebFetch: 'globe', WebSearch: 'globe', TodoWrite: 'list', Task: 'sparkle', Agent: 'sparkle', ExitPlanMode: 'plan', AskUserQuestion: 'question' };
const TOOL_NAMES = { Bash: 'Run', Read: 'Read', Write: 'Write', Edit: 'Edit', MultiEdit: 'Edit', Grep: 'Search', Glob: 'Find files', WebFetch: 'Fetch', WebSearch: 'Web search', TodoWrite: 'Update todos', Task: 'Agent', Agent: 'Agent', LS: 'List' };

function toolArg(name, input = {}) {
  switch (name) {
    case 'Bash': return input.description || input.command || '';
    case 'Read': case 'Write': case 'Edit': case 'MultiEdit': case 'NotebookEdit': return rel(input.file_path ?? input.notebook_path);
    case 'Grep': return `${input.pattern ?? ''}${input.path ? `  in ${rel(input.path)}` : ''}`;
    case 'Glob': return input.pattern ?? '';
    case 'WebFetch': return input.url ?? '';
    case 'WebSearch': return input.query ?? '';
    case 'Task': case 'Agent': return input.description ?? '';
    case 'TodoWrite': return `${(input.todos ?? []).length} items`;
    default: {
      const v = Object.values(input).find((x) => typeof x === 'string');
      return v ?? '';
    }
  }
}

function toolDetail(t) {
  const input = t.input ?? {};
  const out = [];
  if (t.name === 'Edit' && typeof input.old_string === 'string') out.push(renderDiff(diffLines(input.old_string, input.new_string ?? ''), 2));
  else if (t.name === 'MultiEdit' && Array.isArray(input.edits)) out.push(input.edits.map((e) => renderDiff(diffLines(e.old_string ?? '', e.new_string ?? ''), 2)).join('<div class="hunk"></div>'));
  else if (t.name === 'Write' && typeof input.content === 'string') out.push(renderDiff(diffLines('', input.content.split('\n').slice(0, 400).join('\n')), 0));
  else if (t.name === 'Bash') out.push(`<div class="label">Command</div><pre>${esc(input.command ?? '')}</pre>`);
  else if (t.name === 'TodoWrite') out.push(`<pre>${esc((input.todos ?? []).map((x) => `${x.status === 'completed' ? '☑' : x.status === 'in_progress' ? '◐' : '☐'} ${x.content}`).join('\n'))}</pre>`);
  else if (Object.keys(input).length) out.push(`<div class="label">Input</div><pre>${esc(JSON.stringify(input, null, 2))}</pre>`);
  if (t.result != null && !(['Edit', 'MultiEdit', 'Write'].includes(t.name) && !t.error)) {
    const r = String(t.result);
    out.push(`<div class="label">${t.error ? 'Error' : 'Output'}</div><pre>${esc(r.length > 20000 ? `${r.slice(0, 20000)}\n… (${r.length - 20000} more characters)` : r)}</pre>`);
  }
  return out.join('');
}

function permTitle(tool, input = {}) {
  switch (tool) {
    case 'Bash': return 'Run this command?';
    case 'Write': return `Write ${rel(input.file_path)}?`;
    case 'Edit': case 'MultiEdit': return `Edit ${rel(input.file_path)}?`;
    case 'NotebookEdit': return `Edit ${rel(input.notebook_path)}?`;
    case 'WebFetch': return `Fetch ${input.url ?? 'a web page'}?`;
    case 'WebSearch': return 'Search the web?';
    case 'ExitPlanMode': return 'Ready to start?';
    default: return `Use ${tool}?`;
  }
}

function renderPermission(it) {
  const input = it.input ?? {};
  if (it.tool === 'AskUserQuestion') {
    const qs = input.questions ?? [];
    return `<div class="ask ${it.done ? 'done' : ''}" data-req="${esc(it.requestId)}">
      <div class="ask-head"><span class="badge">${icon('question', 'sm')}</span>Claude has ${qs.length > 1 ? 'some questions' : 'a question'}</div>
      ${qs.map((q, qi) => `<div class="q"><div class="qh">${esc(q.header ?? '')}</div><div class="qt">${esc(q.question)}</div><div class="opts">${(q.options ?? []).map((o, oi) => {
        const on = (it.answers[q.question] ?? []).includes(o.label);
        return `<button class="opt ${on ? 'on' : ''}" data-q="${qi}" data-o="${oi}" ${it.done ? 'disabled' : ''}><div class="ol">${esc(o.label)}</div>${o.description ? `<div class="od">${esc(o.description)}</div>` : ''}</button>`;
      }).join('')}</div></div>`).join('')}
      ${it.done ? `<div class="verdict">${esc(it.verdict ?? '')}</div>` : ''}
      <div class="ask-actions"><button class="btn ghost" data-perm="deny">Skip</button><span class="grow"></span><button class="btn primary" data-perm="answer" ${qs.every((q) => (it.answers[q.question] ?? []).length) ? '' : 'disabled'}>Submit</button></div>
    </div>`;
  }
  let detail = '';
  if (it.tool === 'ExitPlanMode') detail = `<div class="ask-detail"><div class="prose" style="padding:12px 14px">${markdown(input.plan ?? '')}</div></div>`;
  else if (it.tool === 'Bash') detail = `<div class="ask-detail"><pre>${esc(input.command ?? '')}</pre></div>`;
  else if (['Edit', 'MultiEdit', 'Write'].includes(it.tool)) detail = `<div class="ask-detail">${toolDetail({ name: it.tool, input })}</div>`;
  else if (Object.keys(input).length) detail = `<div class="ask-detail"><pre>${esc(JSON.stringify(input, null, 2))}</pre></div>`;
  const plan = it.tool === 'ExitPlanMode';
  const desc = it.tool === 'Bash' ? input.description : it.description;
  return `<div class="ask ${it.done ? 'done' : ''}" data-req="${esc(it.requestId)}">
    <div class="ask-head"><span class="badge">${icon(plan ? 'plan' : 'shield', 'sm')}</span>${esc(permTitle(it.tool, input))}</div>
    ${desc && desc !== rel(input.file_path) ? `<div class="ask-desc">${esc(desc)}</div>` : ''}
    ${detail}
    ${it.done ? `<div class="verdict">${esc(it.verdict ?? '')}</div>` : ''}
    <div class="ask-actions">
      <button class="btn ghost" data-perm="deny">${plan ? 'Keep planning' : 'Deny'}</button>
      <span class="grow"></span>
      ${it.canAlways ? `<button class="btn" data-perm="always">${plan ? 'Start, accepting edits' : 'Always allow'}</button>` : ''}
      <button class="btn primary" data-perm="allow">${plan ? 'Start' : 'Allow'}</button>
    </div>
  </div>`;
}

function renderItem(it, s, idx) {
  switch (it.kind) {
    case 'user': return `<div class="msg-user">${esc(it.text)}</div>`;
    case 'text': return `<div class="prose ${it.live && s.busy ? 'cursor' : ''}">${markdown(it.text)}</div>`;
    case 'thinking': return `<details class="thinking" ${it.open ? 'open' : ''} data-idx="${idx}"><summary>${icon('brain', 'sm')}${it.live && s.busy ? 'Thinking…' : 'Thought process'}</summary><div class="think-body">${esc(it.text)}</div></details>`;
    case 'tool': {
      const name = TOOL_NAMES[it.name] ?? it.name.replace(/^mcp__([^_]+)__/, '$1 · ');
      const state = it.pending && s.running ? '<span class="state"><span class="spinner"></span></span>' : it.error ? `<span class="state err">${icon('alert', 'sm')}Failed</span>` : '';
      let extra = '';
      if (!it.pending && ['Edit', 'MultiEdit', 'Write'].includes(it.name)) {
        const ops = it.name === 'Write' ? diffLines('', it.input?.content ?? '') : diffLines(it.input?.old_string ?? '', it.input?.new_string ?? '');
        const c = counts(ops);
        if (it.name !== 'MultiEdit') extra = `<span class="counts"><span class="add">+${c.add}</span>${c.del ? `<span class="del">−${c.del}</span>` : ''}</span>`;
      }
      return `<div class="tool" data-idx="${idx}"><div class="tool-head" data-toggle="${idx}"><span class="ic">${icon(TOOL_ICONS[it.name] ?? 'cpu', 'sm')}</span><span class="name">${esc(name)}</span><span class="arg">${esc(toolArg(it.name, it.input))}</span>${extra}${state}</div>${it.open ? `<div class="tool-body">${toolDetail(it)}</div>` : ''}</div>`;
    }
    case 'permission': return renderPermission(it);
    case 'notice': return `<div class="notice ${it.tone ?? ''}">${icon(it.tone ? 'alert' : 'info', 'sm')}<div>${esc(it.text)}${it.detail ? `<pre>${esc(it.detail)}</pre>` : ''}</div></div>`;
    case 'meta': return `<div class="turn-meta"><span>${(it.duration / 1000).toFixed(1)}s</span>${it.cost ? `<span>$${it.cost.toFixed(4)}</span>` : ''}</div>`;
    default: return '';
  }
}

const threadCache = new Map(); // element key -> html
let renderQueued = new Set();
function scheduleRender(id) {
  renderQueued.add(id);
  if (renderQueued.size === 1) requestAnimationFrame(() => {
    const ids = renderQueued; renderQueued = new Set();
    if (ids.has(S.current) && S.view === 'chat') { renderThread(); renderComposerState(); renderHead(); }
    renderSidebar();
  });
}

function renderThread() {
  const s = S.current && S.sessions[S.current];
  const box = $('#thread');
  if (!s || !box) return;
  const scroller = $('#scroll');
  const atBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;
  const visible = s.items.filter((i) => i.kind !== 'text' || i.text.trim() || i.live);
  const children = box.children;
  visible.forEach((it, n) => {
    const idx = s.items.indexOf(it);
    const html = renderItem(it, s, idx);
    const el = children[n];
    if (el && el.dataset.html === html) return;
    const tmp = document.createElement('div');
    tmp.innerHTML = html;
    const fresh = tmp.firstElementChild ?? document.createElement('div');
    fresh.dataset.html = html;
    if (el) el.replaceWith(fresh); else box.append(fresh);
  });
  while (children.length > visible.length) children[children.length - 1].remove();
  if (s.busy && !s.items.some((i) => (i.live && i.kind === 'text') || (i.kind === 'permission' && !i.done))) {
    box.insertAdjacentHTML('beforeend', '<div class="thinking working" data-html="w"><span style="display:inline-flex;gap:8px;align-items:center"><span class="spinner"></span>Working…</span></div>');
  }
  if (atBottom) scroller.scrollTop = scroller.scrollHeight;
}

// ---------------------------------------------------------------- main view
function renderHead() {
  const head = $('#chat-head');
  if (!head) return;
  const s = S.current && S.sessions[S.current];
  const row = S.list.find((r) => r.id === S.current);
  const title = row?.title ?? s?.title ?? (s && firstUserText(s)) ?? '';
  head.innerHTML = s && s.items.length ? `<span class="title">${esc(title)}</span>${row?.branch ? `<span class="sub">${icon('branch', 'sm')}</span>` : ''}<span class="grow" style="flex:1"></span>${s.running ? `<span class="sub">${esc(s.model ?? '')}</span>` : ''}` : '';
}

function composerHtml(s) {
  const mode = modeOf(s?.mode);
  const prof = S.settings?.profiles?.find((p) => p.id === S.settings.activeProfile);
  const modelLabel = s?.modelChoice ? labelForModel(s.modelChoice) : s?.model ? labelForModel(s.model) : prof?.model || (S.settings?.defaultModel ? labelForModel(S.settings.defaultModel) : 'Default model');
  return `<div class="composer">
      <textarea id="input" rows="1" placeholder="${s?.items.length ? 'Reply to Claude…' : 'Describe a task, ask a question, or paste an error…'}" spellcheck="true">${esc(s?.draft ?? '')}</textarea>
      <div class="comp-row">
        <button class="pill mode-${mode.id}" data-act="mode" title="Permission mode (Shift+Tab)">${icon(mode.icon, 'sm')}${esc(mode.short)}${icon('chevron', 'sm')}</button>
        <button class="pill" data-act="model" title="Model">${icon('cpu', 'sm')}${esc(modelLabel)}${icon('chevron', 'sm')}</button>
        <span class="grow"></span>
        <button class="send" id="send" title="Send (Enter)">${icon('up', 'lg')}</button>
      </div>
    </div>`;
}

function labelForModel(m) {
  const known = S.models?.find((x) => x.value === m || x.resolvedModel === m);
  if (known) return known.displayName.replace(/\s*\(.*\)$/, '');
  return m;
}

function renderComposerState() {
  const s = S.current && S.sessions[S.current];
  const btn = $('#send');
  const input = $('#input');
  if (!btn || !input) return;
  const hasText = input.value.trim().length > 0;
  const stop = s?.busy && !hasText;
  btn.classList.toggle('stop', !!stop);
  btn.innerHTML = stop ? icon('stop', 'lg') : icon('up', 'lg');
  btn.title = stop ? 'Stop (Esc)' : 'Send (Enter)';
  btn.disabled = !stop && !hasText;
}

function autosize(t) { t.style.height = 'auto'; t.style.height = `${Math.min(t.scrollHeight, innerHeight * 0.4)}px`; }

function renderMain() {
  const main = $('#main');
  if (S.view === 'settings') { renderSettings(main); return; }
  if (!S.info?.claude) { main.innerHTML = setupHtml(); return; }
  const s = S.current ? session(S.current) : null;
  if (!s || (!s.items.length && s.loaded !== 'loading')) {
    const suggestions = ['Explain how this project is structured', 'Find and fix a bug in the tests', 'Review my uncommitted changes', 'Write a README for this project'];
    main.innerHTML = `<div class="empty">
        <h1>What should we work on in <span class="proj">${esc(S.info?.name ?? 'this project')}</span>?</h1>
        <p class="lead">Claude works in ${esc(S.info?.workspace ?? 'your folder')}. It asks before it changes anything.</p>
        <div class="composer-wrap">${composerHtml(s)}</div>
        <div class="chips">${suggestions.map((t) => `<button class="chip" data-suggest="${esc(t)}">${esc(t)}</button>`).join('')}</div>
      </div>`;
  } else {
    main.innerHTML = `<div class="chat-head" id="chat-head"></div>
      <div class="scroll" id="scroll"><div class="thread" id="thread"></div></div>
      <div class="composer-wrap">${composerHtml(s)}<div class="hint">Enter to send · Shift+Enter for a new line · Shift+Tab to change mode</div></div>`;
    renderHead();
    renderThread();
    const sc = $('#scroll');
    sc.scrollTop = sc.scrollHeight;
    sc.addEventListener('scroll', () => $('#chat-head')?.classList.toggle('scrolled', sc.scrollTop > 4));
  }
  const input = $('#input');
  if (input) { autosize(input); renderComposerState(); }
}

function setupHtml() {
  const win = S.info?.platform === 'windows';
  const cmd = win ? 'irm https://claude.ai/install.ps1 | iex' : 'curl -fsSL https://claude.ai/install.sh | bash';
  return `<div class="empty"><div class="setup">
    <h2>Install Claude Code to start</h2>
    <p>Forge runs the Claude Code CLI on your computer. It isn't installed yet, or it isn't on your PATH. Run this in ${win ? 'PowerShell' : 'a terminal'}:</p>
    <div class="cmd"><span>${esc(cmd)}</span><button class="icon-btn" data-copy="${esc(cmd)}" title="Copy">${icon('copy', 'sm')}</button></div>
    <p>Then sign in with <code>claude</code> once, or set up an endpoint in Settings.</p>
    <div style="display:flex;gap:8px"><button class="btn primary" data-act="recheck">Check again</button><button class="btn" data-act="endpoints">Endpoints</button></div>
  </div></div>`;
}

// ---------------------------------------------------------------- sending
async function send(text) {
  const t = text.trim();
  if (!t) return;
  if (!S.current) S.current = await invoke('session_new');
  const s = session(S.current);
  const first = !s.items.length;
  pushItem(s, { kind: 'user', text: t });
  s.busy = true;
  s.draft = '';
  if (first) renderMain(); else { const i = $('#input'); if (i) { i.value = ''; autosize(i); } scheduleRender(s.id); }
  try {
    await invoke('session_send', { id: s.id, text: t, model: s.modelChoice ?? null, mode: s.mode ?? null });
    s.running = true;
  } catch (e) {
    s.busy = false;
    pushItem(s, { kind: 'notice', tone: 'error', text: String(e) });
    scheduleRender(s.id);
  }
}

async function stop() {
  const s = S.current && S.sessions[S.current];
  if (!s?.busy) return;
  await invoke('session_interrupt', { id: s.id }).catch(fail);
}

async function respond(reqId, kind) {
  const s = session(S.current);
  const it = s.items.find((i) => i.kind === 'permission' && i.requestId === reqId);
  if (!it || it.done) return;
  const answer = { kind };
  if (kind === 'answer') {
    answer.answers = Object.fromEntries(Object.entries(it.answers).map(([q, v]) => [q, v.join(', ')]));
  }
  if (kind === 'deny' && it.tool === 'ExitPlanMode') answer.message = 'The user wants to keep planning. Ask what to change.';
  try {
    await invoke('session_respond', { id: s.id, requestId: reqId, answer });
    it.done = true;
    it.verdict = { allow: 'Allowed', always: 'Always allowed', deny: it.tool === 'ExitPlanMode' ? 'Kept planning' : it.tool === 'AskUserQuestion' ? 'Skipped' : 'Denied', answer: 'Answered' }[kind];
    if (kind === 'always' && it.tool === 'ExitPlanMode') s.mode = 'acceptEdits';
    else if (it.tool === 'ExitPlanMode' && kind === 'allow') s.mode = 'default';
  } catch (e) { fail(e); }
  scheduleRender(s.id);
}

async function setMode(id) {
  const s = session(S.current ?? (S.current = await invoke('session_new')));
  s.mode = id;
  if (s.running) await invoke('session_configure', { id: s.id, mode: id, model: null }).catch(fail);
  renderMain();
}

async function setModel(value) {
  const s = session(S.current ?? (S.current = await invoke('session_new')));
  s.modelChoice = value;
  if (s.running && value) await invoke('session_configure', { id: s.id, mode: null, model: value }).catch(fail);
  renderMain();
}

function modelMenu(anchor) {
  const s = S.current && S.sessions[S.current];
  const prof = S.settings?.profiles?.find((p) => p.id === S.settings.activeProfile);
  const current = s?.modelChoice ?? s?.model;
  const items = [];
  if (prof) {
    items.push({ header: `${prof.name}` });
    items.push({ label: prof.model || 'Server default', icon: 'cpu', checked: true, run: () => {} });
  } else {
    const models = S.models ?? [
      { value: 'default', displayName: 'Default', description: 'The recommended model' },
      { value: 'opus', displayName: 'Opus', description: 'Most capable' },
      { value: 'sonnet', displayName: 'Sonnet', description: 'Fast and capable' },
      { value: 'haiku', displayName: 'Haiku', description: 'Fastest' },
    ];
    items.push({ header: 'Model' });
    for (const m of models) items.push({ label: m.displayName.replace(/\s*\(.*\)$/, ''), desc: m.description, checked: current === m.value || (!current && m.value === 'default'), run: () => setModel(m.value) });
  }
  menu(anchor, items, { above: true });
}

function modeMenu(anchor) {
  const s = S.current && S.sessions[S.current];
  menu(anchor, [{ header: 'Permission mode' }, ...MODES.map((m) => ({ label: m.label, desc: m.desc, icon: m.icon, checked: (s?.mode ?? S.settings?.defaultMode) === m.id, run: () => setMode(m.id) }))], { above: true });
}

async function openSession(id) {
  S.view = 'chat';
  S.current = id;
  const s = session(id);
  if (!s.loaded && !s.items.length) {
    s.loaded = 'loading';
    renderAll();
    try { fromSaved(s, await invoke('session_load', { id })); } catch (e) { fail(e); s.loaded = true; }
  }
  renderAll();
  $('#input')?.focus();
}

function newSession() {
  S.view = 'chat';
  S.current = null;
  renderAll();
  $('#input')?.focus();
}

// ---------------------------------------------------------------- changes pane
async function refreshGit() {
  try {
    const st = await invoke('git_status');
    const changed = JSON.stringify(st) !== JSON.stringify(S.git);
    S.git = st;
    if (changed) { renderTitlebar(); if (S.changes) renderChanges(); }
  } catch (e) { console.error(e); }
}

async function renderChanges() {
  const pane = $('#changes');
  if (!S.changes || S.view !== 'chat') { pane.hidden = true; return; }
  pane.hidden = false;
  const files = S.git.files;
  if (S.changesSel && !files.some((f) => f.path === S.changesSel)) S.changesSel = null;
  if (!S.changesSel && files.length) S.changesSel = files[0].path;
  const add = files.reduce((n, f) => n + f.additions, 0), del = files.reduce((n, f) => n + f.deletions, 0);
  const letter = { modified: 'M', added: 'A', untracked: 'U', deleted: 'D', renamed: 'R', conflicted: '!' };
  pane.innerHTML = `
    <div class="resizer" data-resize="changes"></div>
    <div class="ch-head"><h2>Changes</h2><span class="n">${files.length} file${files.length === 1 ? '' : 's'}</span><span class="counts"><span class="add">+${add}</span><span class="del">−${del}</span></span><span style="flex:1"></span>
      <button class="icon-btn" data-act="git-refresh" title="Refresh">${icon('refresh', 'sm')}</button>
      <button class="icon-btn" data-act="changes" title="Close">${icon('x', 'sm')}</button></div>
    ${files.length ? `<div class="ch-files">${files.map((f) => `<div class="frow ${f.path === S.changesSel ? 'sel' : ''}" data-file="${esc(f.path)}" title="${esc(f.path)}"><span class="st ${f.status}">${letter[f.status] ?? '?'}</span><span class="fn">${esc(baseName(f.path))}</span><span class="dir">${esc(dirName(f.path))}</span><span class="counts">${f.binary ? '<span>bin</span>' : `<span class="add">+${f.additions}</span>${f.deletions ? `<span class="del">−${f.deletions}</span>` : ''}`}</span></div>`).join('')}</div>
    <div class="ch-view" id="ch-view"></div>` : `<div class="ch-empty">No changes against HEAD.<br>Edits Claude makes show up here.</div>`}`;
  if (S.changesSel) {
    const view = $('#ch-view');
    try {
      const d = await invoke('git_diff', { path: S.changesSel });
      const bar = `<div class="ch-bar"><span class="path">${esc(d.path)}</span><button class="btn sm" data-act="reveal" data-path="${esc(d.path)}">Show in folder</button><button class="btn sm danger" data-act="discard" data-path="${esc(d.path)}">Discard</button></div>`;
      view.innerHTML = bar + (d.tooLargeOrBinary ? '<div class="ch-empty">This file is binary or too large to show.</div>' : renderDiff(diffLines(d.original, d.modified), 3));
    } catch (e) { view.innerHTML = `<div class="ch-empty">${esc(String(e))}</div>`; }
  }
}

// ---------------------------------------------------------------- settings
function renderSettings(main) {
  const st = S.settings;
  const tabs = [['general', 'General', 'gear'], ['endpoints', 'Endpoints', 'cpu'], ['about', 'About', 'info']];
  let body = '';
  if (S.settingsTab === 'general') {
    body = `<h1>General</h1><p class="lead">How Forge looks and how new sessions start.</p>
      <div class="set-section"><h2>Appearance</h2><div class="card">
        <div class="field"><div class="fl"><div class="t">Theme</div><div class="d">Follow Windows, or pick one.</div></div>
          <div class="seg">${['system', 'light', 'dark'].map((t) => `<button class="${st.theme === t ? 'on' : ''}" data-theme="${t}">${t[0].toUpperCase() + t.slice(1)}</button>`).join('')}</div></div>
      </div></div>
      <div class="set-section"><h2>New sessions</h2><div class="card">
        <div class="field"><div class="fl"><div class="t">Permission mode</div><div class="d">${esc(modeOf(st.defaultMode).desc)}</div></div>
          <select class="input" id="set-mode">${MODES.map((m) => `<option value="${m.id}" ${st.defaultMode === m.id ? 'selected' : ''}>${esc(m.label)}</option>`).join('')}</select></div>
        <div class="field"><div class="fl"><div class="t">Model</div><div class="d">An alias (opus, sonnet, haiku) or a full model name. Empty uses Claude Code's default.</div></div>
          <input class="input" id="set-model" placeholder="Default" value="${esc(st.defaultModel ?? '')}" /></div>
      </div></div>`;
  } else if (S.settingsTab === 'endpoints') {
    const pe = S.profileEdit;
    body = `<h1>Endpoints</h1><p class="lead">Where Claude Code sends requests. Use your Claude account, or any Anthropic-compatible server such as Ollama, LM Studio or a gateway.</p>
      <div class="set-section"><h2>Use for new sessions</h2><div class="card">
        <div class="prow ${!st.activeProfile ? 'on' : ''}"><span class="radio"></span><div class="pl" data-profile-use=""><div class="pn">Claude account</div><div class="pu">claude login · or ANTHROPIC_API_KEY</div></div></div>
        ${st.profiles.map((p) => `<div class="prow ${st.activeProfile === p.id ? 'on' : ''}"><span class="radio"></span><div class="pl" data-profile-use="${esc(p.id)}"><div class="pn">${esc(p.name)}${p.auth !== 'none' && !p.hasKey ? ' <span class="err-text">· no key</span>' : ''}</div><div class="pu">${esc(p.baseUrl)}${p.model ? ` · ${esc(p.model)}` : ''}</div></div>
          <button class="btn sm ghost" data-profile-edit="${esc(p.id)}">Edit</button><button class="icon-btn" data-profile-del="${esc(p.id)}" title="Remove">${icon('trash', 'sm')}</button></div>`).join('')}
      </div>
      ${pe ? '' : `<div style="margin-top:12px"><button class="btn" data-act="profile-add">${icon('plus', 'sm')}Add endpoint</button></div>`}
      </div>
      ${pe ? `<div class="set-section"><h2>${pe.isNew ? 'New endpoint' : `Edit ${esc(pe.name)}`}</h2><div class="card"><form class="form" id="profile-form" autocomplete="off">
        <label>Name</label><input class="input" name="name" value="${esc(pe.name)}" placeholder="Ollama on this PC" required />
        <label>Base URL</label><input class="input" name="baseUrl" value="${esc(pe.baseUrl)}" placeholder="http://127.0.0.1:11434" required />
        <div class="help">The server must speak the Anthropic Messages API (Ollama 0.14+ and LM Studio 0.4+ do).</div>
        <label>Model</label><input class="input" name="model" value="${esc(pe.model)}" placeholder="qwen3-coder:30b" />
        <label>Authentication</label><select class="input" name="auth">${[['none', 'None (local server)'], ['apiKey', 'API key (x-api-key)'], ['bearer', 'Bearer token']].map(([v, l]) => `<option value="${v}" ${pe.auth === v ? 'selected' : ''}>${l}</option>`).join('')}</select>
        <label>Key</label><input class="input" name="key" type="password" placeholder="${pe.hasKey ? 'Saved in Windows Credential Manager · type to replace' : 'Stored in Windows Credential Manager'}" />
        ${pe.error ? `<div></div><div class="err-text">${esc(pe.error)}</div>` : ''}
        <div class="actions"><button type="button" class="btn ghost" data-act="profile-cancel">Cancel</button><button class="btn primary" type="submit">Save</button></div>
      </form></div></div>` : ''}`;
  } else {
    body = `<h1>About</h1><p class="lead">Forge Desktop ${esc(S.info?.version ?? '')}</p>
      <div class="set-section"><div class="card">
        <div class="about"><div class="logo">${icon('sparkle', 'lg')}</div><div><div style="font-weight:650">Forge</div><div style="color:var(--muted);font-size:13px">A desktop app for Claude Code</div></div></div>
        <div class="field"><div class="fl"><div class="t">Claude Code</div><div class="d" style="font-family:var(--mono)">${esc(S.info?.claude ?? 'Not found on PATH')}</div></div></div>
        <div class="field"><div class="fl"><div class="t">Conversations</div><div class="d">Saved by Claude Code under ~/.claude/projects, shared with the terminal.</div></div></div>
        <div class="field"><div class="fl"><div class="t">Shortcuts</div><div class="d">Ctrl+N new session · Ctrl+B sidebar · Ctrl+Shift+D changes · Ctrl+, settings · Shift+Tab mode · Esc stop</div></div></div>
      </div></div>`;
  }
  main.innerHTML = `<div class="settings">
      <nav class="set-nav"><button class="btn ghost sm back" data-act="settings-close">${icon('chevronLeft', 'sm')}Back to chat</button>
        ${tabs.map(([id, label, ic]) => `<button class="nav ${S.settingsTab === id ? 'on' : ''}" data-tab="${id}">${icon(ic)}${label}</button>`).join('')}</nav>
      <div class="set-body"><div class="set-inner">${body}</div></div>
    </div>`;
}

async function saveSettings(patch) {
  try {
    S.settings = await invoke('settings_save', { settings: { ...S.settings, ...patch } });
    applyLayout();
    return true;
  } catch (e) { fail(e); return false; }
}

async function submitProfile(form) {
  const pe = S.profileEdit;
  const data = Object.fromEntries(new FormData(form));
  const profile = { id: pe.id, name: data.name.trim(), baseUrl: data.baseUrl.trim(), model: data.model.trim(), auth: data.auth };
  const profiles = pe.isNew ? [...S.settings.profiles, profile] : S.settings.profiles.map((p) => (p.id === pe.id ? profile : p));
  try {
    S.settings = await invoke('settings_save', { settings: { ...S.settings, profiles } });
    if (data.key) await invoke('profile_key', { id: profile.id, key: data.key });
    S.settings = await invoke('settings_get');
    S.profileEdit = null;
    toast('Endpoint saved');
  } catch (e) { pe.error = String(e); Object.assign(pe, profile); }
  renderAll();
}

// ---------------------------------------------------------------- events wiring
app.addEventListener('click', async (e) => {
  const t = e.target;
  const act = t.closest('[data-act]')?.dataset.act;
  if (act) {
    const el = t.closest('[data-act]');
    switch (act) {
      case 'sidebar': S.sidebar = !S.sidebar; applyLayout(); return;
      case 'project': return projectMenu(el);
      case 'changes':
        if (S.view !== 'chat') { S.view = 'chat'; S.changes = true; return renderAll(); }
        S.changes = !S.changes; applyLayout(); renderTitlebar(); renderChanges(); return;
      case 'settings': S.view = S.view === 'settings' ? 'chat' : 'settings'; renderAll(); return;
      case 'settings-close': S.view = 'chat'; renderAll(); return;
      case 'endpoints': S.view = 'settings'; S.settingsTab = 'endpoints'; renderAll(); return;
      case 'min': return invoke('window_action', { action: 'minimize' });
      case 'max': await invoke('window_action', { action: 'toggleMaximize' }); return syncMax();
      case 'close': return invoke('window_action', { action: 'close' });
      case 'new': return newSession();
      case 'mode': return modeMenu(el);
      case 'model': return modelMenu(el);
      case 'recheck': S.info = await invoke('app_info'); return renderAll();
      case 'git-refresh': return refreshGit().then(renderChanges);
      case 'reveal': return invoke('reveal', { path: el.dataset.path }).catch(fail);
      case 'discard': {
        const ok = await confirmModal({ title: `Discard changes to ${baseName(el.dataset.path)}?`, body: 'The file goes back to the last commit. A new file is deleted. This cannot be undone.', ok: 'Discard', danger: true });
        if (ok) { await invoke('git_discard', { path: el.dataset.path }).catch(fail); await refreshGit(); renderChanges(); }
        return;
      }
      case 'profile-add': S.profileEdit = { isNew: true, id: `ep-${Date.now().toString(36)}`, name: '', baseUrl: 'http://127.0.0.1:11434', model: '', auth: 'none' }; return renderMain();
      case 'profile-cancel': S.profileEdit = null; return renderMain();
    }
  }
  const more = t.closest('[data-more]');
  if (more) { e.stopPropagation(); return sessionMenu(more, more.dataset.more); }
  const row = t.closest('.srow[data-id]');
  if (row) return openSession(row.dataset.id);
  const sug = t.closest('[data-suggest]');
  if (sug) { const i = $('#input'); i.value = sug.dataset.suggest; autosize(i); i.focus(); renderComposerState(); return; }
  const copy = t.closest('[data-copy]');
  if (copy) { navigator.clipboard?.writeText(copy.dataset.copy); toast('Copied'); return; }
  if (t.closest('#send')) { const i = $('#input'); if (i.value.trim()) return send(i.value); return stop(); }
  const toggle = t.closest('[data-toggle]');
  if (toggle) { const s = session(S.current); const it = s.items[+toggle.dataset.toggle]; it.open = !it.open; renderThread(); return; }
  const perm = t.closest('[data-perm]');
  if (perm && !perm.disabled) return respond(perm.closest('[data-req]').dataset.req, perm.dataset.perm);
  const opt = t.closest('.opt[data-q]');
  if (opt) {
    const s = session(S.current);
    const it = s.items.find((i) => i.requestId === opt.closest('[data-req]').dataset.req);
    const q = it.input.questions[+opt.dataset.q];
    const label = q.options[+opt.dataset.o].label;
    const cur = it.answers[q.question] ?? [];
    it.answers[q.question] = q.multiSelect ? (cur.includes(label) ? cur.filter((x) => x !== label) : [...cur, label]) : [label];
    return renderThread();
  }
  const a = t.closest('a[data-href]');
  if (a) { e.preventDefault(); return invoke('open_url', { url: a.dataset.href }).catch(fail); }
  const file = t.closest('[data-file]');
  if (file) { S.changesSel = file.dataset.file; return renderChanges(); }
  const tab = t.closest('[data-tab]');
  if (tab) { S.settingsTab = tab.dataset.tab; S.profileEdit = null; return renderMain(); }
  const theme = t.closest('[data-theme]');
  if (theme) { await saveSettings({ theme: theme.dataset.theme }); return renderMain(); }
  const use = t.closest('[data-profile-use]');
  if (use) { await saveSettings({ activeProfile: use.dataset.profileUse || null }); renderSidebar(); return renderMain(); }
  const pedit = t.closest('[data-profile-edit]');
  if (pedit) { const p = S.settings.profiles.find((x) => x.id === pedit.dataset.profileEdit); S.profileEdit = { ...p }; return renderMain(); }
  const pdel = t.closest('[data-profile-del]');
  if (pdel) {
    const p = S.settings.profiles.find((x) => x.id === pdel.dataset.profileDel);
    if (!(await confirmModal({ title: `Remove ${p.name}?`, body: 'Its key is removed from Windows Credential Manager too.', ok: 'Remove' }))) return;
    await invoke('profile_key', { id: p.id, key: null }).catch(() => {});
    await saveSettings({ profiles: S.settings.profiles.filter((x) => x.id !== p.id), activeProfile: S.settings.activeProfile === p.id ? null : S.settings.activeProfile });
    renderSidebar();
    return renderMain();
  }
});

app.addEventListener('toggle', (e) => {
  const d = e.target.closest?.('details[data-idx]');
  if (d && S.current) { const it = session(S.current).items[+d.dataset.idx]; if (it) it.open = d.open; }
}, true);

app.addEventListener('input', (e) => {
  if (e.target.id === 'input') {
    autosize(e.target);
    if (S.current) session(S.current).draft = e.target.value;
    renderComposerState();
  } else if (e.target.id === 'search') {
    S.search = e.target.value;
    const pos = e.target.selectionStart;
    renderSidebar();
    const i = $('#search'); i.focus(); i.setSelectionRange(pos, pos);
  }
});

app.addEventListener('change', async (e) => {
  if (e.target.id === 'set-mode') { await saveSettings({ defaultMode: e.target.value }); renderMain(); }
  if (e.target.id === 'set-model') await saveSettings({ defaultModel: e.target.value.trim() || null });
});

app.addEventListener('submit', (e) => {
  if (e.target.id === 'profile-form') { e.preventDefault(); submitProfile(e.target); }
});

app.addEventListener('keydown', (e) => {
  if (e.target.id === 'input') {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); send(e.target.value); }
    else if (e.key === 'Tab' && e.shiftKey) {
      e.preventDefault();
      const s = S.current ? session(S.current) : null;
      const order = ['default', 'acceptEdits', 'plan'];
      const cur = s?.mode ?? S.settings.defaultMode;
      setMode(order[(order.indexOf(cur) + 1) % order.length]).then(() => $('#input')?.focus());
    }
  }
  if (e.target.id === 'rename-input') {
    if (e.key === 'Enter') commitRename(S.editing, e.target.value);
    if (e.key === 'Escape') { S.editing = null; renderSidebar(); }
  }
});
app.addEventListener('focusout', (e) => { if (e.target.id === 'rename-input' && S.editing) commitRename(S.editing, e.target.value); });

document.addEventListener('keydown', (e) => {
  const mod = e.ctrlKey || e.metaKey;
  if (e.key === 'Escape') {
    if (openMenu) return closeMenu();
    if (S.view === 'settings') { S.view = 'chat'; return renderAll(); }
    const s = S.current && S.sessions[S.current];
    if (s?.busy) return stop();
  }
  if (!mod) return;
  const k = e.key.toLowerCase();
  if (k === 'n' && !e.shiftKey) { e.preventDefault(); newSession(); }
  else if (k === 'b') { e.preventDefault(); S.sidebar = !S.sidebar; applyLayout(); }
  else if (k === 'd' && e.shiftKey) { e.preventDefault(); $('[data-act="changes"]')?.click(); }
  else if (e.key === ',') { e.preventDefault(); S.view = 'settings'; renderAll(); }
});

// Drag to resize the sidebar and the Changes pane; widths are remembered.
app.addEventListener('mousedown', (e) => {
  const r = e.target.closest('[data-resize]');
  if (!r) return;
  e.preventDefault();
  const which = r.dataset.resize;
  const startX = e.clientX;
  const el = which === 'sidebar' ? $('#sidebar') : $('#changes');
  const startW = el.getBoundingClientRect().width;
  const [lo, hi] = which === 'sidebar' ? [200, 480] : [320, 900];
  let w = startW;
  const move = (ev) => {
    w = Math.round(Math.max(lo, Math.min(hi, which === 'sidebar' ? startW + ev.clientX - startX : startW - (ev.clientX - startX))));
    document.documentElement.style.setProperty(which === 'sidebar' ? '--sidebar-w' : '--changes-w', `${w}px`);
  };
  const up = () => {
    removeEventListener('mousemove', move); removeEventListener('mouseup', up);
    saveSettings(which === 'sidebar' ? { sidebarWidth: w } : { changesWidth: w });
  };
  addEventListener('mousemove', move);
  addEventListener('mouseup', up);
});

function renderAll() {
  applyLayout();
  renderTitlebar();
  renderSidebar();
  renderMain();
  renderChanges();
}

// ---------------------------------------------------------------- start
async function start() {
  shell();
  const channel = new tauri.core.Channel();
  channel.onmessage = onEvent;
  await invoke('events_attach', { channel });
  [S.info, S.settings] = await Promise.all([invoke('app_info'), invoke('settings_get')]);
  for (const id of S.info.running ?? []) session(id).running = true;
  renderAll();
  await Promise.all([refreshList(), refreshGit()]);
  syncMax();
  addEventListener('resize', () => { clearTimeout(start.rt); start.rt = setTimeout(syncMax, 150); });
  addEventListener('focus', () => { refreshGit(); refreshList(); });
  setInterval(refreshGit, 4000);
  $('#input')?.focus();
}

start().catch((e) => {
  console.error(e);
  app.innerHTML = `<div class="empty"><div class="setup"><h2>Forge could not start</h2><p>${esc(String(e))}</p></div></div>`;
});
