<script lang="ts">
  // Illustrations drawn for this app: a pump head with a glowing rim and tubes
  // (water coolers), a fin tower with a fan (air coolers), or a case.
  let { category, uid }: { category: string; uid: number } = $props();
  const id = (name: string) => `${name}-${uid}`;
</script>

<svg width="170" height="120" viewBox="0 0 170 120" aria-hidden="true">
  <defs>
    <linearGradient id={id("rim")} x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#ff4fd8" />
      <stop offset="0.3" stop-color="#7a5cff" />
      <stop offset="0.55" stop-color="#2fd0ff" />
      <stop offset="0.8" stop-color="#3dff8a" />
      <stop offset="1" stop-color="#ffe14d" />
    </linearGradient>
    <linearGradient id={id("body")} x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#2a2a2a" />
      <stop offset="1" stop-color="#0d0d0d" />
    </linearGradient>
  </defs>

  {#if category === "water_cooler"}
    <!-- tubes -->
    <path d="M86 70c40 0 74 6 74 24s-34 20-90 20H50" fill="none" stroke="#151515" stroke-width="10" stroke-linecap="round" />
    <path d="M86 82c30 0 58 4 58 13s-24 11-70 11H52" fill="none" stroke="#262626" stroke-width="9" stroke-linecap="round" />
    <!-- pump head -->
    <rect x="16" y="3" width="80" height="94" rx="13" fill={`url(#${id("body")})`} />
    <rect x="10" y="3" width="80" height="86" rx="13" fill="#111" stroke={`url(#${id("rim")})`} stroke-width="3" />
    <rect x="16" y="9" width="68" height="74" rx="8" fill="#000" />
    {#each [0, 1, 2] as row (row)}
      {#each [0, 1, 2] as col (col)}
        <rect x={33 + col * 14} y={17 + row * 21} width="10" height="16" rx="1.5" fill="none" stroke="#f2f2f2" stroke-width="2" />
      {/each}
      <rect x="21" y={19 + row * 21} width="3" height="12" fill="#bbb" />
    {/each}
  {:else if category === "air_cooler"}
    <rect x="40" y="6" width="90" height="100" rx="4" fill="#d9d9d9" />
    {#each Array(16) as _, i (i)}
      <rect x="40" y={9 + i * 6} width="90" height="2" fill="#a6a6a6" />
    {/each}
    <rect x="30" y="8" width="18" height="96" rx="3" fill="#1a1a1a" />
    <circle cx="85" cy="56" r="36" fill="#1d1d1d" />
    <circle cx="85" cy="56" r="10" fill="#333" />
    <rect x="62" y="0" width="46" height="14" rx="3" fill="#000" stroke={`url(#${id("rim")})`} stroke-width="1.5" />
  {:else}
    <rect x="45" y="4" width="80" height="110" rx="4" fill="#1b1b1b" />
    <rect x="52" y="12" width="66" height="94" rx="2" fill="#2b2b2b" stroke="#3d3d3d" />
    <rect x="58" y="18" width="54" height="22" rx="2" fill="#000" stroke={`url(#${id("rim")})`} stroke-width="1.5" />
    <circle cx="85" cy="66" r="16" fill="#111" stroke="#555" />
    <circle cx="85" cy="94" r="8" fill="#111" stroke="#555" />
  {/if}
</svg>
