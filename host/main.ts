/**
 * Forge Desktop's Node sidecar: Forge's own extension host, outside VS Code.
 *
 * It replays VS Code's side of activation:
 * 1. call Forge's real `activate()` (src/extension.ts) with a desktop
 *    ExtensionContext; `vscode` is the shim (vscodeShim.ts);
 * 2. resolve the chat view Forge registered, with a webview whose
 *    `postMessage` writes a frame to stdout and whose `onDidReceiveMessage`
 *    is fed from stdin.
 *
 * So the dispatcher, the handlers, the routing by webview id and the HTML are
 * Forge's own; the Rust shell only carries lines. `--print-html <file>` writes
 * the page that webview would get and exits (the build stages it as the
 * window's index.html).
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { env, handleLine, listen, log, redirectConsole, sendFrame } from './runtime';
import {
	CancellationTokenSource,
	EventEmitter,
	ExtensionMode,
	Uri,
	createExtensionContext,
	registeredViewProviders,
} from './vscodeShim';
import { activate, deactivate } from 'forge/extension';

/** The view id the chat page boots under (Forge's primary side-bar chat). */
export const CHAT_VIEW_ID = 'forge.chatView';

/**
 * What `${webview.cspSource}` stands for in Forge's CSP: the app's own files,
 * and Tauri's IPC endpoints (`ipc://localhost` on Linux and macOS,
 * `http://ipc.localhost` on Windows).
 */
const CSP_SOURCE = "'self' ipc: http://ipc.localhost";

export interface DesktopWebview {
	webview: any;
	view: any;
	/** A webview message arrived (from stdin). */
	receive(message: unknown): void;
	setVisible(visible: boolean): void;
	readonly html: string;
}

/**
 * A `vscode.WebviewView` for the window. `asWebviewUri` maps a file under the
 * Forge root to a path relative to the page, which is where the build stages it.
 */
export function createDesktopWebview(root: string, post: (payload: unknown) => void): DesktopWebview {
	const received = new EventEmitter<unknown>();
	const visibility = new EventEmitter<void>();
	const disposed = new EventEmitter<void>();
	let html = '';
	let visible = true;

	const webview = {
		options: {},
		cspSource: CSP_SOURCE,
		get html() {
			return html;
		},
		set html(value: string) {
			html = value;
		},
		asWebviewUri(uri: Uri): Uri {
			const rel = path.relative(root, uri.fsPath).split(path.sep).join('/');
			if (rel.startsWith('..')) throw new Error(`Forge Desktop: ${uri.fsPath} is outside the app's files`);
			// toString() is what the HTML template interpolates.
			return Object.assign(Uri.from({ scheme: 'forge-relative', path: rel }), { toString: () => rel });
		},
		async postMessage(payload: unknown): Promise<boolean> {
			post(payload);
			return true;
		},
		onDidReceiveMessage: received.event,
	};

	const view = {
		viewType: CHAT_VIEW_ID,
		webview,
		title: 'Forge',
		get visible() {
			return visible;
		},
		onDidChangeVisibility: visibility.event,
		onDidDispose: disposed.event,
		show() {},
	};

	return {
		webview,
		view,
		receive: (message) => received.fire(message),
		setVisible(next) {
			if (next === visible) return;
			visible = next;
			visibility.fire();
		},
		get html() {
			return html;
		},
	};
}

/** Activate Forge and resolve its chat view on `webview`. */
export function start(webview: DesktopWebview, mode = ExtensionMode.Production) {
	const context = createExtensionContext(mode);
	const api = activate(context as any);
	const provider = registeredViewProviders.get(CHAT_VIEW_ID);
	if (!provider) throw new Error(`Forge registered no provider for ${CHAT_VIEW_ID}`);
	provider.resolveWebviewView(webview.view, { state: undefined }, new CancellationTokenSource().token);
	return { context, api };
}

function printHtml(file: string): void {
	const webview = createDesktopWebview(env.root, () => {});
	start(webview);
	if (!webview.html) throw new Error('Forge produced no HTML for the chat view');
	fs.mkdirSync(path.dirname(file), { recursive: true });
	fs.writeFileSync(file, webview.html);
	void deactivate().finally(() => process.exit(0));
}

function main(): void {
	redirectConsole();
	process.on('uncaughtException', (error) => log('error', '[host] uncaught exception', error));
	process.on('unhandledRejection', (reason) => log('error', '[host] unhandled rejection', reason));

	const htmlIndex = process.argv.indexOf('--print-html');
	if (htmlIndex !== -1) {
		printHtml(path.resolve(process.argv[htmlIndex + 1] ?? 'index.html'));
		return;
	}

	log('info', `[host] Forge Desktop ${env.appVersion}; workspace ${env.workspace ?? '(none)'}; root ${env.root}`);
	if (env.workspace) {
		try {
			process.chdir(env.workspace);
		} catch (error) {
			log('warn', `[host] could not enter ${env.workspace}`, error);
		}
	}

	const webview = createDesktopWebview(env.root, (payload) => sendFrame({ $forge: 'out', message: payload }));
	listen({
		message: (message) => webview.receive(message),
		visibility: (visible) => webview.setVisible(visible),
	});
	start(webview);
	sendFrame({ $forge: 'ready' });
	log('info', '[host] ready');
}

// Exported for the specs, which drive the same code without a shell.
export { handleLine };

if (require.main === module) main();
