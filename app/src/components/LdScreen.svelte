<script lang="ts">
  // Live preview of the LD-Series display: three rows of 7-segment digits
  // (power, temperature, usage) with unlit segments shown dimly.
  let {
    power,
    temp,
    usage,
    fahrenheit,
    leadZeros,
    size = 236,
  }: { power: number; temp: number; usage: number; fahrenheit: boolean; leadZeros: boolean; size?: number } = $props();

  // Segment order: a b c d e f g
  const DIGITS: Record<string, number[]> = {
    "0": [1, 1, 1, 1, 1, 1, 0],
    "1": [0, 1, 1, 0, 0, 0, 0],
    "2": [1, 1, 0, 1, 1, 0, 1],
    "3": [1, 1, 1, 1, 0, 0, 1],
    "4": [0, 1, 1, 0, 0, 1, 1],
    "5": [1, 0, 1, 1, 0, 1, 1],
    "6": [1, 0, 1, 1, 1, 1, 1],
    "7": [1, 1, 1, 0, 0, 0, 0],
    "8": [1, 1, 1, 1, 1, 1, 1],
    "9": [1, 1, 1, 1, 0, 1, 1],
    " ": [0, 0, 0, 0, 0, 0, 0],
  };

  const W = 35; // digit width
  const H = 50; // digit height
  const T = 6.5; // segment thickness
  const G = 1.2; // gap between segments

  /** Hexagonal segment polygons for a digit at (0, 0). */
  function segments(): string[] {
    const h = T / 2;
    const mid = H / 2;
    const horiz = (y: number) =>
      `${G + h},${y} ${G + T},${y - h} ${W - G - T},${y - h} ${W - G - h},${y} ${W - G - T},${y + h} ${G + T},${y + h}`;
    const vert = (x: number, y0: number, y1: number) =>
      `${x},${y0 + G + h} ${x + h},${y0 + G + T} ${x + h},${y1 - G - T} ${x},${y1 - G - h} ${x - h},${y1 - G - T} ${x - h},${y0 + G + T}`;
    return [
      horiz(h), // a
      vert(W - h, 0, mid), // b
      vert(W - h, mid, H), // c
      horiz(H - h), // d
      vert(h, mid, H), // e
      vert(h, 0, mid), // f
      horiz(mid), // g
    ];
  }
  const SEGMENTS = segments();

  function text(value: number): string {
    const clamped = Math.max(0, Math.min(999, Math.round(value)));
    return leadZeros ? clamped.toString().padStart(3, "0") : clamped.toString().padStart(3, " ");
  }

  let rows = $derived([
    { label: "POWER", value: text(power), units: ["W", "W"], lit: 1 },
    { label: "TEMP.", value: text(temp), units: ["°F", "°C"], lit: fahrenheit ? 0 : 1 },
    { label: "USAGE", value: text(usage), units: ["%", "%"], lit: 1 },
  ]);
</script>

<svg width={size} height={size} viewBox="0 0 236 236" role="img" aria-label="Display preview: {power} W, {temp}°, {usage}%">
  <rect x="0" y="0" width="236" height="236" rx="34" fill="#444" />
  <rect x="8" y="8" width="220" height="220" rx="27" fill="#000" />
  <text class="side" transform="translate(33 119) rotate(-90)" text-anchor="middle">CPU</text>
  {#each rows as row, r (row.label)}
    {@const top = 32 + r * 60}
    <text class="label" transform="translate(53 {top + H / 2}) rotate(-90)" text-anchor="middle">{row.label}</text>
    {#each [...row.value] as ch, i (i)}
      <g transform="translate({65 + i * 43} {top})">
        {#each SEGMENTS as points, s (s)}
          <polygon {points} fill={DIGITS[ch][s] ? "#fff" : "#1e1e1e"} />
        {/each}
      </g>
    {/each}
    <text class="unit" x="189" y={top + 27} fill={row.lit === 0 ? "#fff" : "#262626"}>{row.units[0]}</text>
    <text class="unit" x="189" y={top + 51} fill={row.lit === 1 ? "#fff" : "#262626"}>{row.units[1]}</text>
  {/each}
</svg>

<style>
  .label,
  .side {
    font-size: 10px;
    font-weight: 700;
    fill: #fff;
    letter-spacing: -0.2px;
  }
  .side {
    font-weight: 400;
  }
  .unit {
    font-size: 13px;
    font-weight: 700;
  }
</style>
