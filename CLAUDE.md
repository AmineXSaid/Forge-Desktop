# Forge Desktop — working rules

Forge Desktop is a standalone Windows desktop app for Claude Code, written in
Rust (Tauri 2) with its own UI. The plan is `docs/ARCHITECTURE.md`; the visual
system is `docs/DESIGN.md`. It does not depend on the FORGE repository.

1. **The CLI is the engine; Rust is everything else.** Each chat session is a
   `claude` process driven over stream-json and its stdio control protocol
   (`cli.rs`). Sessions, providers (the gateway), settings, git, files and the
   window are Rust. Don't add a Node runtime to the app.
2. **The page is untrusted.** Every command checks its arguments: session ids
   are UUIDs, project paths are recent projects or picked folders, files are
   ones git lists or inside the project, links are http(s). A permission answer
   can only name a prompt the CLI is waiting on, and the input that runs is
   always the CLI's own. The page never chooses a command to run.
3. **Keys never reach the CLI or the page.** Provider keys live in Windows
   Credential Manager. The gateway (127.0.0.1, random port, per-launch token)
   attaches them; the CLI only sees the gateway's token.
4. **Fast is a feature.** Hold the budgets in `docs/ARCHITECTURE.md` §5. No I/O
   or blocking work on the UI thread; coalesce streaming events; never poll what
   can be watched.
5. **Follow `docs/DESIGN.md`.** Tokens, not ad-hoc colours; one blue action per
   surface; hairlines, not shadows, on cards; no gradients; DM Sans.
6. **Prove it on the real thing.** `cli_e2e` drives the real CLI against a stub
   API; the gateway's e2e does the same through each wire. For UI changes, run
   the app and look at it. Windows is the only release target: say what was
   verified on Windows and what only on the Linux dev box.
