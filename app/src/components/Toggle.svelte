<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    checked,
    onchange,
    disabled = false,
    children,
  }: { checked: boolean; onchange: (value: boolean) => void; disabled?: boolean; children?: Snippet } = $props();
</script>

<label class="toggle" class:disabled>
  <input type="checkbox" role="switch" {checked} {disabled} onchange={(e) => onchange(e.currentTarget.checked)} />
  <span class="track" aria-hidden="true"><span class="knob"></span></span>
  {#if children}<span class="text">{@render children()}</span>{/if}
</label>

<style>
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    font-size: 14px;
    line-height: 20px;
    color: var(--text);
    cursor: pointer;
  }
  .toggle.disabled {
    opacity: 0.5;
    cursor: default;
  }
  input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
  }
  .track {
    position: relative;
    width: 21px;
    height: 12px;
    border-radius: 6px;
    background: #c0c4cc;
    transition: background 0.2s;
    flex: none;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #fff;
    transition: left 0.2s;
  }
  input:checked + .track {
    background: var(--teal);
  }
  input:checked + .track .knob {
    left: 11px;
  }
  input:focus-visible + .track {
    outline: 2px solid var(--teal);
    outline-offset: 2px;
  }
</style>
