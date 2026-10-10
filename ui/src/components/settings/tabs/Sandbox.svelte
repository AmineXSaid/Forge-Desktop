<script lang="ts">
  import Section from '../Section.svelte';
  import Field from '../Field.svelte';
  import { app } from '../../../lib/store.svelte';
</script>

{#if app.info?.platform === 'windows'}
  <p class="note">Claude Code's sandbox runs on macOS and Linux (and WSL 2). On Windows these settings are kept, but not applied.</p>
{/if}

<Section title="Sandbox">
  <Field key="sandbox.enabled" type="bool" fallback={false} label="Sandbox shell commands" desc="Bash runs in an OS sandbox that limits what it can read, write and reach." />
  <Field key="sandbox.autoAllowBashIfSandboxed" type="bool" fallback={true} label="Allow sandboxed commands without asking" desc="A command that stays in the sandbox needs no permission prompt." />
  <Field key="sandbox.allowUnsandboxedCommands" type="bool" fallback={true} label="Allow commands outside the sandbox" desc="A command may ask to run unsandboxed. Off: everything stays in." />
  <Field key="sandbox.excludedCommands" type="list" label="Always outside the sandbox" desc="Commands that run unsandboxed, such as docker." placeholder="docker" />
</Section>

<Section title="Sandbox network">
  <Field key="sandbox.network.allowLocalBinding" type="bool" fallback={false} label="Allow local ports" desc="Sandboxed commands may listen on localhost, for dev servers (macOS)." />
  <Field key="sandbox.network.allowUnixSockets" type="list" label="Unix sockets" desc="Socket paths sandboxed commands may connect to, such as the SSH agent." placeholder="/var/run/docker.sock" />
  <Field key="sandbox.network.httpProxyPort" type="number" min={1} max={65535} placeholder="Auto" label="HTTP proxy port" desc="Your own HTTP proxy for sandboxed network access." />
  <Field key="sandbox.network.socksProxyPort" type="number" min={1} max={65535} placeholder="Auto" label="SOCKS proxy port" desc="Your own SOCKS5 proxy for sandboxed network access." />
</Section>

<style>
  .note { margin: 0 0 18px; font-size: 13px; color: var(--status-needs-fg); background: var(--status-needs-bg); padding: 10px 14px; border-radius: var(--r-button); }
</style>
