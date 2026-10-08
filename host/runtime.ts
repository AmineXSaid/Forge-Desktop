/**
 * The sidecar's ground floor: its environment, its log file, and the NDJSON
 * frames it exchanges with the Rust shell over stdin/stdout.
 *
 * Nothing here imports Forge or `vscode`. The shim (vscodeShim.ts) and the
 * entry point (main.ts) build on it.
 *
 * Frames, one JSON object per line:
 *
 *   Rust -> Node  {"$forge":"in","message":<WebViewToExtensionMessage>}
 *                 {"$forge":"return","id":<n>,"result":<any>} | {"$forge":"return","id":<n>,"error":<string>}
 *                 {"$forge":"visibility","visible":<bool>}
 *   Node -> Rust  {"$forge":"ready"}
 *                 {"$forge":"out","message":<{type:"from-extension",message}>}
 *                 {"$forge":"call","id":<n>,"method":<string>,"params":<object>}
 *
 * stdout carries frames only. Everything else a library prints is redirected
 * to the log, and Rust logs any stdout line that is not a `$forge` frame
 * instead of parsing it, so a stray print cannot corrupt the protocol.
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import * as readline from 'node:readline';

export interface DesktopEnv {
	/** The project folder this window chat runs in. */
	workspace: string | undefined;
	/** The staged Forge root: dist/, resources/, package.json (the extension path). */
	root: string;
	/** Where settings.json and state.json live. */
	dataDir: string;
	/** Where the host log lives. */
	logDir: string;
	appVersion: string;
}

function dirFromEnv(name: string): string | undefined {
	const value = process.env[name];
	return value && path.isAbsolute(value) ? value : undefined;
}

export function readEnv(): DesktopEnv {
	// dist/forge-host.cjs sits in <root>/dist, as dist/extension.cjs does.
	const root = dirFromEnv('FORGE_RESOURCES') ?? path.resolve(__dirname, '..');
	const dataDir = dirFromEnv('FORGE_DATA_DIR') ?? path.join(os.homedir(), '.forge-desktop');
	return {
		workspace: dirFromEnv('FORGE_WORKSPACE'),
		root,
		dataDir,
		logDir: dirFromEnv('FORGE_LOG_DIR') ?? path.join(dataDir, 'logs'),
		appVersion: process.env.FORGE_APP_VERSION || '0.0.0',
	};
}

export const env = readEnv();

// ---------------------------------------------------------------------------
// Log file
// ---------------------------------------------------------------------------

const LOG_LIMIT = 5 * 1024 * 1024;
let logFd: number | undefined;

function openLog(): number | undefined {
	if (logFd !== undefined) return logFd;
	try {
		fs.mkdirSync(env.logDir, { recursive: true });
		const file = path.join(env.logDir, 'forge-host.log');
		// One rotation, at startup: forge-host.log -> forge-host.1.log.
		try {
			if (fs.statSync(file).size > LOG_LIMIT) fs.renameSync(file, path.join(env.logDir, 'forge-host.1.log'));
		} catch { /* no log yet */ }
		logFd = fs.openSync(file, 'a');
	} catch {
		logFd = undefined;
	}
	return logFd;
}

function format(value: unknown): string {
	if (typeof value === 'string') return value;
	if (value instanceof Error) return value.stack || value.message;
	try {
		return JSON.stringify(value);
	} catch {
		return String(value);
	}
}

export function log(level: string, message: unknown, ...args: unknown[]): void {
	const line = `${new Date().toISOString()} [${level}] ${[message, ...args].map(format).join(' ')}\n`;
	const fd = openLog();
	if (fd !== undefined) {
		try {
			fs.writeSync(fd, line);
			return;
		} catch { /* fall through to stderr */ }
	}
	process.stderr.write(line);
}

export function logPath(): string {
	return path.join(env.logDir, 'forge-host.log');
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

const writeStdout = process.stdout.write.bind(process.stdout) as (chunk: string) => boolean;

/** Route every console call and any raw stdout write to the log. Call once, first. */
export function redirectConsole(): void {
	console.log = (...a: unknown[]) => log('info', ...(a as [unknown]));
	console.info = (...a: unknown[]) => log('info', ...(a as [unknown]));
	console.debug = (...a: unknown[]) => log('debug', ...(a as [unknown]));
	console.warn = (...a: unknown[]) => log('warn', ...(a as [unknown]));
	console.error = (...a: unknown[]) => log('error', ...(a as [unknown]));
	(process.stdout as any).write = (chunk: unknown, ...rest: unknown[]) => {
		log('stdout', typeof chunk === 'string' ? chunk.trimEnd() : String(chunk));
		const cb = rest.find((r) => typeof r === 'function') as (() => void) | undefined;
		cb?.();
		return true;
	};
}

export function sendFrame(frame: Record<string, unknown> & { $forge: string }): void {
	writeStdout(JSON.stringify(frame) + '\n');
}

// ---------------------------------------------------------------------------
// Host calls: Node -> Rust -> Node
// ---------------------------------------------------------------------------

/** The methods Rust answers (src-tauri/src/host_calls.rs). Anything else is refused there too. */
export type HostMethod =
	| 'dialog.openFolder'
	| 'dialog.openFiles'
	| 'dialog.saveFile'
	| 'shell.openUrl'
	| 'shell.showInFolder'
	| 'window.setTitle'
	| 'app.info';

let nextCallId = 1;
const pending = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void }>();

export function hostCall<T = unknown>(method: HostMethod, params: Record<string, unknown> = {}): Promise<T> {
	const id = nextCallId++;
	return new Promise<T>((resolve, reject) => {
		pending.set(id, { resolve, reject });
		sendFrame({ $forge: 'call', id, method, params });
	});
}

// ---------------------------------------------------------------------------
// Inbound
// ---------------------------------------------------------------------------

export interface InboundHandlers {
	message(message: unknown): void;
	visibility(visible: boolean): void;
}

/** Parse one inbound line. Exported for the specs. */
export function handleLine(line: string, handlers: InboundHandlers): void {
	if (!line.trim()) return;
	let frame: any;
	try {
		frame = JSON.parse(line);
	} catch {
		log('warn', `[stdio] dropped a line that is not JSON (${line.length} chars)`);
		return;
	}
	switch (frame?.$forge) {
		case 'in':
			handlers.message(frame.message);
			return;
		case 'return': {
			const call = pending.get(frame.id);
			if (!call) return;
			pending.delete(frame.id);
			if (typeof frame.error === 'string') call.reject(new Error(frame.error));
			else call.resolve(frame.result);
			return;
		}
		case 'visibility':
			handlers.visibility(frame.visible === true);
			return;
		default:
			log('warn', `[stdio] dropped an unknown frame: ${String(frame?.$forge)}`);
	}
}

export function listen(handlers: InboundHandlers): void {
	const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
	rl.on('line', (line) => handleLine(line, handlers));
	// The shell went away: so does the host (and with it every CLI it spawned).
	rl.on('close', () => {
		log('info', '[stdio] stdin closed; exiting');
		process.exit(0);
	});
}
