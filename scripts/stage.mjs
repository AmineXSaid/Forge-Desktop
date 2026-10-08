// Assemble the app's files from a built Forge checkout and the host bundle:
//
//   stage/forge/   the host's root, laid out like the VSIX:
//                  package.json, dist/forge-host.cjs, dist/7z-wasm/, dist/media/,
//                  resources/ (Claude Code binary, ripgrep, endpoint rules, ...)
//   stage/web/     the window's pages (Tauri's frontendDist):
//                  index.html (Forge's own chat-view HTML, from --print-html),
//                  desktop-host.css, dist/media/, resources/forge-welcome-*.png
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { forge, hostBundle, repo, requireForge, stageForge, stageWeb } from './paths.mjs';
import { createHash } from 'node:crypto';
import { asDesktopPage, withAssetFolder, withHostStyles } from './html.mjs';

requireForge();
const need = (file, hint) => {
	if (!fs.existsSync(file)) {
		console.error(`[forge-desktop] missing ${file}. ${hint}`);
		process.exit(1);
	}
};
need(path.join(forge, 'dist', 'media', 'main.js'), 'Run `pnpm run forge:build`.');
need(hostBundle, 'Run `pnpm run build:host`.');

const copy = (from, to) => {
	fs.rmSync(to, { recursive: true, force: true });
	fs.mkdirSync(path.dirname(to), { recursive: true });
	fs.cpSync(from, to, { recursive: true, dereference: true });
};

// --- stage/forge ------------------------------------------------------------
fs.copyFileSync(path.join(forge, 'package.json'), path.join(stageForge, 'package.json'));
copy(path.join(forge, 'dist', 'media'), path.join(stageForge, 'dist', 'media'));
if (fs.existsSync(path.join(forge, 'dist', '7z-wasm'))) {
	copy(path.join(forge, 'dist', '7z-wasm'), path.join(stageForge, 'dist', '7z-wasm'));
}
copy(path.join(forge, 'resources'), path.join(stageForge, 'resources'));
const binary = ['native-binary', 'native-binaries'].some((d) => fs.existsSync(path.join(stageForge, 'resources', d)));
if (!binary) console.warn('[forge-desktop] no Claude Code binary in resources/: the app will not be able to chat. Run `pnpm run forge:build`.');

// --- stage/web --------------------------------------------------------------
fs.rmSync(stageWeb, { recursive: true, force: true });
const media = path.join(forge, 'dist', 'media');
const hash = createHash('sha256');
for (const name of fs.readdirSync(media, { recursive: true }).map(String).sort()) {
	const file = path.join(media, name);
	if (fs.statSync(file).isFile()) hash.update(name).update(fs.readFileSync(file));
}
const assetFolder = `assets-${hash.digest('hex').slice(0, 12)}`;
copy(media, path.join(stageWeb, assetFolder));
for (const art of ['forge-welcome-light.png', 'forge-welcome-dark.png']) {
	fs.mkdirSync(path.join(stageWeb, 'resources'), { recursive: true });
	fs.copyFileSync(path.join(forge, 'resources', art), path.join(stageWeb, 'resources', art));
}
fs.copyFileSync(path.join(repo, 'web', 'desktop-host.css'), path.join(stageWeb, 'desktop-host.css'));

// Forge's own HTML for the chat view: the host prints it with a throwaway data
// directory (no doctor run, no state written anywhere real).
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-desktop-html-'));
fs.writeFileSync(path.join(scratch, 'settings.json'), JSON.stringify({ 'forge.runDoctorOnStartup': false }));
const raw = path.join(scratch, 'index.html');
execFileSync(process.execPath, [hostBundle, '--print-html', raw], {
	env: { ...process.env, FORGE_RESOURCES: stageForge, FORGE_DATA_DIR: scratch, FORGE_WORKSPACE: '' },
	stdio: ['ignore', 'ignore', 'inherit'],
	timeout: 60_000,
});
fs.writeFileSync(path.join(stageWeb, 'index.html'), withAssetFolder(withHostStyles(asDesktopPage(fs.readFileSync(raw, 'utf8'))), assetFolder));
fs.rmSync(scratch, { recursive: true, force: true });
// The window opens index.html?build=<id> (src-tauri/src/lib.rs reads this file
// at compile time), so a new build never gets the webview's cached page.
fs.writeFileSync(path.join(stageWeb, 'build-id'), assetFolder.slice('assets-'.length));

console.log(`[forge-desktop] staged ${path.relative(repo, stageForge)} and ${path.relative(repo, stageWeb)}`);
