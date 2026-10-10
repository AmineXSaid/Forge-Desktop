<script lang="ts">
  import Icon from './Icon.svelte';
  import { menu, closeMenu } from '../lib/menu.svelte';

  function outside(e: MouseEvent) {
    const t = e.target as Node;
    if (menu.items.length && !el?.contains(t) && !menu.anchor?.contains(t)) closeMenu();
  }
  let el = $state<HTMLDivElement>();
</script>

<svelte:window onmousedown={outside} onkeydown={(e) => e.key === 'Escape' && closeMenu()} onblur={closeMenu} />

{#if menu.items.length}
  <div class="menu" bind:this={el} role="menu" style:left="{menu.x}px" style:width="{menu.width}px" style={menu.above ? `bottom:${menu.y}px` : `top:${menu.y}px`}>
    {#each menu.items as it}
      {#if it === '-'}
        <div class="sep"></div>
      {:else if 'header' in it}
        <div class="head">{it.header}</div>
      {:else}
        <button class="item" class:danger={it.danger} role="menuitem" onclick={() => { closeMenu(); it.run(); }}>
          {#if it.icon}<Icon name={it.icon} />{/if}
          <span class="text"><span class="label">{it.label}</span>{#if it.desc}<span class="desc">{it.desc}</span>{/if}</span>
          {#if it.hint}<kbd>{it.hint}</kbd>{/if}
          {#if it.checked}<span class="check"><Icon name="check" size={14} /></span>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .menu {
    position: fixed; z-index: 100; background: var(--card); border-radius: var(--r-card); box-shadow: var(--float);
    padding: 5px; animation: rise 120ms var(--ease); max-height: 70vh; overflow-y: auto;
  }
  .item {
    display: flex; align-items: center; gap: 10px; width: 100%; padding: 7px 10px; border-radius: var(--r-button);
    text-align: left; color: var(--ink); font-size: 14px;
  }
  .item:hover { background: var(--hover); }
  .item :global(.icon) { color: var(--ink-3); }
  .text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .label { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .desc { font-size: 12px; color: var(--ink-3); line-height: 1.33; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .check { color: var(--blue); display: grid; }
  .danger, .danger :global(.icon) { color: var(--status-error-fg); }
  .head { font-size: 12px; font-weight: 500; color: var(--ink-4); padding: 8px 10px 4px; letter-spacing: 0.01em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sep { height: 1px; background: var(--line); margin: 5px 4px; }
</style>
