<script lang="ts">
  import { call } from "../lib/api";
  import { LANGUAGES } from "../lib/i18n";
  import { app, savePrefs, setCpuPower, t } from "../lib/store.svelte";
  import Select from "../components/Select.svelte";
  import Toggle from "../components/Toggle.svelte";

  const APP = "DeepCool Digital";
  let notice = $state<string | null>(null);
  let error = $state<string | null>(null);

  /** Splits "Boot {app} Auto-start" so the app name can be bold. */
  function parts(text: string): [string, string] {
    const [before, after = ""] = text.split("{app}");
    return [before, after];
  }

  async function change(changes: Parameters<typeof savePrefs>[0]) {
    error = null;
    try {
      await savePrefs(changes);
    } catch (e) {
      error = String(e);
    }
  }

  async function openWebsite() {
    try {
      await call("open_website");
    } catch (e) {
      error = String(e);
    }
  }
</script>

{#snippet withApp(text: string)}
  {@const [before, after] = parts(text)}
  {before}<b>{APP}</b>{after}
{/snippet}

<div class="panel">
  {#if app.prefs}
    <section>
      <h2>{t("startupSettings")}</h2>
      <div class="rows">
        <Toggle checked={app.prefs.start_at_login} onchange={(v) => change({ start_at_login: v })}>
          {@render withApp(t("bootAutostart", { app: "{app}" }))}
        </Toggle>
      </div>
    </section>

    <section>
      <h2>{t("tempDisplay")}</h2>
      <div class="radios" role="radiogroup" aria-label={t("tempDisplay")}>
        <label class="radio">
          <input type="radio" name="unit" checked={!app.prefs.fahrenheit} onchange={() => change({ fahrenheit: false })} />
          <span class="dot" aria-hidden="true"></span>{t("celsius")}
        </label>
        <label class="radio">
          <input type="radio" name="unit" checked={app.prefs.fahrenheit} onchange={() => change({ fahrenheit: true })} />
          <span class="dot" aria-hidden="true"></span>{t("fahrenheitUnit")}
        </label>
      </div>
    </section>

    <section>
      <h2>{t("language")}</h2>
      <div class="indent">
        <Select label={t("language")} value={app.lang} options={LANGUAGES.map((l) => ({ value: l.code, label: l.name }))}
          onchange={(v) => change({ language: v })} />
      </div>
    </section>

    <section class="update">
      <h2>{t("updateSettings")}</h2>
      <p class="version">{t("version", { v: app.version })}</p>
      <div class="buttons">
        <button class="outline wide" onclick={() => (notice = t("updateUnavailable"))}>{t("checkUpdates")}</button>
        <button class="outline" onclick={openWebsite}>{t("officialWebsite")}</button>
      </div>
      <div class="rows">
        <Toggle checked={app.prefs.check_for_updates} onchange={(v) => change({ check_for_updates: v })}>
          {@render withApp(t("autoUpdate", { app: "{app}" }))}
        </Toggle>
      </div>
      {#if notice}<p class="notice" role="status">{notice}</p>{/if}
    </section>

    <section>
      <h2>{t("backgroundSettings")}</h2>
      <div class="rows">
        <Toggle checked={app.prefs.run_in_background} onchange={(v) => change({ run_in_background: v })}>
          {t("keepRunning")}
        </Toggle>
      </div>
      {#if error}<p class="notice error" role="alert">{error}</p>{/if}
    </section>

    {#if app.power}
      {@const power = app.power}
      <section>
        <h2>{t("cpuPowerSettings")}</h2>
        <div class="rows">
          <Toggle
            checked={power.access === "readable"}
            disabled={app.powerBusy || power.access === "no_sensor" || (power.access === "readable" && !power.ruleInstalled)}
            onchange={(v) => setCpuPower(v)}
          >
            {app.powerBusy ? t("waiting") : t("cpuPowerToggle")}
          </Toggle>
        </div>
        {#if power.access === "restricted"}
          <p class="hint">{t("cpuPowerRestricted")}</p>
        {:else if power.access === "no_sensor"}
          <p class="hint">{t("cpuPowerNoSensor")}</p>
        {:else if !power.ruleInstalled}
          <p class="hint">{t("cpuPowerExternal")}</p>
        {/if}
        {#if app.powerError}<p class="notice error" role="alert">{app.powerError}</p>{/if}
      </section>
    {/if}
  {/if}
</div>

<style>
  .panel {
    margin: 12px;
    min-height: calc(100% - 24px);
    background: var(--surface);
    padding: 16px 24px 24px;
  }
  section + section {
    margin-top: 16px;
  }
  section.update {
    margin-top: 22px;
  }
  h2 {
    font-size: 14px;
    line-height: 20px;
    font-weight: 500;
    color: var(--text);
  }
  .rows {
    margin: 7px 0 0 11px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rows :global(b) {
    font-weight: 700;
  }
  .radios {
    display: flex;
    margin: 8px 0 0 12px;
  }
  .radio {
    position: relative;
    width: 154px;
    display: inline-flex;
    align-items: center;
    gap: 9px;
    font-size: 14px;
    line-height: 20px;
    cursor: pointer;
  }
  .radio input {
    position: absolute;
    opacity: 0;
  }
  .dot {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1px solid var(--control-border);
    display: grid;
    place-items: center;
  }
  .radio input:checked + .dot {
    border-color: var(--teal);
  }
  .radio input:checked + .dot::after {
    content: "";
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--teal);
  }
  .radio input:focus-visible + .dot {
    outline: 2px solid var(--teal);
    outline-offset: 2px;
  }
  .indent {
    margin: 9px 0 0 12px;
  }
  .version {
    margin: 9px 0 0 13px;
    font-size: 14px;
    line-height: 20px;
    color: var(--text-muted);
  }
  .buttons {
    display: flex;
    gap: 21px;
    margin: 8px 0 0 12px;
  }
  .outline {
    height: 36px;
    min-width: 170px;
    padding: 0 20px;
    border: 1px solid var(--teal);
    border-radius: 4px;
    color: var(--teal);
    font-size: 14px;
  }
  .outline.wide {
    min-width: 191px;
  }
  .outline:hover {
    background: var(--teal-soft);
  }
  .buttons + .rows {
    margin-top: 11px;
  }
  .hint {
    margin: 6px 0 0 41px;
    max-width: 520px;
    font-size: 12px;
    line-height: 17px;
    color: var(--text-muted);
  }
  .notice {
    margin: 12px 0 0 12px;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .notice.error {
    color: #c0392b;
  }
</style>
