import { test } from 'node:test';
import assert from 'node:assert/strict';
import { withHostStyles } from '../scripts/html.mjs';

test('the host stylesheet comes first in <head>, the theme class goes on <body>', () => {
	const out = withHostStyles('<!DOCTYPE html><html><head><meta charset="UTF-8" /><link href="dist/media/style.css" rel="stylesheet" /></head><body><div id="app"></div></body></html>');
	assert.ok(out.indexOf('desktop-host.css') < out.indexOf('dist/media/style.css'), "Forge's styles load after the host's, as in VS Code");
	assert.match(out, /<body class="vscode-dark"/);
});

test('HTML of an unexpected shape is refused rather than half-patched', () => {
	assert.throws(() => withHostStyles('<html><body></body></html>'));
	assert.throws(() => withHostStyles('<head></head><head></head><body></body>'));
});
