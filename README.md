# Forge Desktop

A Windows desktop app for Claude Code, written in Rust. Each chat runs the
Claude Code CLI in your project folder; everything around it (sessions,
providers, settings, the Changes view, the window) is the app's own.

- **Plan and architecture:** [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- **Design system:** [`docs/DESIGN.md`](docs/DESIGN.md)
- **Working rules:** [`CLAUDE.md`](CLAUDE.md)

## Build and run (Windows)

You need:
- Git and Node.js 22+ with pnpm (`corepack enable`), for the Tauri CLI;
- Rust with the MSVC toolchain (`winget install Rustlang.Rustup`) and the
  Visual Studio C++ Build Tools (`winget install Microsoft.VisualStudio.2022.BuildTools`
  with the "Desktop development with C++" workload);
- WebView2 (Windows 11 has it);
- Claude Code itself, to chat: `irm https://claude.ai/install.ps1 | iex`, then
  `claude` once to sign in. The app tells you if it can't find it.

```powershell
git clone https://github.com/AmineXSaid/forge-desktop
cd forge-desktop
pnpm install
pnpm dev      # run it (Vite serves the UI, Tauri opens the window)
pnpm build    # installer: target\release\bundle\nsis\
```

`pnpm dev` keeps the terminal attached; close the window to stop. The
installer is what you double-click; the development build
under `target\debug` needs `pnpm dev` running.

## Where things are

| What | Where |
| --- | --- |
| Settings, recent projects | `%APPDATA%\com.msaid.forge\` |
| Endpoint keys | Windows Credential Manager (`com.msaid.forge`) |
| Logs (app and CLI stderr) | `%LOCALAPPDATA%\com.msaid.forge\logs\forge-desktop.log` |
| Conversations | `%USERPROFILE%\.claude\projects\`, shared with the CLI |

## Tests

```bash
pnpm test        # UI unit tests (vitest), then cargo test for the workspace
pnpm ui:check    # Svelte and TypeScript checks
pnpm ui:dev      # the UI alone in a browser, against a demo backend (ui/src/lib/mock.ts)
```

`cli_e2e` runs the real `claude` CLI against `test/stub-anthropic.mjs` (no
credentials, no network): a Write is prompted, allowed, lands on disk, the
conversation is saved, and a second process resumes it. It is skipped when
`claude` or `node` is missing.
