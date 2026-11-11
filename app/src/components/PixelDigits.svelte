<script lang="ts">
  // Dot-matrix number, used for the device index ("01").
  let { text, cell = 3.4, color = "#444" }: { text: string; cell?: number; color?: string } = $props();

  // 5×7 patterns; zero is slashed.
  const FONT: Record<string, string[]> = {
    "0": ["01110", "10011", "10101", "10101", "10101", "11001", "01110"],
    "1": ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
    "2": ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
    "3": ["11110", "00001", "00001", "01110", "00001", "00001", "11110"],
    "4": ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
    "5": ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
    "6": ["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
    "7": ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
    "8": ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
    "9": ["01110", "10001", "10001", "01111", "00001", "00010", "01100"],
  };
  const GAP = 6; // cells between digits

  let cells = $derived(
    [...text].flatMap((ch, i) =>
      (FONT[ch] ?? FONT["0"]).flatMap((row, y) =>
        [...row].flatMap((bit, x) => (bit === "1" ? [{ x: i * (5 + GAP) + x, y }] : [])),
      ),
    ),
  );
  let width = $derived((text.length * (5 + GAP) - GAP) * cell);
</script>

<svg width={width} height={7 * cell} viewBox="0 0 {width} {7 * cell}" aria-label={text} role="img">
  {#each cells as c (c.x * 10 + c.y)}
    <rect x={c.x * cell + 0.3} y={c.y * cell + 0.3} width={cell - 0.6} height={cell - 0.6} fill={color} />
  {/each}
</svg>
