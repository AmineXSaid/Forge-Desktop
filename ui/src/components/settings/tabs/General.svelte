<script lang="ts">
  import Section from '../Section.svelte';
  import Row from '../Row.svelte';
  import Field from '../Field.svelte';
  import { app, MODES } from '../../../lib/store.svelte';
  import { getPath } from '../../../lib/ccsettings';
  import { settingsWindow as sw } from '../../../lib/settings.svelte';

  const helper = $derived(getPath(sw.cc[sw.scope], 'apiKeyHelper'));
</script>

{#if app.settings}
  {@const s = app.settings}
  <Section title="Forge" caption="The app's own settings. They apply in every project, whatever the scope above.">
    <Row label="Theme" desc="Follows Windows unless you pick one.">
      <div class="seg">
        {#each ['system', 'light', 'dark'] as const as t}
          <button class:on={s.theme === t} onclick={() => app.saveSettings({ theme: t })}>{t[0].toUpperCase() + t.slice(1)}</button>
        {/each}
      </div>
    </Row>
    <Row label="Permission mode for new sessions" desc={MODES.find((m) => m.id === s.defaultMode)?.desc}>
      <select class="input" value={s.defaultMode} onchange={(e) => app.saveSettings({ defaultMode: e.currentTarget.value })} aria-label="Permission mode for new sessions">
        {#each MODES as m}<option value={m.id}>{m.label}</option>{/each}
      </select>
    </Row>
    <Row label="Model for new sessions" desc="Overrides Claude Code's model (Models tab) in Forge. Empty: use that one.">
      <input class="input" placeholder="Claude Code's model" value={s.defaultModel ?? ''} onchange={(e) => app.saveSettings({ defaultModel: e.currentTarget.value.trim() || null })} aria-label="Model for new sessions" />
    </Row>
  </Section>
{/if}

<Section title="Language & output">
  <Field key="language" label="Language" desc="The language Claude replies in." placeholder="e.g. french, japanese" />
  <Field key="outputStyle" label="Output style" desc="A built-in or custom output style." placeholder="e.g. Explanatory, Learning" />
</Section>

<Section title="Agent behaviour">
  <Field key="respectGitignore" type="bool" fallback={true} label="Respect .gitignore" desc="Leave files that .gitignore matches out of file search and @-mentions." />
  <Field key="teammateMode" type="choice" fallback="auto" label="Teammate mode" desc="How parallel agent teammates run."
    options={[{ value: 'auto', label: 'Auto' }, { value: 'in-process', label: 'In-process' }, { value: 'tmux', label: 'tmux panes' }]} />
  <Field key="plansDirectory" label="Plans directory" desc="Where plan files are kept, relative to the project." placeholder="Default" mono />
  <Field key="showTurnDuration" type="bool" fallback={false} label="Show turn duration" desc="Claude Code reports how long each turn took." />
</Section>

<Section title="Git attribution">
  <Field key="attribution.commit" label="Commit message" desc="Text added to commits Claude makes. Empty string: none." placeholder="Claude Code's default" />
  <Field key="attribution.pr" label="Pull request description" desc="Text added to pull requests Claude opens." placeholder="Claude Code's default" />
  <Field key="includeCoAuthoredBy" type="bool" fallback={true} label="Co-Authored-By line" desc="Older setting; attribution above replaces it." />
</Section>

<Section title="Chat history & updates">
  <Field key="cleanupPeriodDays" type="number" min={1} max={36500} placeholder="30" label="Keep conversations for" desc="Days since a conversation's last activity before Claude Code deletes it." />
  <Field key="autoUpdatesChannel" type="choice" fallback="latest" label="Updates channel" desc="Which Claude Code releases it updates to."
    options={[{ value: 'latest', label: 'Latest' }, { value: 'stable', label: 'Stable' }]} />
  <Field key="forceLoginMethod" type="choice" label="Login method" desc="Restrict sign-in to one kind of account."
    options={[{ value: 'claudeai', label: 'Claude.ai account' }, { value: 'console', label: 'Console (API billing)' }]} />
  <Row label="API key helper" desc="A script that prints an API key. It runs a command, so Forge only shows it; edit settings.json to change it.">
    <span class="ro">{helper ? String(helper) : 'Not set'}</span>
  </Row>
</Section>

<style>
  .seg { display: inline-flex; padding: 3px; gap: 2px; border-radius: var(--r-button); background: var(--hover); }
  .seg button { height: 28px; padding: 0 12px; border-radius: 6px; font-size: 13px; font-weight: 500; color: var(--ink-3); }
  .seg button.on { background: var(--card); color: var(--ink); box-shadow: inset 0 0 0 1px var(--line); }
  .input { width: 260px; }
  select.input { width: 200px; }
  .ro { font: 12.5px var(--font-mono); color: var(--ink-3); max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
