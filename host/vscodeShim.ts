/**
 * The `vscode` module, for Forge's host running outside VS Code (plan §4.5 M1).
 *
 * The build aliases `vscode` to this file, so Forge's own `extension.ts`,
 * services and handlers run unchanged. It implements what the chat path
 * touches, on Node and on the Rust shell:
 *
 * - configuration: a JSON file (`<data>/settings.json`), with the defaults
 *   from Forge's package.json;
 * - `globalState` / `workspaceState`: `<data>/state.json`, written atomically;
 * - `workspace.fs`: `node:fs`;
 * - the output channel: the host log file;
 * - messages: logged, and answered with "no choice" (a confirmation is never
 *   taken as a yes);
 * - dialogs and `openExternal`: host calls to Rust.
 *
 * Every member that is not here is a stub: reading it is harmless, calling or
 * constructing it throws `Forge Desktop: vscode.<name> is not available yet`,
 * which the dispatcher turns into a failed request (ClaudeAgentService keeps
 * going). The shim is deleted in M3, when real host interfaces replace it.
 */

import * as fs from 'node:fs';
import * as fsp from 'node:fs/promises';
import * as path from 'node:path';
import { env, hostCall, log } from './runtime';

// ---------------------------------------------------------------------------
// Stubs for whatever is not implemented
// ---------------------------------------------------------------------------

function unavailable(name: string): never {
	const error = new Error(`Forge Desktop: vscode.${name} is not available yet`);
	log('warn', error.message);
	throw error;
}

/**
 * A value that can be read through, compared and `instanceof`-tested, but
 * fails loudly when called or constructed. `then` is undefined so awaiting it
 * does not hang.
 */
function stub(name: string): any {
	const target = function () { /* stub */ };
	return new Proxy(target, {
		get(_t, prop) {
			if (prop === 'then' || prop === Symbol.toPrimitive || prop === 'toJSON') return undefined;
			if (prop === 'prototype') return target.prototype;
			if (prop === Symbol.hasInstance) return () => false;
			if (typeof prop === 'symbol') return undefined;
			return stub(`${name}.${prop}`);
		},
		apply() {
			return unavailable(name);
		},
		construct() {
			return unavailable(name);
		},
	});
}

/** Wrap a namespace so unknown members become stubs. */
function namespace<T extends object>(name: string, members: T): T {
	return new Proxy(members, {
		get(target, prop, receiver) {
			if (prop in target || typeof prop === 'symbol') return Reflect.get(target, prop, receiver);
			return stub(`${name}.${String(prop)}`);
		},
	});
}

// ---------------------------------------------------------------------------
// Events and disposables
// ---------------------------------------------------------------------------

export class Disposable {
	static from(...items: { dispose(): unknown }[]): Disposable {
		return new Disposable(() => items.forEach((i) => i?.dispose()));
	}
	constructor(private readonly callOnDispose: () => unknown) {}
	dispose(): void {
		this.callOnDispose();
	}
}

export type Event<T> = (listener: (e: T) => unknown, thisArgs?: unknown, disposables?: Disposable[]) => Disposable;

export class EventEmitter<T> {
	private listeners = new Set<(e: T) => unknown>();
	readonly event: Event<T> = (listener, thisArgs, disposables) => {
		const bound = thisArgs ? listener.bind(thisArgs) : listener;
		this.listeners.add(bound);
		const d = new Disposable(() => this.listeners.delete(bound));
		disposables?.push(d);
		return d;
	};
	fire(e: T): void {
		for (const l of [...this.listeners]) {
			try {
				l(e);
			} catch (error) {
				log('error', '[vscodeShim] an event listener threw', error);
			}
		}
	}
	dispose(): void {
		this.listeners.clear();
	}
}

/** An event that never fires: editors, terminals, diagnostics. */
const never: Event<any> = (_l, _t, disposables) => {
	const d = new Disposable(() => {});
	disposables?.push(d);
	return d;
};

export class CancellationTokenSource {
	private readonly emitter = new EventEmitter<void>();
	readonly token = { isCancellationRequested: false, onCancellationRequested: this.emitter.event };
	cancel(): void {
		if (this.token.isCancellationRequested) return;
		this.token.isCancellationRequested = true;
		this.emitter.fire();
	}
	dispose(): void {
		this.emitter.dispose();
	}
}

// ---------------------------------------------------------------------------
// Uri and small value classes
// ---------------------------------------------------------------------------

export class Uri {
	private constructor(
		readonly scheme: string,
		readonly authority: string,
		readonly path: string,
		readonly query: string,
		readonly fragment: string,
	) {}

	static file(fsPath: string): Uri {
		let p = fsPath.replace(/\\/g, '/');
		if (!p.startsWith('/')) p = '/' + p;
		return new Uri('file', '', p, '', '');
	}

	static parse(value: string): Uri {
		const m = /^([a-zA-Z][\w+.-]*):(?:\/\/([^/?#]*))?([^?#]*)(?:\?([^#]*))?(?:#(.*))?$/.exec(value);
		if (!m) return Uri.file(value);
		return new Uri(m[1], m[2] ?? '', decodeURIComponent(m[3] ?? ''), m[4] ?? '', m[5] ?? '');
	}

	static joinPath(base: Uri, ...segments: string[]): Uri {
		return base.with({ path: path.posix.join(base.path, ...segments) });
	}

	static from(c: { scheme: string; authority?: string; path?: string; query?: string; fragment?: string }): Uri {
		return new Uri(c.scheme, c.authority ?? '', c.path ?? '', c.query ?? '', c.fragment ?? '');
	}

	get fsPath(): string {
		// "/C:/x" -> "C:\x" on Windows; the path as-is elsewhere.
		if (/^\/[a-zA-Z]:/.test(this.path)) {
			const p = this.path.slice(1);
			return process.platform === 'win32' ? p.replace(/\//g, '\\') : p;
		}
		return process.platform === 'win32' ? this.path.replace(/\//g, '\\') : this.path;
	}

	with(change: { scheme?: string; authority?: string; path?: string; query?: string; fragment?: string }): Uri {
		return new Uri(
			change.scheme ?? this.scheme,
			change.authority ?? this.authority,
			change.path ?? this.path,
			change.query ?? this.query,
			change.fragment ?? this.fragment,
		);
	}

	toString(): string {
		const q = this.query ? `?${this.query}` : '';
		const f = this.fragment ? `#${this.fragment}` : '';
		if (this.scheme === 'file') return `file://${encodeURI(this.path)}${q}${f}`;
		return `${this.scheme}://${this.authority}${this.path}${q}${f}`;
	}

	toJSON(): unknown {
		return { scheme: this.scheme, path: this.path, fsPath: this.fsPath };
	}
}

export class Position {
	constructor(readonly line: number, readonly character: number) {}
}

export class Range {
	readonly start: Position;
	readonly end: Position;
	constructor(a: Position | number, b: Position | number, c?: number, d?: number) {
		this.start = typeof a === 'number' ? new Position(a, b as number) : a;
		this.end = typeof a === 'number' ? new Position(c!, d!) : (b as Position);
	}
	get isEmpty(): boolean {
		return this.start.line === this.end.line && this.start.character === this.end.character;
	}
}

export class Selection extends Range {
	constructor(readonly anchor: Position, readonly active: Position) {
		super(anchor, active);
	}
}

export class ThemeColor {
	constructor(readonly id: string) {}
}

export class ThemeIcon {
	constructor(readonly id: string, readonly color?: ThemeColor) {}
}

export class MarkdownString {
	constructor(public value = '') {}
}

export class RelativePattern {
	constructor(readonly base: unknown, readonly pattern: string) {}
}

// Tabs: there are none, so nothing is ever an instance of these.
export class TabInputWebview { constructor(readonly viewType: string) {} }
export class TabInputText { constructor(readonly uri: Uri) {} }
export class TabInputTextDiff { constructor(readonly original: Uri, readonly modified: Uri) {} }

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

export enum ExtensionMode { Production = 1, Development = 2, Test = 3 }
export enum ConfigurationTarget { Global = 1, Workspace = 2, WorkspaceFolder = 3 }
export enum ViewColumn { Active = -1, Beside = -2, One = 1, Two, Three, Four, Five, Six, Seven, Eight, Nine }
export enum ProgressLocation { SourceControl = 1, Window = 10, Notification = 15 }
export enum FileType { Unknown = 0, File = 1, Directory = 2, SymbolicLink = 64 }
export enum TextEditorRevealType { Default = 0, InCenter = 1, InCenterIfOutsideViewport = 2, AtTop = 3 }
export enum OverviewRulerLane { Left = 1, Center = 2, Right = 4, Full = 7 }
export enum DiagnosticSeverity { Error = 0, Warning = 1, Information = 2, Hint = 3 }
export enum TextEditorSelectionChangeKind { Keyboard = 1, Mouse = 2, Command = 3 }
export enum StatusBarAlignment { Left = 1, Right = 2 }
export enum UIKind { Desktop = 1, Web = 2 }
export enum LogLevel { Off = 0, Trace = 1, Debug = 2, Info = 3, Warning = 4, Error = 5 }

// ---------------------------------------------------------------------------
// JSON files: settings and state
// ---------------------------------------------------------------------------

function readJson(file: string): Record<string, unknown> {
	try {
		const value = JSON.parse(fs.readFileSync(file, 'utf8'));
		return value && typeof value === 'object' && !Array.isArray(value) ? value : {};
	} catch {
		return {};
	}
}

function writeJsonAtomic(file: string, value: unknown): void {
	fs.mkdirSync(path.dirname(file), { recursive: true });
	const tmp = `${file}.${process.pid}.tmp`;
	fs.writeFileSync(tmp, JSON.stringify(value, null, 2) + '\n');
	fs.renameSync(tmp, file);
}

/** Forge's package.json, staged next to dist/ (the extension's packageJSON). */
export function readPackageJson(): any {
	try {
		return JSON.parse(fs.readFileSync(path.join(env.root, 'package.json'), 'utf8'));
	} catch {
		return {};
	}
}

const packageJson = readPackageJson();

/**
 * Defaults that differ on the desktop. `forge.followEdits` opens and
 * highlights each edited file in an editor; there is no editor here (plan §2.1 #22).
 */
const DESKTOP_DEFAULTS: Record<string, unknown> = {
	'forge.followEdits': false,
};

function contributedDefaults(): Record<string, unknown> {
	const config = packageJson?.contributes?.configuration;
	const blocks = Array.isArray(config) ? config : config ? [config] : [];
	const out: Record<string, unknown> = {};
	for (const block of blocks) {
		for (const [key, schema] of Object.entries<any>(block?.properties ?? {})) {
			if (schema && 'default' in schema) out[key] = schema.default;
		}
	}
	return { ...out, ...DESKTOP_DEFAULTS };
}

const defaults = contributedDefaults();
const settingsFile = () => path.join(env.dataDir, 'settings.json');
const configEmitter = new EventEmitter<{ affectsConfiguration(section: string): boolean }>();

function readSettings(): Record<string, unknown> {
	return readJson(settingsFile());
}

function lookup(source: Record<string, unknown>, full: string): unknown {
	if (full in source) return source[full];
	// A nested object written by hand: {"forge": {"endpoints": {...}}}.
	let node: any = source;
	for (const part of full.split('.')) {
		if (node && typeof node === 'object' && part in node) node = node[part];
		else return undefined;
	}
	return node;
}

function getConfiguration(section?: string) {
	const full = (key: string) => (section ? `${section}.${key}` : key);
	return {
		get<T>(key: string, fallback?: T): T | undefined {
			const user = lookup(readSettings(), full(key));
			if (user !== undefined) return user as T;
			const def = lookup(defaults, full(key));
			return (def !== undefined ? def : fallback) as T | undefined;
		},
		has(key: string): boolean {
			return lookup(readSettings(), full(key)) !== undefined || lookup(defaults, full(key)) !== undefined;
		},
		inspect<T>(key: string) {
			return {
				key: full(key),
				defaultValue: lookup(defaults, full(key)) as T | undefined,
				globalValue: lookup(readSettings(), full(key)) as T | undefined,
				workspaceValue: undefined as T | undefined,
				workspaceFolderValue: undefined as T | undefined,
			};
		},
		async update(key: string, value: unknown, _target?: unknown): Promise<void> {
			const settings = readSettings();
			if (value === undefined) delete settings[full(key)];
			else settings[full(key)] = value;
			writeJsonAtomic(settingsFile(), settings);
			const changed = full(key);
			configEmitter.fire({
				affectsConfiguration: (s: string) => changed === s || changed.startsWith(`${s}.`) || s.startsWith(`${changed}.`),
			});
		},
	};
}

class Memento {
	private cache: Record<string, unknown>;
	constructor(private readonly file: string) {
		this.cache = readJson(file);
	}
	keys(): readonly string[] {
		return Object.keys(this.cache);
	}
	get<T>(key: string, fallback?: T): T | undefined {
		return key in this.cache ? (this.cache[key] as T) : fallback;
	}
	async update(key: string, value: unknown): Promise<void> {
		if (value === undefined) delete this.cache[key];
		else this.cache[key] = value;
		writeJsonAtomic(this.file, this.cache);
	}
	setKeysForSync(): void {}
}

/**
 * Endpoint tokens. Kept in memory only until M5 moves them to Windows
 * Credential Manager: a token never lands in a plain file.
 */
class MemorySecrets {
	private readonly values = new Map<string, string>();
	private readonly emitter = new EventEmitter<{ key: string }>();
	readonly onDidChange = this.emitter.event;
	async get(key: string): Promise<string | undefined> {
		return this.values.get(key);
	}
	async store(key: string, value: string): Promise<void> {
		this.values.set(key, value);
		this.emitter.fire({ key });
	}
	async delete(key: string): Promise<void> {
		this.values.delete(key);
		this.emitter.fire({ key });
	}
	async keys(): Promise<string[]> {
		return [...this.values.keys()];
	}
}

// ---------------------------------------------------------------------------
// workspace
// ---------------------------------------------------------------------------

const workspaceFolders = env.workspace
	? [{ uri: Uri.file(env.workspace), name: path.basename(env.workspace), index: 0 }]
	: undefined;

function fileTypeOf(stat: fs.Stats): FileType {
	let type = stat.isDirectory() ? FileType.Directory : stat.isFile() ? FileType.File : FileType.Unknown;
	if (stat.isSymbolicLink()) type |= FileType.SymbolicLink;
	return type;
}

function notFound(uri: Uri, error: unknown): Error {
	const e = error as NodeJS.ErrnoException;
	if (e?.code === 'ENOENT') {
		const out = new Error(`EntryNotFound (FileSystemError): ${uri.fsPath}`) as Error & { code: string };
		out.code = 'FileNotFound';
		return out;
	}
	return e instanceof Error ? e : new Error(String(e));
}

const workspaceFs = {
	async readFile(uri: Uri): Promise<Uint8Array> {
		try {
			return await fsp.readFile(uri.fsPath);
		} catch (e) {
			throw notFound(uri, e);
		}
	},
	async writeFile(uri: Uri, content: Uint8Array): Promise<void> {
		await fsp.mkdir(path.dirname(uri.fsPath), { recursive: true });
		await fsp.writeFile(uri.fsPath, content);
	},
	async delete(uri: Uri, options?: { recursive?: boolean }): Promise<void> {
		try {
			await fsp.rm(uri.fsPath, { recursive: options?.recursive ?? false });
		} catch (e) {
			throw notFound(uri, e);
		}
	},
	async rename(source: Uri, target: Uri, options?: { overwrite?: boolean }): Promise<void> {
		if (!options?.overwrite && fs.existsSync(target.fsPath)) throw new Error(`FileExists: ${target.fsPath}`);
		await fsp.rename(source.fsPath, target.fsPath);
	},
	async copy(source: Uri, target: Uri): Promise<void> {
		await fsp.cp(source.fsPath, target.fsPath, { recursive: true });
	},
	async createDirectory(uri: Uri): Promise<void> {
		await fsp.mkdir(uri.fsPath, { recursive: true });
	},
	async readDirectory(uri: Uri): Promise<[string, FileType][]> {
		try {
			const entries = await fsp.readdir(uri.fsPath, { withFileTypes: true });
			return entries.map((e) => [e.name, e.isDirectory() ? FileType.Directory : e.isFile() ? FileType.File : FileType.Unknown]);
		} catch (e) {
			throw notFound(uri, e);
		}
	},
	async stat(uri: Uri) {
		try {
			const s = await fsp.stat(uri.fsPath);
			return { type: fileTypeOf(s), ctime: s.ctimeMs, mtime: s.mtimeMs, size: s.size, permissions: undefined };
		} catch (e) {
			throw notFound(uri, e);
		}
	},
};

export const workspace = namespace('workspace', {
	get workspaceFolders() {
		return workspaceFolders;
	},
	get rootPath() {
		return env.workspace;
	},
	name: env.workspace ? path.basename(env.workspace) : undefined,
	isTrusted: true,
	textDocuments: [] as unknown[],
	fs: workspaceFs,
	getConfiguration,
	onDidChangeConfiguration: configEmitter.event,
	onDidChangeWorkspaceFolders: never,
	onDidChangeTextDocument: never,
	onDidCloseTextDocument: never,
	onDidOpenTextDocument: never,
	onDidSaveTextDocument: never,
	onDidCreateFiles: never,
	onDidDeleteFiles: never,
	onDidRenameFiles: never,
	getWorkspaceFolder(uri: Uri) {
		const folder = workspaceFolders?.[0];
		if (!folder) return undefined;
		const rel = path.relative(folder.uri.fsPath, uri.fsPath);
		return rel.startsWith('..') || path.isAbsolute(rel) ? undefined : folder;
	},
	asRelativePath(value: Uri | string): string {
		const p = typeof value === 'string' ? value : value.fsPath;
		const folder = workspaceFolders?.[0];
		if (!folder) return p;
		const rel = path.relative(folder.uri.fsPath, p);
		return rel.startsWith('..') || path.isAbsolute(rel) ? p : rel.split(path.sep).join('/');
	},
	/** The ripgrep path is the primary search; this fallback finds nothing. */
	async findFiles(): Promise<Uri[]> {
		return [];
	},
	createFileSystemWatcher() {
		const d = { dispose() {} };
		return { onDidCreate: never, onDidChange: never, onDidDelete: never, ...d };
	},
});

// ---------------------------------------------------------------------------
// window
// ---------------------------------------------------------------------------

/** The webview view providers Forge registers, by view id. main.ts resolves the chat one. */
export const registeredViewProviders = new Map<string, any>();

/** Messages with buttons: log, and answer "no choice". A confirmation is never a yes. */
function message(level: 'info' | 'warn' | 'error') {
	return async (text: string, ...rest: unknown[]): Promise<undefined> => {
		const actions = rest.filter((r) => typeof r === 'string' || (r && typeof r === 'object' && 'title' in (r as object)));
		log(level, `[message] ${text}${actions.length ? ` (actions: ${actions.map((a: any) => a.title ?? a).join(', ')})` : ''}`);
		return undefined;
	};
}

function outputChannel(name: string) {
	const write = (level: string) => (m: unknown, ...a: unknown[]) => log(level, `[${name}]`, m, ...a);
	return {
		name,
		logLevel: LogLevel.Trace,
		onDidChangeLogLevel: never,
		append: write('info'),
		appendLine: write('info'),
		replace: write('info'),
		clear() {},
		show() {
			void hostCall('shell.showInFolder', { path: path.join(env.logDir, 'forge-host.log') }).catch(() => {});
		},
		hide() {},
		dispose() {},
		trace: write('trace'),
		debug: write('debug'),
		info: write('info'),
		warn: write('warn'),
		error: write('error'),
	};
}

function toFilters(filters?: Record<string, string[]>) {
	return filters ? Object.entries(filters).map(([name, extensions]) => ({ name, extensions })) : undefined;
}

export const window = namespace('window', {
	activeTextEditor: undefined,
	visibleTextEditors: [] as unknown[],
	terminals: [] as unknown[],
	activeTerminal: undefined,
	tabGroups: {
		all: [] as unknown[],
		activeTabGroup: { viewColumn: 1, tabs: [] as unknown[], isActive: true },
		close: async () => true,
		onDidChangeTabs: never,
		onDidChangeTabGroups: never,
	},
	onDidChangeActiveTextEditor: never,
	onDidChangeVisibleTextEditors: never,
	onDidChangeTextEditorSelection: never,
	onDidChangeWindowState: never,
	onDidCloseTerminal: never,
	onDidOpenTerminal: never,
	onDidChangeTerminalShellIntegration: never,
	onDidEndTerminalShellExecution: never,
	onDidStartTerminalShellExecution: never,
	state: { focused: true, active: true },
	createOutputChannel: (name: string) => outputChannel(name),
	showInformationMessage: message('info'),
	showWarningMessage: message('warn'),
	showErrorMessage: message('error'),
	async showQuickPick(): Promise<undefined> {
		log('warn', '[vscodeShim] showQuickPick is not available yet; answered "cancelled"');
		return undefined;
	},
	async showInputBox(): Promise<undefined> {
		log('warn', '[vscodeShim] showInputBox is not available yet; answered "cancelled"');
		return undefined;
	},
	async withProgress<T>(_options: unknown, task: (progress: { report(v: unknown): void }, token: unknown) => Promise<T>): Promise<T> {
		return task({ report() {} }, new CancellationTokenSource().token);
	},
	async showOpenDialog(options?: { canSelectFolders?: boolean; canSelectMany?: boolean; title?: string; defaultUri?: Uri; filters?: Record<string, string[]> }): Promise<Uri[] | undefined> {
		if (options?.canSelectFolders) {
			const folder = await hostCall<string | null>('dialog.openFolder', { title: options.title, defaultPath: options.defaultUri?.fsPath });
			return folder ? [Uri.file(folder)] : undefined;
		}
		const files = await hostCall<string[] | null>('dialog.openFiles', {
			title: options?.title,
			defaultPath: options?.defaultUri?.fsPath,
			multiple: options?.canSelectMany === true,
			filters: toFilters(options?.filters),
		});
		return files?.length ? files.map((f) => Uri.file(f)) : undefined;
	},
	async showSaveDialog(options?: { title?: string; defaultUri?: Uri; filters?: Record<string, string[]> }): Promise<Uri | undefined> {
		const file = await hostCall<string | null>('dialog.saveFile', {
			title: options?.title,
			defaultPath: options?.defaultUri?.fsPath,
			filters: toFilters(options?.filters),
		});
		return file ? Uri.file(file) : undefined;
	},
	registerWebviewViewProvider(viewId: string, provider: unknown): Disposable {
		registeredViewProviders.set(viewId, provider);
		return new Disposable(() => registeredViewProviders.delete(viewId));
	},
	registerUriHandler: () => new Disposable(() => {}),
	createTextEditorDecorationType: () => ({ key: 'forge-desktop', dispose() {} }),
	createStatusBarItem: () => ({ show() {}, hide() {}, dispose() {}, text: '', tooltip: '', command: undefined }),
});

// ---------------------------------------------------------------------------
// commands, env, languages, extensions
// ---------------------------------------------------------------------------

const commandHandlers = new Map<string, (...args: unknown[]) => unknown>();

export const commands = namespace('commands', {
	registerCommand(id: string, handler: (...args: unknown[]) => unknown): Disposable {
		commandHandlers.set(id, handler);
		return new Disposable(() => commandHandlers.delete(id));
	},
	async executeCommand<T>(id: string, ...args: unknown[]): Promise<T | undefined> {
		const handler = commandHandlers.get(id);
		if (handler) return (await handler(...args)) as T;
		// Context keys drive VS Code `when` clauses; there are none here.
		if (id === 'setContext') return undefined;
		log('warn', `[vscodeShim] command ${id} is not available on the desktop`);
		return undefined;
	},
	async getCommands(): Promise<string[]> {
		return [...commandHandlers.keys()];
	},
});

/** Run a registered command (the native menus, later). */
export function runCommand(id: string, ...args: unknown[]): Promise<unknown> {
	return commands.executeCommand(id, ...args);
}

const OPENABLE = /^(https?|mailto):/i;

export const env_ = namespace('env', {
	appName: 'Forge Desktop',
	appRoot: env.root,
	appHost: 'desktop',
	language: 'en',
	machineId: 'forge-desktop',
	sessionId: `forge-desktop-${process.pid}`,
	uriScheme: 'forge-desktop',
	uiKind: UIKind.Desktop,
	remoteName: undefined,
	get shell(): string {
		return process.platform === 'win32' ? 'powershell.exe' : process.env.SHELL || '/bin/sh';
	},
	async openExternal(target: Uri | string): Promise<boolean> {
		const url = typeof target === 'string' ? target : target.toString();
		if (!OPENABLE.test(url)) return false;
		try {
			await hostCall('shell.openUrl', { url });
			return true;
		} catch (error) {
			log('warn', '[vscodeShim] openExternal failed', error);
			return false;
		}
	},
	clipboard: {
		async readText() {
			return '';
		},
		async writeText() {
			unavailable('env.clipboard.writeText');
		},
	},
	asExternalUri: async (uri: Uri) => uri,
});
export { env_ as env };

export const languages = namespace('languages', {
	getDiagnostics: () => [],
	onDidChangeDiagnostics: never,
});

export const extensions = namespace('extensions', {
	all: [] as unknown[],
	onDidChange: never,
	getExtension(id: string) {
		if (id !== 'msaid.forge') return undefined;
		return { id, extensionPath: env.root, extensionUri: Uri.file(env.root), packageJSON: packageJson, isActive: true };
	},
});

export const l10n = { t: (message: string, ..._args: unknown[]) => message };

/**
 * `vscode.version`. Forge gates the secondary side bar on it; the desktop has
 * neither side bar, so any modern number does.
 */
export const version = '1.105.0';

// Namespaces Forge reaches only from VS Code-only features.
export const debug = stub('debug');
export const tasks = stub('tasks');
export const scm = stub('scm');
export const tests = stub('tests');
export const notebooks = stub('notebooks');
export const authentication = stub('authentication');
export const chat = stub('chat');
export const lm = stub('lm');

// ---------------------------------------------------------------------------
// The extension context
// ---------------------------------------------------------------------------

export function createExtensionContext(mode: ExtensionMode = ExtensionMode.Production) {
	const subscriptions: { dispose(): unknown }[] = [];
	const root = env.root;
	return {
		subscriptions,
		extensionPath: root,
		extensionUri: Uri.file(root),
		extensionMode: mode,
		extension: extensions.getExtension('msaid.forge'),
		globalState: new Memento(path.join(env.dataDir, 'state.json')),
		workspaceState: new Memento(path.join(env.dataDir, 'workspace-state.json')),
		secrets: new MemorySecrets(),
		globalStorageUri: Uri.file(path.join(env.dataDir, 'globalStorage')),
		globalStoragePath: path.join(env.dataDir, 'globalStorage'),
		storageUri: Uri.file(path.join(env.dataDir, 'workspaceStorage')),
		storagePath: path.join(env.dataDir, 'workspaceStorage'),
		logUri: Uri.file(env.logDir),
		logPath: env.logDir,
		environmentVariableCollection: stub('ExtensionContext.environmentVariableCollection'),
		asAbsolutePath: (relative: string) => path.join(root, relative),
	};
}
