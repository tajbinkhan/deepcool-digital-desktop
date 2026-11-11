<script lang="ts">
  // Two-ring gauge: the inner ring shows load (0–100 %), the outer ring shows
  // temperature on a 0–120 °C scale with a tick at 110 °C. Geometry measured from
  // the reference: centre (105, 90), outer r 82.5, inner r 70.
  import { t } from "../lib/store.svelte";
  import { toUnit, unitSymbol } from "../lib/format";

  let { load, temp, fahrenheit }: { load: number; temp: number | null; fahrenheit: boolean } = $props();

  const CX = 105;
  const CY = 90;
  const OUTER = { r: 82.5, start: 183.5, sweep: 187 };
  const INNER = { r: 70, start: 216, sweep: 252 };
  const MAX_C = 120;
  const TICK_C = 109.2;

  function point(r: number, deg: number): [number, number] {
    const rad = (deg * Math.PI) / 180;
    return [CX + r * Math.cos(rad), CY - r * Math.sin(rad)];
  }

  /** Clockwise arc from `start` covering `fraction` of `sweep` degrees. */
  function arc(r: number, start: number, sweep: number, fraction: number): string {
    const f = Math.min(Math.max(fraction, 0), 1);
    const end = start - sweep * f;
    const [x1, y1] = point(r, start);
    const [x2, y2] = point(r, end);
    const large = sweep * f > 180 ? 1 : 0;
    return `M${x1.toFixed(2)} ${y1.toFixed(2)}A${r} ${r} 0 ${large} 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
  }

  // A short radial line across the outer ring (measured: 13.3°, radius 79.5–86).
  const tickAngle = OUTER.start - OUTER.sweep * (TICK_C / MAX_C);
  const [tx1, ty1] = point(OUTER.r - 3, tickAngle);
  const [tx2, ty2] = point(OUTER.r + 3.5, tickAngle);

  let loadPath = $derived(load > 0 ? arc(INNER.r, INNER.start, INNER.sweep, load / 100) : "");
  let tempPath = $derived(temp != null && temp > 0 ? arc(OUTER.r, OUTER.start, OUTER.sweep, temp / MAX_C) : "");
  let maxLabel = $derived(`${Math.round(toUnit(MAX_C, fahrenheit))} ${unitSymbol(fahrenheit)}`);
  let minLabel = $derived(`${Math.round(toUnit(0, fahrenheit))} ${unitSymbol(fahrenheit)}`);
</script>

<div class="gauge">
  <svg width="210" height="150" viewBox="0 0 210 150" aria-hidden="true">
    <path d={arc(OUTER.r, OUTER.start, OUTER.sweep, 1)} stroke="var(--track)" stroke-width="4" fill="none" />
    <path d={arc(INNER.r, INNER.start, INNER.sweep, 1)} stroke="var(--track)" stroke-width="6" fill="none" stroke-linecap="round" />
    {#if tempPath}<path d={tempPath} stroke="var(--teal-arc)" stroke-width="5" fill="none" />{/if}
    {#if loadPath}<path d={loadPath} stroke="var(--teal-arc)" stroke-width="6" fill="none" stroke-linecap="round" />{/if}
    <line x1={tx1} y1={ty1} x2={tx2} y2={ty2} stroke="var(--danger)" stroke-width="1.5" />
    <text x="25.5" y="109.5" text-anchor="end" class="scale">{minLabel}</text>
    <text x="181.5" y="109.5" class="scale">{maxLabel}</text>
  </svg>
  <div class="center">
    <span class="caption">{t("load")}</span>
    <span class="number">{Math.round(load)}<span class="pct">%</span></span>
  </div>
</div>

<style>
  .gauge {
    position: relative;
    width: 210px;
    height: 150px;
  }
  svg {
    display: block;
    overflow: visible;
  }
  .scale {
    font-size: 10px;
    fill: var(--text-muted);
  }
  .center {
    position: absolute;
    left: 0;
    right: 0;
    top: 59px;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .caption {
    font-size: 11px;
    line-height: 14px;
    color: var(--text-muted);
  }
  .number {
    position: relative;
    margin-top: 1px;
    font-size: 42px;
    line-height: 44px;
    color: var(--text);
  }
  .pct {
    position: absolute;
    left: 100%;
    bottom: 7px;
    margin-left: 2px;
    font-size: 14px;
    line-height: 1;
  }
</style>
