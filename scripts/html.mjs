/**
 * What VS Code adds around a webview's HTML, applied to the page Forge prints:
 * its stylesheet first in <head> (web/desktop-host.css: VS Code's defaults and
 * the theme variables), and the theme class on <body>, where Forge's tokens
 * read it (forge-tokens.css).
 */
export function withHostStyles(html) {
	if ((html.match(/<head>/g) ?? []).length !== 1 || (html.match(/<body>/g) ?? []).length !== 1) {
		throw new Error('unexpected HTML from Forge: expected one <head> and one <body>');
	}
	return html
		.replace('<head>', '<head>\n    <link rel="stylesheet" href="desktop-host.css" />')
		.replace('<body>', '<body class="vscode-dark" data-vscode-theme-kind="vscode-dark">');
}
