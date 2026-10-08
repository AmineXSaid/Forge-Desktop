# Forge Desktop — working rules

Forge Desktop puts Forge (AmineXSaid/FORGE) in a Tauri window. The plan is
`docs/forge-desktop-plan.md` in FORGE; milestones and their "done when" lists
come from there. The FORGE checkout is `../FORGE`, or `$FORGE_SRC`.

1. **Forge's code runs unchanged.** The page is Forge's `dist/media` and
   Forge's own HTML. The host is Forge's `activate()`. Don't copy Forge
   logic into this repo. If the desktop needs Forge to change, change Forge,
   behind an interface, keeping the VS Code build green (plan §4.1).
2. **The shim is temporary.** `host/vscodeShim.ts` implements only what a
   working feature touches. An unimplemented member must throw by name and
   must never answer "yes" to a confirmation. A Forge reference to a member
   the shim doesn't export fails `build:host` (esbuild `import-is-undefined`).
3. **The page is untrusted.** Every page message passes
   `ipc::check_envelope`. Host calls are the fixed list in `host_calls.rs`,
   with checked parameters. Node never runs a command because the page or
   the host asked Rust to.
4. **stdout is frames only.** `host/runtime.ts` redirects console output to
   the log. Rust logs any non-frame line and never forwards it.
5. **Prove it on the real thing.** `test/sidecar.e2e.test.mjs` drives the
   real CLI. For UI changes, run the app (under Xvfb on Linux) and look at
   it. Don't claim Windows behaviour you haven't seen: say what was
   verified where.
