<script lang="ts">
  import Section from '../Section.svelte';
  import Field from '../Field.svelte';
  import Row from '../Row.svelte';
  import Toggle from '../Toggle.svelte';
  import { effective } from '../../../lib/ccsettings';
  import { settingsWindow as sw } from '../../../lib/settings.svelte';

  const bypassOff = $derived(effective(sw.cc, 'permissions.disableBypassPermissionsMode').value === 'disable');
</script>

<Section title="Default mode">
  <Field key="permissions.defaultMode" type="choice" label="Permission mode" desc="How Claude Code starts when Forge doesn't choose. Bypass is only ever chosen per session, in the composer."
    options={[{ value: 'default', label: 'Ask before acting' }, { value: 'acceptEdits', label: 'Accept edits' }, { value: 'plan', label: 'Plan' }, { value: 'auto', label: 'Auto' }, { value: 'dontAsk', label: "Don't ask (deny)" }]} />
  <Row key="permissions.disableBypassPermissionsMode" label="Disable bypass mode" desc="Claude Code refuses to run with every check off.">
    <Toggle checked={bypassOff} label="Disable bypass mode" onchange={(v) => sw.set('permissions.disableBypassPermissionsMode', v ? 'disable' : null)} />
  </Row>
</Section>

<Section title="Rules" caption="Lists from every scope are combined. Deny wins over ask, ask over allow. Examples: Bash(npm run test:*), Read(./secrets/**), WebFetch(domain:example.com).">
  <Field key="permissions.deny" type="list" label="Deny" desc="Never allowed." placeholder="Bash(rm -rf:*)" />
  <Field key="permissions.ask" type="list" label="Ask" desc="Always ask first, even in Accept edits or Auto." placeholder="Bash(git push:*)" />
  <Field key="permissions.allow" type="list" label="Allow" desc="Allowed without asking." placeholder="Bash(npm run test:*)" />
</Section>

<Section title="Additional directories">
  <Field key="permissions.additionalDirectories" type="list" label="Directories" desc="Folders outside the project Claude Code may read and edit." placeholder="../shared or C:\docs" />
</Section>
