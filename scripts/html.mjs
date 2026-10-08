/**
 * The page the window boots: Forge's `desktop` page (sidebar, chat, Changes)
 * instead of the side-bar chat the host printed. The host keeps routing by
 * the chat view's id; only the page differs.
 */
export function asDesktopPage(html) {
  const marker = '"page":"chat"';
  if ((html.split(marker).length - 1) !== 1) throw new Error('unexpected HTML from Forge: expected one chat bootstrap');
  return html.replace(marker, '"page":"desktop"');
}

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
		.replace('<body>', '<body class="vscode-light forge-desktop" data-vscode-theme-kind="vscode-light">');
}

/**
 * Point the page at this build's assets. Forge's entry files keep the same
 * names on every build (main.js, style.css), and the webview caches them, so
 * an updated app could run the previous build's code. Each build's assets are
 * staged under their own folder, named from their content, instead of
 * appending a query (the lazy chunks import "./main.js" by name, and a second
 * URL for it would load the app twice).
 */
export function withAssetFolder(html, folder) {
  if (!/^assets-[0-9a-f]{8,}$/.test(folder)) throw new Error(`bad asset folder: ${folder}`);
  const count = html.split('dist/media/').length - 1;
  if (count < 2) throw new Error('unexpected HTML from Forge: expected its dist/media script and style');
  return html.split('dist/media/').join(`${folder}/`);
}
