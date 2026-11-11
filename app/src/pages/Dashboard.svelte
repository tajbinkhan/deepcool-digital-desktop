<script lang="ts">
  import { call, type Snapshot } from "../lib/api";
  import { diskGb, diskRate, gb, netRate, percent, temp } from "../lib/format";
  import { app, setCpuPower, t } from "../lib/store.svelte";
  import { dropdown } from "../lib/motion";
  import Gauge from "../components/Gauge.svelte";
  import HatchBar from "../components/HatchBar.svelte";
  import Icon from "../components/Icon.svelte";

  let s = $derived(app.snapshot);
  let f = $derived(app.prefs?.fahrenheit ?? false);
  let gpuMenu = $state(false);
  let gpuPicker = $state<HTMLDivElement>();

  async function pickGpu(index: number) {
    gpuMenu = false;
    app.snapshot = await call<Snapshot>("select_gpu", { index });
  }

  function onWindowClick(e: MouseEvent) {
    if (gpuMenu && !gpuPicker?.contains(e.target as Node)) gpuMenu = false;
  }

  function memGb(used: number | null | undefined, total: number | null | undefined) {
    return used == null || total == null ? "--" : `${gb(used)}/${gb(total)} GB`;
  }
</script>

<svelte:window onclick={onWindowClick} />

{#snippet stat(value: string, label: string)}
  <div class="stat">
    <div class="value">{value}</div>
    <div class="label">{label}</div>
  </div>
{/snippet}

{#snippet rate(icon: string, parts: [string, string], label: string)}
  <div class="rate" title={label}>
    <span class="rate-icon"><Icon name={icon} size={18} stroke={1.1} /></span>
    <b>{parts[0]}</b>&nbsp;{parts[1]}
  </div>
{/snippet}

{#if s}
  <div class="grid">
    <section class="card top">
      <header>
        <h2><Icon name="cpu" size={18} stroke={1.2} />{t("cpu")}</h2>
        <span class="aside" title={s.cpu.name}>{s.cpu.name}</span>
      </header>
      <div class="gauge-pos"><Gauge load={s.cpu.load} temp={s.cpu.temp} fahrenheit={f} /></div>
      <div class="stats">
        {@render stat(`${s.cpu.mhz} MHz`, t("cpuClock"))}
        {@render stat(temp(s.cpu.temp, f), t("cpuTemp"))}
        {#if s.cpu.power == null && app.power?.access === "restricted"}
          <div class="stat">
            <div class="value">
              --
              <button class="allow" title={t("cpuPowerUnavailable")} disabled={app.powerBusy} onclick={() => setCpuPower(true)}>
                {app.powerBusy ? t("waiting") : t("allow")}
              </button>
            </div>
            <div class="label">{t("cpuPower")}</div>
          </div>
        {:else}
          {@render stat(s.cpu.power == null ? "--" : `${s.cpu.power} W`, t("cpuPower"))}
        {/if}
      </div>
    </section>

    <section class="card top">
      <header>
        <h2><Icon name="gpu" size={18} stroke={1.2} />{t("gpu")}</h2>
        {#if s.gpus.length > 0}
          <div class="picker" bind:this={gpuPicker}>
            <button class="aside picker-button" aria-haspopup="listbox" aria-expanded={gpuMenu} onclick={() => (gpuMenu = !gpuMenu)}>
              <span class="picker-name">{s.gpu?.name ?? ""}</span>
              <span class="chevron" class:up={gpuMenu}><Icon name="chevron-down" size={15} stroke={1.3} /></span>
            </button>
            {#if gpuMenu}
              <ul class="menu" role="listbox" transition:dropdown>
                {#each s.gpus as name, index (index)}
                  <li role="option" aria-selected={index === s.gpu?.index}>
                    <button class:selected={index === s.gpu?.index} title={name} onclick={() => pickGpu(index)}>{name}</button>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        {/if}
      </header>
      {#if s.gpu}
        <div class="gauge-pos"><Gauge load={s.gpu.load} temp={s.gpu.temp} fahrenheit={f} /></div>
        <div class="stats gpu-stats">
          {@render stat(`${s.gpu.mhz} MHz`, t("gpuClock"))}
          {@render stat(temp(s.gpu.temp, f), t("gpuTemp"))}
          {@render stat(memGb(s.gpu.memUsed, s.gpu.memTotal), t("gpuMemory"))}
        </div>
      {:else}
        <p class="empty">{t("noGpu")}</p>
      {/if}
    </section>

    <div class="row">
      <section class="card bottom">
        <header><h2><Icon name="memory" size={18} stroke={1.2} />{t("memory")}</h2></header>
        <div class="mem-bar"><HatchBar value={percent(s.memUsed, s.memTotal)} /></div>
        <div class="bar-caption">
          <span>{gb(s.memUsed)}/{gb(s.memTotal)} GB</span>
          <span>{percent(s.memUsed, s.memTotal)}%</span>
        </div>
        <div class="stats flow">
          {@render stat("--", t("frequency"))}
          {@render stat(`${gb(s.memUsed)}/${gb(s.memTotal)} GB`, t("usage"))}
        </div>
      </section>

      <section class="card bottom">
        <header><h2><Icon name="storage" size={18} stroke={1.2} />{t("storage")}</h2></header>
        <div class="rates">
          {@render rate("up", diskRate(s.diskWrite), t("write"))}
          {@render rate("down", diskRate(s.diskRead), t("read"))}
        </div>
        <div class="disks">
          {#each s.disks.slice(0, 2) as disk, i (disk.name)}
            <div class="disk" title={disk.name}>
              <div class="disk-name">{t("disk")} {i + 1}</div>
              <HatchBar value={percent(disk.used, disk.total)} height={14} />
              <div class="bar-caption">
                <span>{diskGb(disk.used)} / {diskGb(disk.total)} GB</span>
                <span>{percent(disk.used, disk.total)}%</span>
              </div>
            </div>
          {/each}
        </div>
      </section>

      <section class="card bottom">
        <header><h2><Icon name="network" size={18} stroke={1.2} />{t("network")}</h2></header>
        <div class="rates">
          {@render rate("up", netRate(s.netUp), t("upload"))}
          {@render rate("down", netRate(s.netDown), t("download"))}
        </div>
      </section>
    </div>
  </div>
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 218px 241px;
    gap: 12px;
    padding: 12px;
  }
  .row {
    grid-column: 1 / -1;
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 12px;
  }
  .card {
    position: relative;
    background: var(--surface);
    border: 1px solid var(--card-border);
    min-width: 0;
  }
  header {
    height: 20px;
    margin: 16px 17px 0 18px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 14px;
    font-weight: 500;
    color: var(--text);
    white-space: nowrap;
  }
  .bottom header {
    margin-top: 17px;
  }
  .aside {
    font-size: 14px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .gauge-pos {
    position: absolute;
    left: 13.5px;
    top: 43px;
  }
  .stats {
    position: absolute;
    left: 282px;
    top: 53px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .gpu-stats {
    top: 50px;
  }
  .stats.flow {
    position: static;
    margin: 13px 18px 0;
    gap: 11px;
  }
  .stat .value {
    line-height: 18px;
  }
  .stat .label {
    margin-top: 1px;
  }
  .allow {
    margin-left: 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--teal);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .allow:disabled {
    color: var(--text-muted);
    text-decoration: none;
    cursor: default;
  }
  .empty {
    margin: 60px 18px;
    color: var(--text-muted);
  }

  .picker {
    position: relative;
    min-width: 0;
  }
  .picker-button {
    display: flex;
    align-items: center;
    gap: 7px;
    max-width: 200px;
  }
  .chevron {
    flex: none;
    display: inline-grid;
    color: var(--text-muted);
    transition: transform 0.2s ease;
  }
  .chevron.up {
    transform: rotate(180deg);
  }
  .picker-name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .menu {
    position: absolute;
    z-index: 10;
    right: -12px;
    top: 26px;
    width: max-content;
    min-width: 150px;
    max-width: 260px;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--surface);
    border-radius: 4px;
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.12);
  }
  .menu button {
    width: 100%;
    height: 34px;
    padding: 0 12px;
    text-align: left;
    color: var(--text-secondary);
    border-radius: 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .menu button:hover,
  .menu button.selected {
    background: var(--teal-soft);
  }

  .mem-bar {
    margin: 19px 18px 0 16px;
  }
  .bar-caption {
    display: flex;
    justify-content: space-between;
    margin-top: 4px;
    font-size: 12px;
    line-height: 14px;
    color: var(--text-muted);
  }
  .bottom > .bar-caption {
    margin: 4px 17px 0 19px;
  }
  .rates {
    margin: 16px 18px 0 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .rate {
    display: flex;
    align-items: center;
    height: 18px;
    font-size: 14px;
    color: var(--text);
  }
  .rate b {
    font-weight: 700;
  }
  .rate-icon {
    display: inline-grid;
    color: #c4c4c4;
    margin-right: 9px;
  }
  .disks {
    margin: 13px 18px 0 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .disk-name {
    font-size: 14px;
    line-height: 16px;
    margin: 0 0 7px 2px;
    color: var(--text);
  }
  .disk .bar-caption {
    margin: 3px 1px 0 2px;
  }
</style>
