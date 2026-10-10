---
paths:
  - "src-tauri/src/**"
  - "crates/forge-core/src/**"
---

# Commands: the page is untrusted

Every command checks its arguments:

- session ids are UUIDs;
- project paths are recent projects or folders the user picked;
- files are ones git lists, or inside the project;
- links are http(s).

A permission answer can only name a prompt the CLI is waiting on, and the input
that runs is always the CLI's own. The page never chooses a command to run.
