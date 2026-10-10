<script lang="ts">
  let { checked, label, onchange, disabled = false }: { checked: boolean; label: string; onchange: (v: boolean) => void; disabled?: boolean } = $props();
</script>

<button type="button" class="sw" class:on={checked} role="switch" aria-checked={checked} aria-label={label} title={label} {disabled} onclick={() => onchange(!checked)}>
  <span class="knob"></span>
</button>

<style>
  .sw { width: 34px; height: 20px; border-radius: var(--r-pill); background: var(--ink-5); position: relative; flex: none; transition: background var(--t) var(--ease); }
  .sw.on { background: var(--blue); }
  .sw:disabled { opacity: 0.4; cursor: default; }
  .knob { position: absolute; top: 2px; left: 2px; width: 16px; height: 16px; border-radius: 50%; background: #fff; transition: transform var(--t) var(--ease); }
  .on .knob { transform: translateX(14px); }
</style>
