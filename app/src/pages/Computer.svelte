<script lang="ts">
  import { call, type SystemInfo } from "../lib/api";
  import { t } from "../lib/store.svelte";

  let info = $state<SystemInfo | null>(null);
  call<SystemInfo>("get_system_info").then((value) => (info = value));

  /** Installed memory, rounded up to whole GB (the kernel hides some). */
  function memory(bytes: number) {
    return `${Math.ceil(bytes / 1024 ** 3 / 2) * 2} GB`;
  }
</script>

{#snippet row(label: string, values: string[])}
  <dt>{label}</dt>
  {#each values as value, i (i)}<dd>{value}</dd>{/each}
{/snippet}

<div class="panel">
  <h1>{t("computerConfig")}</h1>
  {#if info}
    <dl>
      {@render row(t("deviceName"), [info.hostname])}
      {@render row(t("system"), [info.os])}
      {@render row(t("cpu"), [info.cpu])}
      {@render row(t("gpu"), info.gpus.length ? info.gpus : [t("unknown")])}
      {@render row(t("motherboard"), [info.motherboard ?? t("unknown")])}
      {@render row(t("memory"), [memory(info.memoryTotal)])}
      {@render row(
        t("storageDrives"),
        info.drives.length ? info.drives.map((d) => (d.serial ? `${d.model} (${d.serial})` : d.model)) : [t("unknown")],
      )}
    </dl>
  {/if}
</div>

<style>
  .panel {
    margin: 12px;
    min-height: calc(100% - 24px);
    background: var(--surface);
    padding: 11px 17px 24px;
  }
  h1 {
    font-size: 20px;
    line-height: 28px;
    font-weight: 500;
    color: var(--text);
  }
  dl {
    margin: 19px 0 0 8px;
  }
  dt {
    font-size: 14px;
    line-height: 20px;
    font-weight: 500;
    color: var(--text);
    margin-top: 16px;
  }
  dt:first-child {
    margin-top: 0;
  }
  dd {
    margin: 4px 0 0 13px;
    font-size: 14px;
    line-height: 20px;
    color: var(--text-muted);
    -webkit-user-select: text;
    user-select: text;
  }
</style>
