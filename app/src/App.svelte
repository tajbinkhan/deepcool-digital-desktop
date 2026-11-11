<script lang="ts">
  import { onMount } from "svelte";
  import { app, init } from "./lib/store.svelte";
  import TitleBar from "./components/TitleBar.svelte";
  import Rail from "./components/Rail.svelte";
  import Dashboard from "./pages/Dashboard.svelte";
  import Computer from "./pages/Computer.svelte";
  import Devices from "./pages/Devices.svelte";
  import DeviceDetail from "./pages/DeviceDetail.svelte";
  import Settings from "./pages/Settings.svelte";

  onMount(() => {
    init();
  });
</script>

<div class="window">
  <TitleBar />
  <div class="body">
    <Rail />
    <main>
      {#if app.page === "dashboard"}
        <Dashboard />
      {:else if app.page === "computer"}
        <Computer />
      {:else if app.page === "device"}
        {#if app.detail != null}<DeviceDetail pid={app.detail} />{:else}<Devices />{/if}
      {:else}
        <Settings />
      {/if}
    </main>
  </div>
</div>

<style>
  .window {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  main {
    flex: 1;
    min-width: 0;
    background: var(--page);
    overflow: auto;
  }
</style>
