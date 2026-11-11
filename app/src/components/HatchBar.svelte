<script lang="ts">
  // Teal-outlined bar with a hatched fill, as in the Memory and Storage cards.
  let { value, height = 48 }: { value: number; height?: number } = $props();
  let width = $derived(Math.min(Math.max(value, 0), 100));
</script>

<div class="bar" style:height="{height}px" role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(width)}>
  <div class="fill" style:width="{width}%"></div>
</div>

<style>
  .bar {
    border: 1px solid var(--teal);
    background: var(--surface);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background-color: var(--teal-fill);
    background-image: repeating-linear-gradient(
      -45deg,
      transparent 0,
      transparent 5px,
      var(--teal-hatch) 5px,
      var(--teal-hatch) 6px
    );
    transition: width 0.4s ease;
  }
</style>
