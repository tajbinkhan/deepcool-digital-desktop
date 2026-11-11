<script lang="ts">
  // Drop-down list styled like the reference (36 px box, floating menu).
  import { dropdown } from "../lib/motion";
  import Icon from "./Icon.svelte";

  let {
    value,
    options,
    onchange,
    width = 300,
    label,
  }: {
    value: string;
    options: { value: string; label: string }[];
    onchange: (value: string) => void;
    width?: number;
    label?: string;
  } = $props();

  let open = $state(false);
  let root: HTMLDivElement;
  let current = $derived(options.find((o) => o.value === value)?.label ?? value);

  function choose(v: string) {
    open = false;
    if (v !== value) onchange(v);
  }

  function onWindowClick(e: MouseEvent) {
    if (open && !root.contains(e.target as Node)) open = false;
  }

  function onKey(e: KeyboardEvent) {
    const i = options.findIndex((o) => o.value === value);
    if (e.key === "Escape") open = false;
    else if (e.key === "ArrowDown" && i < options.length - 1) (e.preventDefault(), onchange(options[i + 1].value));
    else if (e.key === "ArrowUp" && i > 0) (e.preventDefault(), onchange(options[i - 1].value));
  }
</script>

<svelte:window onclick={onWindowClick} />

<div class="select" class:open bind:this={root} style:width="{width}px">
  <button
    class="box"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label={label}
    onclick={() => (open = !open)}
    onkeydown={onKey}
  >
    <span>{current}</span>
    <span class="chevron"><Icon name="chevron-down" size={16} stroke={1.3} /></span>
  </button>
  {#if open}
    <ul class="menu" role="listbox" transition:dropdown>
      {#each options as option (option.value)}
        <li role="option" aria-selected={option.value === value}>
          <button class:selected={option.value === value} onclick={() => choose(option.value)}>{option.label}</button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .select {
    position: relative;
  }
  .box {
    width: 100%;
    height: 36px;
    padding: 0 12px 0 15px;
    border: 1px solid var(--control-border);
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--text-secondary);
    font-size: 14px;
    background: var(--surface);
    transition: border-color 0.2s;
  }
  .chevron {
    display: inline-grid;
    color: #a8abb2;
    transition: transform 0.2s ease;
  }
  .open .chevron {
    transform: rotate(180deg);
  }
  .open .box,
  .box:hover {
    border-color: var(--teal);
  }
  .menu {
    position: absolute;
    z-index: 20;
    left: 0;
    right: 0;
    top: 46px;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--surface);
    border-radius: 4px;
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.12);
  }
  .menu button {
    width: 100%;
    height: 36px;
    padding: 0 20px;
    text-align: left;
    color: var(--text-secondary);
    border-radius: 4px;
  }
  .menu button:hover,
  .menu button.selected {
    background: var(--teal-soft);
  }
</style>
