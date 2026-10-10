// A stand-in for an OpenAI-compatible chat completions API, for the gateway's
// tests: no credentials, no network. It checks what the gateway must do:
//
// - the key is the one Forge holds (STUB_KEY), never the CLI's token;
// - the model arrives without Forge's `provider/` prefix (gpt-stub*);
// and answers by what the person last said (the CLI adds its own context
// after it, and after tool results):
// - a tool result: "Done.";
// - "write hello in <dir>": thinking, a sentence, then a Write call whose
//   arguments arrive in two pieces;
// - "think about it": reasoning, then an answer;
// - "rate limit once": 429 the first time, then an answer;
// - a session-title request (response_format json_schema): {"title": ...};
// - anything else: "Hello from the OpenAI stub."
//
// `node test/stub-openai.mjs` prints `listening <port>` and serves until killed.
import http from 'node:http';

const KEY = process.env.STUB_KEY ?? 'sk-openai-stub';
const limited = new Set();
let n = 0;

const chunk = (delta, finish = null) => ({ id: `chatcmpl-${n}`, object: 'chat.completion.chunk', choices: [{ index: 0, delta, finish_reason: finish }] });
const usage = { id: `chatcmpl-${n}`, object: 'chat.completion.chunk', choices: [], usage: { prompt_tokens: 120, completion_tokens: 12, prompt_tokens_details: { cached_tokens: 20 } } };

function script(body) {
	const msgs = body.messages ?? [];
	// The CLI adds its own context as further user messages: read every user
	// message since the last reply.
	const from = msgs.findLastIndex((m) => m.role !== 'user') + 1;
	const textOf = (c) => (typeof c === 'string' ? c : (c ?? []).map((p) => p.text ?? '').join('\n'));
	const text = msgs.slice(from).filter((m) => m.role === 'user').map((m) => textOf(m.content)).join('\n');
	if (body.response_format?.json_schema?.schema?.properties?.title) {
		const first = (text.match(/<session>\n?([^\n]*)/)?.[1] ?? text).trim().slice(0, 40);
		return [chunk({ role: 'assistant', content: JSON.stringify({ title: `OpenAI: ${first}` }) }), chunk({}, 'stop')];
	}
	if (msgs.slice(msgs.findLastIndex((m) => m.role === 'assistant') + 1).some((m) => m.role === 'tool')) {
		return [chunk({ role: 'assistant', content: 'Done.' }), chunk({}, 'stop')];
	}
	const dir = text.match(/write hello in (\S+)/)?.[1];
	if (dir) {
		const args = JSON.stringify({ file_path: `${dir}/hello.txt`, content: 'hello\n' });
		return [
			chunk({ role: 'assistant', reasoning_content: 'The user wants a file.' }),
			chunk({ content: 'Writing the file.' }),
			chunk({ tool_calls: [{ index: 0, id: `call_${n}`, type: 'function', function: { name: 'Write', arguments: '' } }] }),
			chunk({ tool_calls: [{ index: 0, function: { arguments: args.slice(0, 10) } }] }),
			chunk({ tool_calls: [{ index: 0, function: { arguments: args.slice(10) } }] }),
			chunk({}, 'tool_calls'),
		];
	}
	if (/think about it/.test(text)) {
		return [chunk({ role: 'assistant', reasoning_content: 'Weighing ' }), chunk({ reasoning_content: 'the options.' }), chunk({ content: 'Thought it through.' }), chunk({}, 'stop')];
	}
	return [chunk({ role: 'assistant', content: 'Hello from the OpenAI stub.' }), chunk({}, 'stop')];
}

const json = (res, status, body, headers = {}) => {
	res.writeHead(status, { 'content-type': 'application/json', ...headers });
	res.end(JSON.stringify(body));
};

const server = http.createServer((req, res) => {
	let raw = '';
	req.on('data', (d) => (raw += d));
	req.on('end', () => {
		// STUB_OPEN: no key needed, like a local server.
		if (!process.env.STUB_OPEN && req.headers.authorization !== `Bearer ${KEY}`) return json(res, 401, { error: { message: 'Incorrect API key provided', type: 'invalid_request_error' } });
		if (req.method === 'GET' && req.url === '/v1/models') {
			return json(res, 200, { object: 'list', data: [{ id: 'gpt-stub', object: 'model' }, { id: 'gpt-stub-fast', object: 'model' }] });
		}
		if (req.method !== 'POST' || req.url !== '/v1/chat/completions') return json(res, 404, { error: { message: `no ${req.url}` } });
		let body = {};
		try { body = JSON.parse(raw); } catch {}
		n++;
		if (process.env.STUB_LOG) console.error("REQ " + JSON.stringify({ model: body.model, messages: (body.messages ?? []).filter((m) => m.role !== "system").map((m) => ({ role: m.role, content: JSON.stringify(m.content).slice(0, 300) })) }));
		if (!/^gpt-stub/.test(body.model ?? '')) return json(res, 400, { error: { message: `unknown model ${body.model}` } });
		const said = JSON.stringify(body.messages?.slice(body.messages.findLastIndex((m) => m.role !== 'user') + 1) ?? []);
		if (said.includes('rate limit once') && !limited.has('rate')) {
			limited.add('rate');
			return json(res, 429, { error: { message: 'Rate limit reached', type: 'rate_limit' } }, { 'retry-after': '0' });
		}
		const chunks = script(body);
		if (!body.stream) {
			const content = chunks.map((c) => c.choices[0]?.delta?.content ?? '').join('');
			return json(res, 200, { id: `chatcmpl-${n}`, choices: [{ index: 0, message: { role: 'assistant', content }, finish_reason: 'stop' }], usage: usage.usage });
		}
		res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
		for (const c of [...chunks, usage]) res.write(`data: ${JSON.stringify(c)}\n\n`);
		res.end('data: [DONE]\n\n');
	});
});

server.listen(Number(process.env.PORT ?? 0), '127.0.0.1', () => console.log(`listening ${server.address().port}`));
