<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { inTauri } from "../lib/api";
  import { t } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";

  const win = inTauri ? getCurrentWindow() : null;
  let maximized = $state(false);

  async function toggleMaximize() {
    await win?.toggleMaximize();
    maximized = (await win?.isMaximized()) ?? false;
  }
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region><Logo /></div>
  <div class="controls">
    <button title={t("minimize")} aria-label={t("minimize")} onclick={() => win?.minimize()}>
      <Icon name="minimize" size={20} stroke={1} />
    </button>
    <button title={t("maximize")} aria-label={t("maximize")} onclick={toggleMaximize}>
      <Icon name={maximized ? "restore" : "maximize"} size={20} stroke={1} />
    </button>
    <button class="close" title={t("close")} aria-label={t("close")} onclick={() => win?.close()}>
      <Icon name="close" size={20} stroke={1} />
    </button>
  </div>
</header>

<style>
  .titlebar {
    height: calc(var(--titlebar-h) + 1px);
    border-bottom: 1px solid #f1f1f1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: var(--surface);
  }
  .brand {
    padding-left: 20px;
    display: flex;
    align-items: center;
  }
  .controls {
    display: flex;
    height: 100%;
    padding-right: 15px;
  }
  .controls button {
    width: 40px;
    height: 100%;
    display: grid;
    place-items: center;
    color: #1f1f1f;
  }
  .controls button:hover {
    background: #f2f2f2;
  }
  .controls .close:hover {
    background: #e81123;
    color: #fff;
  }
</style>
