# Forge Desktop — architecture and plan

## Context

Forge Desktop was a Tauri wrapper that ran FORGE's VS Code extension code (a Node sidecar plus a `vscode` shim) in a window. It was slow to start, looked like VS Code, hung on resize, and broke wherever the shim fell short (quick picks, setup prompts). The goal is a **standalone Windows desktop app in Rust**: a new UI and a new system design, no dependency on FORGE, built on `docs/DESIGN.md` (warm paper canvas, white hairline cards, one blue action) with **DM Sans** as the main face. It must be **premium, effective, elegant, performant and fast**. It must also reach models the way **Ante and ForgeCode** do: a catalog of providers where you sign in or paste a key, the key is checked, and you pick a model. That covers cloud APIs, local servers and custom endpoints, not only Claude.

Decisions:
- light theme plus a dark theme derived from the same tokens;
- detect the Claude Code CLI and guide its install (not bundled);
- provider access like Ante/ForgeCode in v1;
- Windows only.

**The engine stays the Claude Code CLI.** Rust drives it over stream-json and its stdio control protocol. That is proven:
- `src-tauri/src/cli_e2e.rs` drives the real CLI: permission prompt → allow → file on disk → saved JSONL → `--resume`.
- The SDK's `sdk.mjs` lists about 60 control requests the CLI accepts: `rewind_files`, `set_max_thinking_tokens`, `generate_session_title`, `get_context_usage`, `mcp_*`, `claude_authenticate`, `background_tasks`, and others.

Everything else (sessions, providers, the UI, Windows integration) is Rust plus a compiled web UI in WebView2.

### What M0 starts from
- `src-tauri/src/cli.rs`: spawns `claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --permission-prompt-tool stdio`. It holds pending permission prompts and answers them with the CLI's own input (the page can't change it). It also handles AskUserQuestion answers, interrupt, set_mode and set_model.
- `src-tauri/src/sessions.rs`: lists and loads the CLI's JSONL under `~/.claude/projects/<slug>/`, gives titles, and renames through `custom-title` lines.
- `src-tauri/src/settings.rs`: settings JSON and endpoint profiles, with keys in Windows Credential Manager (`keyring`).
- `src-tauri/src/desktop.rs`: Tauri commands with checked input.
- `src-tauri/src/git.rs`: status, diff and discard.
- `test/stub-anthropic.mjs` (stub API) and `cli_e2e.rs`. All 17 Rust tests pass.
- Draft UI in `web/` (vanilla JS): this plan replaces it, but keeps the logic of `web/markdown.js` (escape-first markdown) and `web/diff.js` (Myers diff, fuzz-tested).

---

## 1. Architecture

```
┌──────────────────────── WebView2: UI (Svelte 5 + TS, compiled) ─────────────────────────┐
│ Title bar · Sidebar · Thread · Composer · Cards · Changes · Settings · Onboarding       │
└───────────── typed commands (ui/src/lib/ipc.ts) ▲ one coalesced event channel ───────────────┘
                                             │   (page is untrusted: every input checked)
┌─────────────────────────────── src-tauri (app shell) ───────────────────────────────────┐
│ commands.rs · events.rs · window.rs (frameless, snap layouts, state, single instance)   │
│ toasts · tray · updater · installer                                                     │
└───────────────────────────────────────────┬─────────────────────────────────────────────┘
┌───────────────────────────── crates/forge-core (no Tauri) ──────────────────────────────┐
│ SessionManager ─ CLI driver (cli.rs) ── claude.exe per live session                     │
│   │  state machine, prewarm, reaping, crash-resume, event normalisation                 │
│ SessionStore ── JSONL index (+ app metadata: pinned/unread/archived/route)              │
│ Gateway ─────── loopback HTTP: Anthropic Messages in → provider wires out               │
│ Catalog ─────── providers, models, capabilities, prices, discovery, health              │
│ Credentials (Credential Manager) · Settings · Git · FileSearch · Log                    │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Cargo workspace.** `crates/forge-core` holds the logic and builds and tests without a window. `src-tauri` is a thin shell. The `cli.rs`, `sessions.rs`, `settings.rs` and `git.rs` that exist now move into core unchanged.
- **Rust owns session state.** CLI events are normalised in Rust into a typed `SessionEvent` enum: TextDelta, ThinkingDelta, ToolStart, ToolInput, ToolResult, Permission, Question, PlanReview, TurnEnd{usage, cost}, Status, Error, Exited. Rust keeps each live session's transcript model. Then:
  - a page reload or project switch restores live sessions;
  - toasts, tray badges and sidebar status work even when the page isn't looking.
- **Typed IPC.** `ui/src/lib/ipc.ts` declares every command and event in TypeScript; a Rust test fails if a command or a `SessionEvent` kind is missing from it, so the contract can't drift. (tauri-specta was tried and dropped: its beta TypeScript exporter refuses `serde_json::Value`, which tool inputs need.)
- **Security rules (carried over and extended).**
  - Page input is untrusted: ids must be UUIDs, paths must be recent projects or files git lists, links must be http(s).
  - A permission answer can only name a pending request, and allowed input is always the CLI's own.
  - Provider keys never reach the CLI or the page: the gateway injects them.
  - The gateway listens only on 127.0.0.1, with a random port and a per-launch bearer token.
  - Rust runs only fixed commands. The page never chooses what runs.

## 2. Connecting LLMs (Ante/ForgeCode-style)

**Two routes, chosen per session:**

1. **Claude account.** The CLI uses its own login. Sign-in happens inside the app through the CLI's control requests (`claude_authenticate` → `claude_oauth_callback` / `claude_oauth_wait_for_completion`), so no terminal is needed. No gateway is involved.
2. **Provider via the Forge Gateway.** The CLI is started with `ANTHROPIC_BASE_URL=http://127.0.0.1:<port>`, `ANTHROPIC_AUTH_TOKEN=<launch token>`, `ANTHROPIC_MODEL=<provider>/<model>`, and `ANTHROPIC_DEFAULT_HAIKU_MODEL=<provider>/<fast model>` (used for titles and other background calls). The gateway routes by the `provider/` prefix, attaches the real key from Credential Manager, translates the wire format, and streams the reply back.

**Gateway wires** (axum + reqwest/rustls, streaming end to end, a pooled client per provider):

| Wire | Covers | Notes |
|---|---|---|
| `anthropic` (passthrough) | Anthropic API, and providers with Anthropic-compatible endpoints (Ollama 0.14+, LM Studio, z.ai, Kimi, DeepSeek, LiteLLM, gateways) | Rewrites auth and headers only. Keeps prompt caching. |
| `openai-chat` | OpenAI, OpenRouter, xAI, Groq, Cerebras, Mistral, Together, Fireworks, DeepSeek, Gemini (via its OpenAI-compatible endpoint), vLLM, llama.cpp, Jan, LM Studio, Ollama | Full translation, below. |
| `openai-responses` | OpenAI reasoning models | Lands right after `openai-chat`, still in v1. |
| native env | Bedrock and Vertex (Claude) | `CLAUDE_CODE_USE_BEDROCK` / `CLAUDE_CODE_USE_VERTEX` plus their credentials. No gateway. |

**Translation (Messages ↔ chat completions), in both directions and streamed:**
- system prompt;
- text and image blocks (images as data URLs);
- `tool_use` ↔ `tool_calls`, and `tool_result` ↔ `role: tool`;
- tool schemas and `tool_choice`;
- SSE chunks → `message_start` / `content_block_*` / `input_json_delta` / `message_delta`;
- stop reasons: stop→end_turn, length→max_tokens, tool_calls→tool_use;
- usage → the Anthropic usage shape;
- reasoning: `reasoning_content` deltas → `thinking` blocks, and the Anthropic `thinking` request → `reasoning_effort`. Prior thinking blocks are stripped for providers that aren't Anthropic;
- `count_tokens` → a local estimate;
- provider HTTP errors → Anthropic error types (rate_limit, overloaded, authentication), so the CLI's own retries keep working.

**Catalog:**
- Built-in presets live in a bundled `catalog.json`: id, name, wire, base URL, auth kind, default and fast model, docs link.
- You can extend or override it with `%APPDATA%\com.msaid.forge\catalog.json` (the same idea as Ante's `~/.ante/catalog.json` and ForgeCode's custom providers).
- Models are discovered from each provider's `/models` (OpenRouter adds context window and prices; Ollama uses `/api/tags`). The list is cached on disk with a TTL.
- Each model shows capability badges: tools, vision, reasoning, context. A model without tool calling is flagged "chat only: agent tools unavailable".

**Connect flow:**
1. Settings → Providers → choose a tile:
   - Claude account;
   - Anthropic, OpenAI, Google, OpenRouter, xAI, Groq, DeepSeek, Mistral and the other presets;
   - local servers found by probing their default ports (Ollama, LM Studio, llama.cpp, vLLM, Jan) when the page opens, not by polling;
   - Custom URL.
2. Paste a key, or use OAuth where the provider offers it (Claude, OpenRouter PKCE).
3. The app validates the key by listing models.
4. Pick the default and fast models.
5. The key is saved in Credential Manager.

**Model picker** (composer pill, Ctrl+/):
- searchable with fuzzy match, grouped by provider, with badges and the last measured latency;
- switching within a route: `set_model` (instant);
- switching across routes (account ↔ gateway): the CLI restarts with `--resume` behind the scenes (~1–2 s), and the thread stays in place.

## 3. Sessions

- **Source of truth.** The transcript is the CLI's JSONL, shared with the terminal. Data the app owns goes in `%APPDATA%\com.msaid.forge\sessions.json`: pinned, unread, archived, route, model, mode and generated title.
- **State machine:** Draft (id reserved) → Warming (spawned, `initialize`) → Ready ⇄ Running ⇄ NeedsYou (prompt or question) → Sleeping (process reaped; resumable) → Archived. Failed shows a Resume button (respawn with `--resume`).
- **Process manager:**
  - prewarm a CLI when the composer gets focus or a session opens, so the first token isn't held up by CLI startup;
  - reap Ready sessions idle for more than 10 minutes, LRU, with at most 6 live processes (configurable). Running and NeedsYou sessions are never reaped;
  - if a CLI crashes mid-turn, show a notice with Resume.
- **Fast list:**
  - an index persisted with mtime and size;
  - titles found by substring scan (`memchr`), not by parsing every line;
  - a `notify` watcher on the project folder, so sessions started in the terminal appear live.
- **Features:**
  - rename, pin, archive and unread;
  - search: titles first, full text in Rust;
  - fork (`--resume <id> --fork-session`);
  - AI titles (`generate_session_title`);
  - interrupt, and queued follow-up messages;
  - context meter (`get_context_usage`);
  - thinking level (`set_max_thinking_tokens`);
  - background tasks (`background_tasks` / `stop_task`).

## 4. UI and design system (DESIGN.md → app)

- **Stack.** Svelte 5 + TypeScript + Vite: compiled, fine-grained updates, small runtime. It replaces the vanilla `web/` draft and ports `markdown.js` and `diff.js` to `ui/src/lib/`.
  - Code highlighting: highlight.js core with languages loaded on demand, only on finished code blocks.
  - Long threads render the newest 200 items first and page in older ones on scroll.
- **Tokens:**
  - colours: canvas `#f6f5f4`; cards `#fff` with a 1px hairline `rgba(0,0,0,.08)` and **no shadows**; ink built from black at 95 / 60 / 40% opacity (warm body text `#615d59` for secondary prose);
  - blue `#0075de` for the **one** filled action per surface (Send, Allow, Save). A sky-tint ghost (`#e6f3fe` with blue text) for the second action;
  - radii: cards 12, buttons 8, pills 9999, small 4. Nothing rectangular goes above 12 (the draft's 18px composer and gradient logo are fixed);
  - motion: 200 ms ease; springs only on small marks; `prefers-reduced-motion` respected;
  - no gradients;
  - elevation: only floating menus and popovers, using the DESIGN.md "nav" shadow.
- **Type:**
  - DM Sans for everything (it fills the NotionInter role): 400 body, 500 UI, 600–700 headings, with tight tracking on display sizes;
  - Source Serif 4 (DESIGN.md's own substitute for Lyon Text), only for editorial moments: the empty-state subhead and Settings page intros;
  - JetBrains Mono for code only;
  - the fonts are bundled (OFL), Latin subsets.
- **Accent cast as meaning, not decoration.** Status pills: sky = working, marigold = needs you, coral = error, midnight = plan. Provider tiles take "sticky-note" accent cards. The empty state uses the signature highlight pill (peach `#f6d5b8`) behind the project name: "What should we build in ⟨kryptonite⟩?"
- **Dark theme.** Derived from the same tokens, it follows Windows unless overridden:
  - canvas `#191918`, cards `#202020`;
  - hairlines `rgba(255,255,255,.08)`;
  - ink: white at the same opacity steps;
  - the same blue.
- **Screens:**
  - **Title bar** (40px): sidebar toggle, project switcher, branch pill; a drag area; Changes with +/− counts; settings; window controls with Windows 11 Snap Layouts on the maximise button.
  - **Sidebar:** New session (Ctrl+N), search, sessions grouped by date with status pills, and the account/provider chip at the bottom.
  - **Thread:** a 760px column. Tool calls are one compact row each and open to show a diff or output. Permission, question and plan cards sit inline.
  - **Composer:** a white 12px card with mode and model pills, `@` file mentions, `/` commands, and image paste.
  - **Changes pane:** files changed against HEAD with a unified diff, Discard, and Show in Explorer.
  - **Settings:** a modal window over the app (dimmed backdrop, Esc or a click outside closes it) with a searchable left nav: General, Models, Providers, Permissions, Environment, Memory & Rules, Hooks, MCP Servers, Skills, Agents, Slash Commands, Sandbox, Network, About. Claude Code's own settings files are edited in place through `cc_settings.rs`, with a User / Workspace / Local scope switch; each value says where it comes from (local > workspace > user) and can be reset. Anything that makes the CLI run a command (a hook, approving a project MCP server) is confirmed natively with the exact command first; the page can only remove such approvals.
  - **Onboarding:** install Claude Code if it's missing (one click runs the official installer, with progress) → connect a provider → choose a folder.
  - **Command palette:** Ctrl+K.

### UI versions
- **v1 (now):** every screen works on the real CLI (welcome, thread, tool rows, permission/question/plan cards, Changes, Settings, light and dark), built on the tokens. It is a functional first pass; the owner judged its look not good enough.
- **v2:** a dedicated design pass to sharpen the UI (layout, density, motion, iconography), after M2–M3 land, so it is designed around real sessions and providers rather than demo data.

## 5. Performance budgets (measured, enforced in CI where possible)

| Metric | Budget | How |
|---|---|---|
| Launch → window painted | ≤ 400 ms warm, no white flash | window created hidden with background = canvas; shown on the page's `ready` |
| Session list (500 sessions) | ≤ 50 ms | persisted index; scan only what changed |
| Open a session (2k messages) | ≤ 150 ms to first paint | parse off the main thread; window newest 200 |
| Send → first token (prewarmed) | provider latency + ≤ 30 ms overhead | prewarm; gateway streams without buffering |
| Streaming | 60 fps; ≤ 1 IPC message per 16 ms per session | Rust coalesces deltas; only the last block re-renders |
| Resize / drag | no main-thread I/O | already fixed; no per-resize work |
| Idle memory | app ≤ 120 MB, plus each live CLI | reaping and a live-process cap |
| Installer | ≤ 15 MB | no Node, no bundled CLI, Latin font subsets |

## 6. Windows integration
- frameless window with Snap Layouts (evaluate `tauri-plugin-decorum`);
- `tauri-plugin-window-state`;
- single instance with a `forge://open?path=` deep link and "Open with Forge" on folders;
- toasts with actions ("Allow", "Open") only while the window isn't focused;
- tray: closing the window while a session runs sends it to the tray, with a badge for "needs you";
- NSIS installer; `tauri-plugin-updater` with signed GitHub releases (code-signing certificate: decided at M6);
- logs in `%LOCALAPPDATA%\com.msaid.forge\logs`, with a viewer in About.

## 7. Milestones ("done when")

| # | Milestone | Done when |
|---|---|---|
| **M0** | Foundation | Current work committed; workspace split (`forge-core` / `src-tauri`); typed IPC; event normalisation in Rust; Windows CI (`cargo test`, NSIS build). All tests green on `windows-latest`. |
| **M1** | Premium shell | Svelte UI with DESIGN.md tokens, light and dark; title bar, sidebar, thread, composer, permission/question/plan cards, Changes, Settings; no flash; snap layouts; single instance. Screenshots match the spec; budget rows 1, 3 and 5 measured. |
| **M2** | Sessions | State machine, prewarm, reaping, crash-resume; fast index plus watcher; rename/pin/archive/unread/search/fork/titles; context meter; thinking level. A terminal-started session appears live; 500-session list ≤ 50 ms. |
| **M3** | Providers & models | Gateway (`anthropic`, `openai-chat`, then `openai-responses`); catalog plus user catalog; connect flow with validation; discovery; picker; local auto-detect; in-app Claude sign-in; Bedrock/Vertex env. The real CLI completes a tool-calling turn through the gateway against stub OpenAI and Anthropic servers in CI, and against at least 3 real providers by hand. |
| **M4** | Composer power | `@` mentions (`ignore` + `nucleo`, gitignore-aware); `/` commands from `initialize.commands`; image paste and drag; Ctrl+K palette; plan-mode UX. Mentions return in ≤ 30 ms on a 50k-file repo. |
| **M5** | Review & safety | Changes attributed per session; rewind to a message (`rewind_files`, after checking how the SDK turns on checkpointing); fork from a message; permission rules view; Open in VS Code / Cursor / Explorer. |
| **M6** | Windows polish & ship | Toasts with actions, tray, updater, signed installer, log viewer, first-run onboarding. A clean Windows 11 VM goes from install to first chat in under 2 minutes. |
| **M7** | Extensibility | MCP servers (`mcp_status` / `mcp_toggle` / `mcp_reconnect` / `mcp_authenticate`), skills, agents, hooks, memory (CLAUDE.md), worktree sessions for parallel agents. |

### Status
- **M2 (sessions), done on Linux:**
  - `session_manager.rs`: one live CLI per session; the composer's focus prewarms it (a new session's id is reserved first), so the first message skips CLI start-up. The reaper stops Ready sessions idle for 10 minutes and keeps at most 6 live, least recently used first; running and needs-you sessions are never reaped. A reaped or replaced process exits as `expected` (it sleeps; the next message resumes it). An unexpected exit, or one mid-turn, shows Resume.
  - `sessions.rs`: the index is saved in `session-index.json`; a grown file is read only from where the last read stopped, and only lines that can hold a title are parsed. 500 sessions list in about 6 ms warm or after a restart (debug build; the first-ever build of the index is about 270 ms). Titles: rename (`custom-title`) > generated (`ai-title`) > summary > first line. Full-text search over message text.
  - `watch.rs` (`notify`): a session started in the terminal appears in the sidebar without a refresh.
  - `meta.rs`: pinned and unread (`sessions.json`); a turn that ends in a session the window isn't showing marks it unread.
  - `cli.rs`: control requests with answers (`call`); after each turn, `get_context_usage` (the composer's meter) and, once, `generate_session_title` with `persist`, so the terminal sees the same title; `set_max_thinking_tokens` (composer: Auto / Off / Think / Think hard / Think harder); fork (`--resume <id> --fork-session --session-id <new>`).
  - The CLI ignores a project's permission rules until the folder is trusted (`~/.claude.json`); Settings → Permissions says so.
  - Not yet: queued follow-up messages, background tasks (`background_tasks` / `stop_task`), fork from a message (M5).

## 8. Critical files
- Move: `src-tauri/src/{cli,cli_e2e,sessions,settings,git,recent,paths}.rs` → `crates/forge-core/src/`.
- New in core:
  - `session_manager.rs` (state machine, prewarm, reaping);
  - `events.rs` (`SessionEvent` normalisation);
  - `index.rs` (fast list);
  - `gateway/{mod,anthropic,openai_chat,openai_responses,sse}.rs`;
  - `catalog/{mod,discovery,health}.rs` plus `catalog.json`;
  - `files.rs` (mentions).
- `src-tauri/src/{commands,events,window,tray}.rs`; `src-tauri/tauri.conf.json` (`frontendDist: ../ui/dist`, CSP, updater).
- `ui/` (Svelte): `src/lib/{markdown,diff,ipc}.ts` (ported from `web/markdown.js` and `web/diff.js`), `src/components/*`, `src/styles/tokens.css` (from DESIGN.md).
- Tests:
  - `test/stub-anthropic.mjs` (existing);
  - `test/stub-openai.mjs` (new);
  - `crates/forge-core/tests/{cli_e2e,gateway_e2e}.rs`.
- Docs:
  - `docs/DESIGN.md` (DESIGN.md adapted: DM Sans, dark tokens, app components);
  - `docs/ARCHITECTURE.md` (this plan, kept current);
  - `CLAUDE.md` (new working rules: standalone, page untrusted, keys never reach the CLI or the page, budgets, prove it on Windows);
  - `README.md`.

## 9. Verification
- `cargo test` in `forge-core`:
  - unit tests (protocol, index, settings, catalog, translation fixtures recorded per wire);
  - `cli_e2e`: real CLI + stub Anthropic: prompt → allow → file → resume (passes today);
  - `gateway_e2e`: real CLI → gateway → stub OpenAI: tool call round trip, a streamed thinking turn, a mapped 429 retry.
- UI:
  - Vitest for `markdown` / `diff` / stores;
  - Playwright against the built UI with mocked IPC for every card and state (light and dark screenshots).
- Real app:
  - Windows CI: `tauri-driver` (WebDriver for WebView2) smoke test: launch → onboarding → stub provider → chat → allow → Changes shows the file;
  - by hand on Windows 11: checklist per milestone, and screenshots attached to each milestone's PR (CLAUDE.md rule 5: say what was verified where).
- Performance: Rust logs timings (launch→paint, list, open, first token, IPC rate). A bench script with 500 synthetic sessions and a 5k tokens/s stub stream checks the §5 budgets.

**Sources consulted for the provider model:** [ForgeCode providers](https://www.mintlify.com/antinomyhq/forge/configuration/providers), [ForgeCode custom providers](https://forgecode.dev/docs/custom-providers/), [Ante (everydev)](https://www.everydev.ai/tools/ante), [Ante CLI (terminaltrove)](https://terminaltrove.com/ai-coding-agents/ante-cli/).
