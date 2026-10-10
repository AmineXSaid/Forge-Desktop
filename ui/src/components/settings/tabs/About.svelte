<script lang="ts">
  import Section from '../Section.svelte';
  import Row from '../Row.svelte';
  import { app } from '../../../lib/store.svelte';
</script>

<Section title="Versions">
  <Row label="Forge Desktop" desc="Version {app.info?.version ?? '?'}"><span></span></Row>
  <Row label="Claude Code" desc={app.info?.claude ?? 'Not found on PATH. Install it: irm https://claude.ai/install.ps1 | iex'}><span></span></Row>
</Section>
<Section title="Where things are">
  <Row label="Conversations" desc="~/.claude/projects, shared with the terminal."><span></span></Row>
  <Row label="Claude Code settings" desc="~/.claude/settings.json, and .claude/settings.json and settings.local.json in each project."><span></span></Row>
  <Row label="Forge settings and logs" desc="%APPDATA%\com.msaid.forge and %LOCALAPPDATA%\com.msaid.forge\logs. Provider keys: Windows Credential Manager."><span></span></Row>
</Section>
<Section title="Shortcuts">
  {#each [['Ctrl N', 'New session'], ['Ctrl O', 'Open a folder'], ['Ctrl B', 'Sidebar'], ['Ctrl Shift D', 'Changes'], ['Ctrl ,', 'Settings'], ['Shift Tab', 'Permission mode'], ['Esc', 'Stop, or close this window']] as [k, what]}
    <Row label={what}><kbd>{k}</kbd></Row>
  {/each}
</Section>
