// Bundle the sidecar: host/main.ts plus Forge's host sources into one CommonJS
// file, stage/forge/dist/forge-host.cjs (where dist/extension.cjs sits in the
// VSIX, so Forge's __dirname-relative lookups resolve the same way).
//
// `vscode` resolves to host/vscodeShim.ts, and `forge/*` to <FORGE>/src/*.
// A Forge reference to a vscode member the shim does not export is an error:
// that is the shim's coverage gate.
import esbuild from 'esbuild';
import path from 'node:path';
import fs from 'node:fs';
import { forge, hostBundle, repo, requireForge } from './paths.mjs';

requireForge();
const production = process.argv.includes('--production');
const shim = path.join(repo, 'host', 'vscodeShim.ts');

const aliases = {
	name: 'forge-desktop-aliases',
	setup(build) {
		build.onResolve({ filter: /^vscode$/ }, () => ({ path: shim }));
		build.onResolve({ filter: /^forge\// }, async (args) => {
			const result = await build.resolve('./' + args.path.slice('forge/'.length), {
				resolveDir: path.join(forge, 'src'),
				kind: args.kind,
			});
			return result.errors.length ? { errors: result.errors } : { path: result.path };
		});
	},
};

const result = await esbuild.build({
	entryPoints: [path.join(repo, 'host', 'main.ts')],
	bundle: true,
	format: 'cjs',
	platform: 'node',
	target: 'node22',
	outfile: hostBundle,
	minify: production,
	sourcemap: !production,
	sourcesContent: false,
	legalComments: production ? 'none' : 'eof',
	// The Agent SDK is ESM and calls createRequire(import.meta.url) at module
	// scope; Forge's esbuild.ts explains why this define is required in CJS.
	banner: { js: 'const __forgeImportMetaUrl = require("url").pathToFileURL(__filename).href;' },
	define: { 'import.meta.url': '__forgeImportMetaUrl' },
	plugins: [aliases],
	logLevel: 'silent',
	logOverride: { 'import-is-undefined': 'error' },
}).catch((error) => {
	for (const e of error.errors ?? [error]) {
		const where = e.location ? `${e.location.file}:${e.location.line}:${e.location.column}: ` : '';
		console.error(`✘ ${where}${e.text ?? e.message}`);
	}
	process.exit(1);
});

for (const w of result.warnings) {
	const where = w.location ? `${w.location.file}:${w.location.line}: ` : '';
	console.warn(`▲ ${where}${w.text}`);
}
const size = (fs.statSync(hostBundle).size / 1024 / 1024).toFixed(1);
console.log(`[forge-desktop] host -> ${path.relative(repo, hostBundle)} (${size} MB)`);
