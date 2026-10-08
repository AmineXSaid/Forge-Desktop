// Build the parts of Forge the app ships: the webview (dist/media) and, as a
// side effect of Forge's own extension build, the Claude Code binary for this
// machine (resources/native-binary/) and 7-Zip (dist/7z-wasm/).
// Windows: run this on Windows, where pnpm installs claude.exe.
import { execSync } from 'node:child_process';
import { requireForge } from './paths.mjs';

const forge = requireForge();
const run = (cmd) => {
	console.log(`[forge-desktop] ${cmd}  (in ${forge})`);
	execSync(cmd, { cwd: forge, stdio: 'inherit', shell: true });
};
run('pnpm run build:webview');
run('pnpm run build:extension');
