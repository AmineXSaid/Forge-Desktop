// The built sidecar, loaded as a module (main() runs only as a program).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import { hostBundle } from '../scripts/paths.mjs';

const require = createRequire(import.meta.url);
const skip = fs.existsSync(hostBundle) ? false : 'run `pnpm run build:host` first';

test('frames: page messages, host-call answers and junk are told apart', { skip }, () => {
	const host = require(hostBundle);
	const got = [];
	const handlers = { message: (m) => got.push(['message', m]), visibility: (v) => got.push(['visibility', v]) };
	host.handleLine('{"$forge":"in","message":{"type":"request","requestId":"r1"}}', handlers);
	host.handleLine('{"$forge":"visibility","visible":false}', handlers);
	host.handleLine('not json', handlers);
	host.handleLine('{"$forge":"exec","command":"x"}', handlers);
	host.handleLine('', handlers);
	assert.deepEqual(got, [['message', { type: 'request', requestId: 'r1' }], ['visibility', false]]);
});

test('the webview maps files under the root to page-relative paths, and nothing outside it', { skip }, () => {
	const host = require(hostBundle);
	const path = require('node:path');
	const root = path.resolve('/app/forge');
	const posted = [];
	const view = host.createDesktopWebview(root, (p) => posted.push(p));
	const fileUri = (p) => ({ fsPath: p });
	assert.equal(String(view.webview.asWebviewUri(fileUri(path.join(root, 'dist', 'media', 'main.js')))), 'dist/media/main.js');
	assert.throws(() => view.webview.asWebviewUri(fileUri(path.resolve('/etc/passwd'))));
	void view.webview.postMessage({ type: 'from-extension', message: { type: 'response' } });
	assert.equal(posted.length, 1);
	const seen = [];
	view.webview.onDidReceiveMessage((m) => seen.push(m));
	view.receive({ type: 'request' });
	assert.deepEqual(seen, [{ type: 'request' }]);
});
