// End to end, without the window: the sidecar (Forge's real host), the real
// Claude Code CLI, and Forge's stub OpenAI-compatible gateway through Forge's
// endpoint relay. The test plays the webview: it sends the messages the chat
// page sends and answers the permission prompt the way the page does.
//
// This is plan §4.5 M1 "done when" 2-4 at the protocol level: a Write is
// prompted, allowed, the file lands on disk, the conversation is saved under
// ~/.claude/projects, and a second launch can resume it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { forge, hostBundle, stageForge } from '../scripts/paths.mjs';

const GATEWAY = path.join(forge, '.claude', 'skills', 'ui-parity', 'e2e', 'stub-gateway.mjs');
const hasBinary = ['native-binary', 'native-binaries'].some((d) => fs.existsSync(path.join(stageForge, 'resources', d)));
const skip = !fs.existsSync(hostBundle) || !fs.existsSync(GATEWAY) || !hasBinary
	? 'needs a staged app (pnpm run prepare:app) and Forge\'s stub gateway'
	: false;

const freePort = () =>
	new Promise((resolve) => {
		const s = net.createServer();
		s.listen(0, '127.0.0.1', () => {
			const { port } = s.address();
			s.close(() => resolve(port));
		});
	});

const waitFor = async (what, check, ms = 60_000) => {
	const end = Date.now() + ms;
	while (Date.now() < end) {
		const value = await check();
		if (value) return value;
		await new Promise((r) => setTimeout(r, 100));
	}
	throw new Error(`timed out waiting for ${what}`);
};

/**
 * This machine's environment without Claude Code's own variables: a test run
 * from inside a Claude Code session would otherwise hand its credentials and
 * provider settings to the CLI under test.
 */
const cleanEnv = () => Object.fromEntries(Object.entries(process.env).filter(([k]) => !/^(ANTHROPIC|CLAUDE)/.test(k)));

/** One sidecar, played by the test as the webview. */
function startHost({ home, workspace, data }) {
	const child = spawn(process.execPath, [hostBundle], {
		env: {
			...cleanEnv(),
			HOME: home,
			USERPROFILE: home,
			FORGE_WORKSPACE: workspace,
			FORGE_DATA_DIR: data,
			FORGE_RESOURCES: stageForge,
			FORGE_APP_VERSION: 'test',
		},
		stdio: ['pipe', 'pipe', 'pipe'],
	});
	const frames = [];
	const nonFrames = [];
	let buf = '';
	child.stdout.on('data', (d) => {
		buf += d;
		let i;
		while ((i = buf.indexOf('\n')) >= 0) {
			const line = buf.slice(0, i);
			buf = buf.slice(i + 1);
			try {
				frames.push(JSON.parse(line));
			} catch {
				nonFrames.push(line);
			}
		}
	});
	let stderr = '';
	child.stderr.on('data', (d) => (stderr += d));
	let seq = 0;
	const send = (message) => child.stdin.write(JSON.stringify({ $forge: 'in', message }) + '\n');
	/** What the webview receives: the `message` inside each from-extension wrapper. */
	const inbox = () => frames.filter((f) => f.$forge === 'out' && f.message?.type === 'from-extension').map((f) => f.message.message);
	const request = async (request) => {
		const requestId = `t${++seq}`;
		send({ type: 'request', requestId, request });
		return waitFor(`response to ${request.type}`, () => inbox().find((m) => m.type === 'response' && m.requestId === requestId));
	};
	return { child, frames, nonFrames, send, inbox, request, stderr: () => stderr };
}

const userMessage = (text) => ({
	type: 'user',
	session_id: '',
	parent_tool_use_id: null,
	message: { role: 'user', content: [{ type: 'text', text }] },
});

test('a chat in a chosen folder: prompt, allow, file on disk, saved and resumable', { skip, timeout: 240_000 }, async (t) => {
	const root = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-desktop-e2e-'));
	const home = path.join(root, 'home');
	const workspace = path.join(root, 'project');
	const data = path.join(root, 'data');
	for (const d of [home, workspace, data]) fs.mkdirSync(d, { recursive: true });

	const port = await freePort();
	const gateway = spawn(process.execPath, [GATEWAY, '--port', String(port)], { stdio: 'ignore' });
	t.after(() => gateway.kill());
	await waitFor('the stub gateway', () => fetch(`http://127.0.0.1:${port}/v1/models`).then((r) => r.ok, () => false));

	fs.writeFileSync(path.join(data, 'settings.json'), JSON.stringify({
		'forge.runDoctorOnStartup': false,
		'forge.endpointHealth.syncIntervalMinutes': 0,
		'forge.endpoints': {
			e2e: {
				wire: 'openai',
				baseUrl: `http://127.0.0.1:${port}/v1`,
				model: 'qwen3-coder',
				auth: { kind: 'none' },
				capabilities: { contextWindow: 131072, tools: true },
			},
		},
		'forge.endpointProfile': 'e2e',
	}));

	const host = startHost({ home, workspace, data });
	t.after(() => host.child.kill());
	await waitFor('ready', () => host.frames.some((f) => f.$forge === 'ready'));

	const init = await host.request({ type: 'init' });
	assert.equal(init.response.state.defaultCwd, workspace, 'the CLI runs in the chosen folder');

	// The page's first message in a new conversation.
	const target = path.join(workspace, 'hello.txt');
	host.send({ type: 'launch_claude', channelId: 'c1', cwd: workspace, permissionMode: 'default' });
	host.send({ type: 'io_message', channelId: 'c1', message: userMessage(`write ${target} :: hi`), done: false });

	// Manual mode: the Write is asked about, and the test allows it as the page would.
	const prompt = await waitFor('the permission prompt', () =>
		host.inbox().find((m) => m.type === 'request' && m.request?.type === 'tool_permission_request'), 120_000);
	assert.equal(prompt.request.toolName, 'Write');
	assert.ok(!fs.existsSync(target), 'nothing is written before the answer');
	host.send({
		type: 'response',
		requestId: prompt.requestId,
		response: { type: 'tool_permission_response', result: { behavior: 'allow', updatedInput: prompt.request.inputs } },
	});

	await waitFor('hello.txt', () => fs.existsSync(target) && fs.readFileSync(target, 'utf8') === 'hi', 60_000);
	const result = await waitFor('the turn to end', () =>
		host.inbox().find((m) => m.type === 'io_message' && m.channelId === 'c1' && m.message?.type === 'result'), 60_000);
	assert.equal(result.message.is_error, false);

	// Saved where the CLI saves it, so History lists it after a restart.
	const projects = path.join(home, '.claude', 'projects');
	const transcripts = await waitFor('a transcript', () => {
		if (!fs.existsSync(projects)) return undefined;
		const found = fs.readdirSync(projects).flatMap((d) =>
			fs.readdirSync(path.join(projects, d)).filter((f) => f.endsWith('.jsonl')).map((f) => path.join(projects, d, f)));
		return found.length ? found : undefined;
	});
	const sessionId = path.basename(transcripts[0], '.jsonl');
	assert.equal(result.message.session_id, sessionId);

	// The stdout channel carried frames only.
	assert.deepEqual(host.nonFrames, [], 'every stdout line is a frame');

	// Restart: a new host lists the session and resumes it.
	host.child.kill();
	const again = startHost({ home, workspace, data });
	t.after(() => again.child.kill());
	await waitFor('ready (second launch)', () => again.frames.some((f) => f.$forge === 'ready'));
	const list = await again.request({ type: 'list_sessions_request' });
	const ids = JSON.stringify(list.response);
	assert.ok(ids.includes(sessionId), 'History lists the saved session');

	again.send({ type: 'launch_claude', channelId: 'c2', cwd: workspace, resume: sessionId, permissionMode: 'default' });
	again.send({ type: 'io_message', channelId: 'c2', message: userMessage('are you there'), done: false });
	const resumed = await waitFor('the resumed turn', () =>
		again.inbox().find((m) => m.type === 'io_message' && m.channelId === 'c2' && m.message?.type === 'result'), 120_000);
	assert.equal(resumed.message.session_id, sessionId, 'the resumed turn continues the same conversation');

	fs.rmSync(root, { recursive: true, force: true });
});
