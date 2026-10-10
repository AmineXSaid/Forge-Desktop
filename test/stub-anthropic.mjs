// A stand-in for the Anthropic Messages API, for tests: no credentials, no
// network. Each conversation is scripted by its last user turn:
//
// - a user prompt containing "write hello" answers with a Write tool call for
//   hello.txt in the prompt's folder (the test names it);
// - a user prompt "fix the greeting in <file>" answers with an Edit of that
//   file ("Hello, world" to "Hello, Forge");
// - a tool result answers "Done." (after an Edit: a short markdown summary);
// - anything else answers "Hello from the stub."
//
// `node test/stub-anthropic.mjs` prints `listening <port>` and serves until killed.
import http from 'node:http';

const sse = (res, events) => {
	res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
	for (const [event, data] of events) res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
	res.end();
};

let ids = 0;
const message = (model) => ({
	id: `msg_stub_${++ids}`,
	type: 'message',
	role: 'assistant',
	model,
	content: [],
	stop_reason: null,
	stop_sequence: null,
	usage: { input_tokens: 10, output_tokens: 0 },
});

function textEvents(model, text) {
	return [
		['message_start', { type: 'message_start', message: message(model) }],
		['content_block_start', { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } }],
		['content_block_delta', { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text } }],
		['content_block_stop', { type: 'content_block_stop', index: 0 }],
		['message_delta', { type: 'message_delta', delta: { stop_reason: 'end_turn', stop_sequence: null }, usage: { output_tokens: 5 } }],
		['message_stop', { type: 'message_stop' }],
	];
}

function toolEvents(model, name, input, say = 'Writing the file.') {
	return [
		['message_start', { type: 'message_start', message: message(model) }],
		['content_block_start', { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } }],
		['content_block_delta', { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: say } }],
		['content_block_stop', { type: 'content_block_stop', index: 0 }],
		['content_block_start', { type: 'content_block_start', index: 1, content_block: { type: 'tool_use', id: `toolu_stub_${ids}`, name, input: {} } }],
		['content_block_delta', { type: 'content_block_delta', index: 1, delta: { type: 'input_json_delta', partial_json: JSON.stringify(input) } }],
		['content_block_stop', { type: 'content_block_stop', index: 1 }],
		['message_delta', { type: 'message_delta', delta: { stop_reason: 'tool_use', stop_sequence: null }, usage: { output_tokens: 20 } }],
		['message_stop', { type: 'message_stop' }],
	];
}

const textOf = (content) =>
	typeof content === 'string' ? content : (content ?? []).filter((b) => b.type === 'text').map((b) => b.text).join('\n');

const EDIT_SUMMARY = [
	'Updated the greeting in `src/main.rs`.',
	'',
	'**What changed**',
	'- The banner now reads *Hello, Forge* instead of the placeholder.',
	'- Nothing else in the file was touched.',
	'',
	'You can check it with:',
	'',
	'```sh',
	'cargo run',
	'```',
].join('\n');

function reply(body) {
	const model = body.model ?? 'stub-model';
	const last = body.messages?.[body.messages.length - 1];
	const blocks = Array.isArray(last?.content) ? last.content : [];
	if (blocks.some((b) => b.type === 'tool_result')) {
		const asked = body.messages?.[body.messages.length - 2];
		const used = (Array.isArray(asked?.content) ? asked.content : []).find((b) => b.type === 'tool_use');
		if (used?.name === 'Edit') return textEvents(model, EDIT_SUMMARY);
		return textEvents(model, 'Done.');
	}
	const said = textOf(last?.content);
	const fix = said.match(/fix the greeting in (\S+)/)?.[1];
	if (fix) return toolEvents(model, 'Edit', { file_path: fix, old_string: 'Hello, world', new_string: 'Hello, Forge' }, "I'll update the greeting so the app introduces itself by name.");
	const folder = said.match(/write hello in (\S+)/)?.[1];
	if (folder) return toolEvents(model, 'Write', { file_path: `${folder}/hello.txt`, content: 'hello\n' });
	return textEvents(model, 'Hello from the stub.');
}

const server = http.createServer((req, res) => {
	let raw = '';
	req.on('data', (d) => (raw += d));
	req.on('end', () => {
		const url = req.url ?? '';
		if (req.method === 'POST' && url.startsWith('/v1/messages/count_tokens')) {
			res.writeHead(200, { 'content-type': 'application/json' });
			return res.end(JSON.stringify({ input_tokens: 10 }));
		}
		if (req.method === 'POST' && url.startsWith('/v1/messages')) {
			let body = {};
			try {
				body = JSON.parse(raw);
			} catch {}
			const events = reply(body);
			if (body.stream) return sse(res, events);
			// Not streamed (title generation and the like): the final message.
			const final = { ...message(body.model), content: [{ type: 'text', text: 'Stub title' }], stop_reason: 'end_turn' };
			res.writeHead(200, { 'content-type': 'application/json' });
			return res.end(JSON.stringify(final));
		}
		res.writeHead(404, { 'content-type': 'application/json' });
		res.end(JSON.stringify({ type: 'error', error: { type: 'not_found_error', message: url } }));
	});
});

server.listen(Number(process.env.PORT ?? 0), '127.0.0.1', () => {
	console.log(`listening ${server.address().port}`);
});
