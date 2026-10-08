// Where things are. Forge's sources come from a checkout of AmineXSaid/FORGE:
// $FORGE_SRC, or ../FORGE next to this repository.
import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';

export const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const forge = path.resolve(process.env.FORGE_SRC || path.join(repo, '..', 'FORGE'));
/** The staged app: stage/forge is the host's root (the "extension path"), stage/web the window's pages. */
export const stage = path.join(repo, 'stage');
export const stageForge = path.join(stage, 'forge');
export const stageWeb = path.join(stage, 'web');
export const hostBundle = path.join(stageForge, 'dist', 'forge-host.cjs');

export function requireForge() {
	if (!fs.existsSync(path.join(forge, 'src', 'extension.ts'))) {
		console.error(`[forge-desktop] no Forge checkout at ${forge}. Clone AmineXSaid/FORGE there, or set FORGE_SRC.`);
		process.exit(1);
	}
	if (!fs.existsSync(path.join(forge, 'node_modules'))) {
		console.error(`[forge-desktop] ${forge} has no node_modules. Run \`pnpm install\` in it first.`);
		process.exit(1);
	}
	return forge;
}
