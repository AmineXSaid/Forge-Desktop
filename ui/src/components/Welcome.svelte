<script lang="ts">
  import Composer from './Composer.svelte';
  import { app } from '../lib/store.svelte';

  let composer = $state<ReturnType<typeof Composer>>();
  const suggestions = [
    'Explain how this project is structured',
    'Find and fix a failing test',
    'Review my uncommitted changes',
    'Write a README for this project',
  ];
</script>

<section class="welcome">
  <div class="inner">
    <h1>What should we build in <span class="hl">{app.info?.name ?? 'this project'}</span>?</h1>
    <p class="lead">Claude works in <span class="path">{app.info?.workspace}</span> and asks before it changes anything.</p>
    <Composer bind:this={composer} session={app.currentSession} placeholder="Describe a task, ask a question, or paste an error…" autofocus />
    <div class="chips">
      {#each suggestions as s}
        <button class="chip" onclick={() => composer?.fill(s)}>{s}</button>
      {/each}
    </div>
  </div>
</section>

<style>
  .welcome { flex: 1; display: grid; place-items: center; padding: 24px 32px 10vh; overflow-y: auto; }
  .inner { width: 100%; max-width: 680px; }
  h1 {
    margin: 0 0 12px; font-size: 40px; line-height: 1.12; font-weight: 600; letter-spacing: -0.036em; color: var(--ink); text-align: center;
  }
  .hl { background: var(--peach); border-radius: var(--r-pill); padding: 0 0.32em 0.04em; white-space: nowrap; }
  .lead { margin: 0 0 30px; font: 400 18px/1.56 var(--font-serif); color: var(--graphite); text-align: center; }
  .path { font-family: var(--font-mono); font-size: 14px; color: var(--ink-3); }
  .chips { display: flex; flex-wrap: wrap; justify-content: center; gap: 8px; margin-top: 16px; }
  .chip {
    height: 32px; padding: 0 14px; border-radius: var(--r-pill); background: var(--card); box-shadow: inset 0 0 0 1px var(--line);
    font-size: 13px; color: var(--ink-3); transition: box-shadow var(--t) var(--ease), color var(--t) var(--ease);
  }
  .chip:hover { box-shadow: inset 0 0 0 1px var(--line-strong); color: var(--ink); }
</style>
