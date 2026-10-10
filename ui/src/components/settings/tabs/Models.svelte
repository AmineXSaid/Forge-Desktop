<script lang="ts">
  import Section from '../Section.svelte';
  import Field from '../Field.svelte';
  import Row from '../Row.svelte';
  import { effective, getPath } from '../../../lib/ccsettings';
  import { settingsWindow as sw } from '../../../lib/settings.svelte';

  // What the edited scope sets, else what applies.
  const model = $derived((getPath(sw.cc[sw.scope], 'model') ?? effective(sw.cc, 'model').value) as string | undefined);
</script>

<Section title="Model">
  <Field key="model" label="Default model" desc="An alias (opus, sonnet, haiku) or a full model name. Not set: Claude Code's default." placeholder="Claude Code's default" mono />
  <Row label="Quick pick" desc="Writes the alias to the scope above.">
    {#each ['opus', 'sonnet', 'haiku'] as m}
      <button class="btn sm" class:outline={model !== m} class:ghost-blue={model === m} onclick={() => sw.set('model', m)}>{m}</button>
    {/each}
  </Row>
</Section>

<Section title="Thinking & effort">
  <Field key="alwaysThinkingEnabled" type="bool" fallback={false} label="Always think" desc="Extended thinking on for every request." />
  <Field key="effortLevel" type="choice" label="Effort level" desc="How hard the model works on each request. Higher is slower and more thorough."
    options={[{ value: 'low', label: 'Low' }, { value: 'medium', label: 'Medium' }, { value: 'high', label: 'High' }, { value: 'xhigh', label: 'Extra high' }, { value: 'max', label: 'Max' }]} />
</Section>

<Section title="Limits" caption="Environment variables Claude Code reads. Empty: its default.">
  <Field key="env.MAX_THINKING_TOKENS" type="env" label="Max thinking tokens" desc="The most tokens a request may spend thinking." placeholder="Default" mono />
  <Field key="env.CLAUDE_CODE_MAX_OUTPUT_TOKENS" type="env" label="Max output tokens" desc="The most tokens one response may contain." placeholder="Default" mono />
</Section>

<Section title="Model routing" caption="Which model each alias and background job uses. Useful with a provider whose model names differ.">
  <Field key="env.ANTHROPIC_DEFAULT_OPUS_MODEL" type="env" label="opus" desc="The model the opus alias means." placeholder="Default" mono />
  <Field key="env.ANTHROPIC_DEFAULT_SONNET_MODEL" type="env" label="sonnet" desc="The model the sonnet alias means." placeholder="Default" mono />
  <Field key="env.ANTHROPIC_DEFAULT_HAIKU_MODEL" type="env" label="haiku" desc="The fast model: titles, summaries and other background calls." placeholder="Default" mono />
  <Field key="env.CLAUDE_CODE_SUBAGENT_MODEL" type="env" label="Subagents" desc="The model subagents use." placeholder="Default" mono />
</Section>
