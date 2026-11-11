<script lang="ts">
  import { call, type Profile } from "../lib/api";
  import { toUnit, unitSymbol } from "../lib/format";
  import { app, refreshDevices, t } from "../lib/store.svelte";
  import type { Key } from "../lib/i18n";
  import LdScreen from "../components/LdScreen.svelte";
  import Select from "../components/Select.svelte";
  import Toggle from "../components/Toggle.svelte";

  let { pid }: { pid: number } = $props();

  let device = $derived(app.devices.find((d) => d.pid === pid));
  let caps = $derived(device?.capabilities);
  let f = $derived(app.prefs?.fahrenheit ?? false);
  let cpu = $derived(app.snapshot?.cpu);
  let tempValue = $derived(cpu?.temp == null ? 0 : Math.round(toUnit(cpu.temp, f)));
  let error = $state<string | null>(null);

  const INTERVALS = [250, 500, 1000, 1500, 2000];

  function modeOptions(modes: string[], withDefault: boolean) {
    const list = withDefault ? ["Device default", ...modes] : modes;
    return list.map((m) => ({ value: m, label: t(`mode.${m}` as Key) }));
  }

  async function update(changes: Partial<Profile>) {
    if (!device) return;
    error = null;
    try {
      await call("save_profile", { pid, profile: { ...device.profile, ...changes } });
    } catch (e) {
      error = String(e);
    }
    await refreshDevices();
  }

  async function setShowing(on: boolean) {
    await update({ autostart: on });
    try {
      await call(on ? "start_display" : "stop_display", { pid });
    } catch (e) {
      error = String(e);
    }
    await refreshDevices();
  }
</script>

<div class="panel">
  <h1>
    <button class="crumb" onclick={() => (app.detail = null)}>{t("device")}</button>
    <span class="sep">/</span>
    <span>{device?.series ?? ""}</span>
  </h1>

  {#if device && caps}
    <div class="layout">
      <section class="settings">
        <h2>{t("digitalDisplaySettings")}</h2>
        <div class="rows">
          {#if caps.leadZeros}
            <Toggle checked={device.profile.zeros} onchange={(v) => update({ zeros: v })}>{t("leadingZero")}</Toggle>
          {/if}
          {#if caps.alarm}
            <Toggle checked={device.profile.alarm} onchange={(v) => update({ alarm: v })}>{t("alarm")}</Toggle>
          {/if}
          <Toggle checked={device.running || device.profile.autostart} onchange={setShowing}>{t("showReadings")}</Toggle>
        </div>

        {#if caps.modes.length > 1}
          <h3>{t("displayMode")}</h3>
          <Select width={260} label={t("displayMode")} value={device.profile.mode}
            options={modeOptions(caps.modes, true)} onchange={(v) => update({ mode: v })} />
        {/if}
        {#if caps.secondary.length > 0}
          <h3>{t("secondaryMode")}</h3>
          <Select width={260} label={t("secondaryMode")} value={device.profile.secondary}
            options={modeOptions(caps.secondary, true)} onchange={(v) => update({ secondary: v })} />
        {/if}
        {#if caps.rotate}
          <h3>{t("rotation")}</h3>
          <Select width={260} label={t("rotation")} value={String(device.profile.rotate)}
            options={[0, 90, 180, 270].map((r) => ({ value: String(r), label: `${r}°` }))}
            onchange={(v) => update({ rotate: Number(v) })} />
        {/if}
        <h3>{t("updateInterval")}</h3>
        <Select width={260} label={t("updateInterval")} value={String(device.profile.update)}
          options={(INTERVALS.includes(device.profile.update) ? INTERVALS : [...INTERVALS, device.profile.update].sort((a, b) => a - b))
            .map((ms) => ({ value: String(ms), label: `${ms} ms` }))}
          onchange={(v) => update({ update: Number(v) })} />

        {#if error ?? device.error}<p class="error" role="alert">{error ?? device.error}</p>{/if}
      </section>

      <aside class="preview">
        <div class="screen">
          {#if device.display === "ld"}
            <LdScreen power={cpu?.power ?? 0} temp={tempValue} usage={cpu?.load ?? 0} fahrenheit={f}
              leadZeros={device.profile.zeros} />
          {:else}
            <div class="generic">
              <span>{tempValue}{unitSymbol(f)}</span>
              <span>{cpu?.load ?? 0}%</span>
            </div>
          {/if}
        </div>
        <div class="readings">
          <div><b>{cpu?.power == null ? "--" : `${cpu.power}W`}</b><small>{t("cpuPower")}</small></div>
          <div><b>{cpu?.temp == null ? "--" : `${tempValue}${f ? "°F" : "°C"}`}</b><small>{t("cpuTemp")}</small></div>
          <div><b>{cpu?.load ?? 0}%</b><small>{t("cpuUsage")}</small></div>
        </div>
      </aside>
    </div>
  {/if}
</div>

<style>
  .panel {
    margin: 12px;
    min-height: calc(100% - 24px);
    background: var(--surface);
    padding: 11px 20px 24px 17px;
  }
  h1 {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 20px;
    line-height: 28px;
    font-weight: 500;
    color: var(--text);
  }
  .crumb {
    font-weight: 400;
    color: var(--text-secondary);
  }
  .crumb:hover {
    color: var(--teal);
  }
  .sep {
    font-weight: 400;
    color: var(--text-muted);
  }
  .layout {
    display: flex;
    justify-content: space-between;
    margin-top: 45px;
  }
  .settings {
    padding-left: 8px;
    min-width: 0;
  }
  h2,
  h3 {
    font-size: 14px;
    line-height: 20px;
    font-weight: 500;
    color: var(--text);
  }
  h3 {
    margin: 18px 0 8px;
  }
  .rows {
    margin: 9px 0 0 11px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h3 + :global(.select) {
    margin-left: 11px;
  }
  .error {
    margin: 16px 0 0 11px;
    max-width: 320px;
    font-size: 12px;
    color: #c0392b;
    -webkit-user-select: text;
    user-select: text;
  }
  .preview {
    width: 364px;
    flex: none;
    margin-top: 14px;
    border: 1px solid var(--line);
  }
  .screen {
    height: 262px;
    background: #f5f5f5;
    display: grid;
    place-items: center;
    padding-bottom: 2px;
  }
  .generic {
    width: 236px;
    height: 150px;
    border-radius: 20px;
    background: #000;
    border: 8px solid #444;
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: space-around;
    font-size: 36px;
    font-weight: 700;
  }
  .readings {
    display: grid;
    grid-template-columns: 173px 1fr;
    row-gap: 17px;
    padding: 21px 16px 18px;
  }
  .readings div {
    display: flex;
    flex-direction: column;
  }
  .readings b {
    font-size: 14px;
    line-height: 18px;
    font-weight: 700;
    color: var(--text);
  }
  .readings small {
    margin-top: 2px;
    font-size: 12px;
    line-height: 15px;
    color: var(--text-muted);
  }
</style>
