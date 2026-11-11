<script lang="ts">
  import { temp } from "../lib/format";
  import { app, t } from "../lib/store.svelte";
  import DeviceArt from "../components/DeviceArt.svelte";
  import Icon from "../components/Icon.svelte";
  import PixelDigits from "../components/PixelDigits.svelte";
  import type { Key } from "../lib/i18n";

  let f = $derived(app.prefs?.fahrenheit ?? false);
  let cpu = $derived(app.snapshot?.cpu);

  function category(key: string): string {
    return ["water_cooler", "air_cooler", "case"].includes(key) ? t(key as Key) : t("other");
  }
</script>

<div class="panel">
  <h1>{t("device")}</h1>
  {#each app.devices as device, i (device.pid)}
    <button class="card" onclick={() => (app.detail = device.pid)} disabled={!device.supported}>
      <span class="index"><PixelDigits text={String(i + 1).padStart(2, "0")} /></span>
      <span class="names">
        <span class="series">{device.series}</span>
        <span class="category">{device.supported ? category(device.category) : t("unsupported")}</span>
      </span>
      <span class="art"><DeviceArt category={device.category} uid={device.pid} /></span>
      <span class="stats">
        <span class="stat"><b>{temp(cpu?.temp, f)}</b><small>{t("cpuTemp")}</small></span>
        <span class="stat"><b>{cpu?.load ?? 0} %</b><small>{t("cpuUsage")}</small></span>
        <span class="stat"><b>{cpu?.power == null ? "--" : `${cpu.power} W`}</b><small>{t("cpuPower")}</small></span>
      </span>
      <span class="chevron"><Icon name="chevron-right" size={18} stroke={1.6} /></span>
    </button>
  {:else}
    <div class="empty">
      <p>{t("noDevices")}</p>
      <p class="hint">{t("noDevicesHint")}</p>
    </div>
  {/each}
</div>

<style>
  .panel {
    margin: 12px;
    min-height: calc(100% - 24px);
    background: var(--surface);
    padding: 11px 16px 24px;
  }
  h1 {
    font-size: 20px;
    line-height: 28px;
    font-weight: 500;
    color: var(--text);
    margin-left: 1px;
    margin-bottom: 9px;
  }
  .card {
    position: relative;
    width: 100%;
    height: 140px;
    margin-bottom: 12px;
    display: flex;
    align-items: center;
    background: var(--tile);
    text-align: left;
  }
  .card:hover:not(:disabled) {
    background: #f3f3f3;
  }
  .card:disabled {
    cursor: default;
  }
  .index {
    display: flex;
    margin-left: 33px;
    margin-top: 2px;
    width: 66px;
  }
  .names {
    display: flex;
    flex-direction: column;
  }
  .series {
    font-size: 32px;
    line-height: 38px;
    font-weight: 500;
    color: var(--text);
  }
  .category {
    margin-top: 8px;
    font-size: 18px;
    line-height: 22px;
    color: var(--text-secondary);
  }
  .art {
    position: absolute;
    left: 462px;
    top: 9px;
  }
  .stats {
    position: absolute;
    left: 661px;
    top: 9px;
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .stat {
    display: flex;
    flex-direction: column;
  }
  .stat b {
    font-size: 14px;
    line-height: 18px;
    font-weight: 700;
    color: var(--text);
  }
  .stat small {
    margin-top: 2px;
    font-size: 12px;
    line-height: 15px;
    color: var(--text-muted);
  }
  .chevron {
    position: absolute;
    right: 29px;
    top: 61px;
    color: var(--text);
  }
  .empty {
    padding: 40px 0;
    text-align: center;
    color: var(--text-secondary);
  }
  .hint {
    margin-top: 6px;
    font-size: 12px;
    color: var(--text-muted);
  }
</style>
