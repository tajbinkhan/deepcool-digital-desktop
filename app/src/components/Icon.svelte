<script lang="ts">
  // Line icons drawn for this app (24×24 grid, stroke follows the text colour).
  let { name, size = 20, stroke = 1.5 }: { name: string; size?: number; stroke?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === "dashboard"}
    <circle cx="12" cy="12" r="9.5" />
    <path d="M7 9.2 5.6 8.4M12 6.2V4.6M17 9.2l1.4-.8" />
    <path d="M14.8 9.4 12.6 14a1.4 1.4 0 1 1-1.9-1.7z" />
    <path d="M8.2 17.5h7.6" />
  {:else if name === "computer"}
    <path d="M8 4.5h12.5v10.5H19" />
    <rect x="3.5" y="7.5" width="13" height="10" />
    <path d="M10 17.5v2.5M6.5 20h7" />
  {:else if name === "device"}
    <rect x="3.5" y="4" width="17" height="7" />
    <rect x="3.5" y="13" width="17" height="7" />
    <path d="M6.5 7.5h1M10 7.5h7.5M6.5 16.5h1M10 16.5h7.5" />
  {:else if name === "settings"}
    <path
      d="M10.3 3.5h3.4l.5 2.4 1.6.9 2.3-.8 1.7 2.9-1.8 1.6v1.9l1.8 1.6-1.7 2.9-2.3-.8-1.6.9-.5 2.4h-3.4l-.5-2.4-1.6-.9-2.3.8-1.7-2.9 1.8-1.6v-1.9L4.2 8.9l1.7-2.9 2.3.8 1.6-.9z"
    />
    <circle cx="12" cy="12" r="2.8" />
  {:else if name === "cpu"}
    <rect x="6" y="6" width="12" height="12" rx="1" />
    <rect x="9" y="9" width="6" height="6" />
    <path d="M9 3v3M12 3v3M15 3v3M9 18v3M12 18v3M15 18v3M3 9h3M3 12h3M3 15h3M18 9h3M18 12h3M18 15h3" />
  {:else if name === "gpu"}
    <path d="M2.5 5h2v14M4.5 7h17v11h-17" />
    <rect x="7" y="9.5" width="6" height="4.5" />
    <path d="M15 10h4M15 12h4M15 14h4M7 18v2h3v-2" />
  {:else if name === "memory"}
    <rect x="3" y="8" width="18" height="7" />
    <path d="M6 11h2M10 11h2M14 11h2M18 11h.5M5 15v2M19 15v2" />
  {:else if name === "storage"}
    <rect x="4.5" y="4" width="15" height="16" rx="2" />
    <path d="M4.5 15h15M7.5 17.5h9" />
  {:else if name === "network"}
    <path d="M7 15.5a4.5 4.5 0 0 1-.4-9 6 6 0 0 1 11 1.6 3.7 3.7 0 0 1-.6 7.4z" />
    <path d="M12 15.5v3M6 20.5h12M12 18.5v2" />
  {:else if name === "up" || name === "down"}
    <circle cx="12" cy="12" r="9" stroke-dasharray="2.2 1.4" />
    {#if name === "up"}
      <path d="M12 16.5v-9M8.5 11 12 7.5l3.5 3.5" />
    {:else}
      <path d="M12 7.5v9M8.5 13l3.5 3.5 3.5-3.5" />
    {/if}
  {:else if name === "chevron-down"}
    <path d="M7 10l5 5 5-5" />
  {:else if name === "chevron-up"}
    <path d="M7 14l5-5 5 5" />
  {:else if name === "chevron-right"}
    <path d="M10 7l5 5-5 5" />
  {:else if name === "minimize"}
    <path d="M6 12h12" />
  {:else if name === "maximize"}
    <rect x="6.5" y="6.5" width="11" height="11" />
  {:else if name === "restore"}
    <rect x="6.5" y="8.5" width="9" height="9" />
    <path d="M9 8.5V6.5h9v9h-2.5" />
  {:else if name === "close"}
    <path d="M6.5 6.5l11 11M17.5 6.5l-11 11" />
  {/if}
</svg>
