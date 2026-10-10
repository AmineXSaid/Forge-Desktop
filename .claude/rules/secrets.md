---
paths:
  - "crates/forge-core/src/gateway/**"
  - "crates/forge-core/src/{settings,cc_settings,catalog,cli,session_manager}.rs"
  - "src-tauri/src/**"
---

# Keys never reach the CLI or the page

Provider keys live in Windows Credential Manager. The gateway listens on
127.0.0.1, on a random port, with a per-launch token, and attaches the keys
itself. The CLI only ever sees the gateway's token, and the page never sees a
key at all, whether in an event, an error or a log line.
