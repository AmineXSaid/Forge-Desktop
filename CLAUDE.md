# Forge Desktop — working rules

Forge Desktop is a standalone Windows desktop app for Claude Code, written in
Rust (Tauri 2) with its own UI. The plan is `docs/ARCHITECTURE.md`; the visual
system is `docs/DESIGN.md`. It does not depend on the FORGE repository.

These rules apply everywhere. Rules for one part of the tree live in
`.claude/rules/` and load when Claude works on those files: `commands.md`
(Rust commands), `secrets.md` (keys and the gateway), `ui.md` (the UI).

1. **The CLI is the engine; Rust is everything else.** Each chat session is a
   `claude` process driven over stream-json and its stdio control protocol
   (`cli.rs`). Sessions, providers (the gateway), settings, git, files and the
   window are Rust. Don't add a Node runtime to the app.
2. **The page is untrusted.** Every command checks its arguments, and the page
   never chooses a command to run. Details: `.claude/rules/commands.md`.
3. **Keys never reach the CLI or the page.** They live in Windows Credential
   Manager; the gateway attaches them. Details: `.claude/rules/secrets.md`.
4. **Fast is a feature.** Hold the budgets in `docs/ARCHITECTURE.md` §5. No I/O
   or blocking work on the UI thread; coalesce streaming events; never poll what
   can be watched.
5. **Follow `docs/DESIGN.md`** for anything you see. Details: `.claude/rules/ui.md`.
6. **Prove it on the real thing.** `cli_e2e` drives the real CLI against a stub
   API; the gateway's e2e does the same through each wire. For UI changes, run
   the app and look at it. Windows is the only release target: say what was
   verified on Windows and what only on the Linux dev box.
