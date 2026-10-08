import { test } from 'node:test';
import assert from 'node:assert/strict';
import { asDesktopPage, withAssetFolder, withHostStyles } from '../scripts/html.mjs';

test('the host stylesheet comes first in <head>, the theme class goes on <body>', () => {
	const out = withHostStyles('<!DOCTYPE html><html><head><meta charset="UTF-8" /><link href="dist/media/style.css" rel="stylesheet" /></head><body><div id="app"></div></body></html>');
	assert.ok(out.indexOf('desktop-host.css') < out.indexOf('dist/media/style.css'), "Forge's styles load after the host's, as in VS Code");
	assert.match(out, /<body class="vscode-light forge-desktop"/);
});

test('HTML of an unexpected shape is refused rather than half-patched', () => {
	assert.throws(() => withHostStyles('<html><body></body></html>'));
	assert.throws(() => withHostStyles('<head></head><head></head><body></body>'));
});

test('the window boots the desktop page', () => {
	const html = '<script>window.FORGE_BOOTSTRAP = {"host":"sidebar","page":"chat","id":"forge.chatView"};</script>';
	assert.match(asDesktopPage(html), /"page":"desktop"/);
	assert.throws(() => asDesktopPage('<script>window.FORGE_BOOTSTRAP = {};</script>'));
});

test("each build's assets get their own folder", () => {
	const html = '<link href="dist/media/style.css"><script src="dist/media/main.js"></script>';
	assert.equal(withAssetFolder(html, 'assets-0123456789ab'), '<link href="assets-0123456789ab/style.css"><script src="assets-0123456789ab/main.js"></script>');
	assert.throws(() => withAssetFolder(html, '../x'));
	assert.throws(() => withAssetFolder('<html></html>', 'assets-0123456789ab'));
});
