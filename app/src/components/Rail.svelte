<script lang="ts">
  import { app, go, t, type Page } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";

  const top: Page[] = ["dashboard", "computer", "device"];
</script>

{#snippet item(page: Page)}
  <button
    class="item"
    class:active={app.page === page}
    title={t(page)}
    aria-label={t(page)}
    aria-current={app.page === page ? "page" : undefined}
    onclick={() => go(page)}
  >
    <Icon name={page} size={22} stroke={1.4} />
    {#if page === "settings" && app.update?.state === "available"}<span class="badge" aria-hidden="true"></span>{/if}
  </button>
{/snippet}

<nav class="rail">
  <div class="group">
    {#each top as page (page)}{@render item(page)}{/each}
  </div>
  <div class="bottom">
    <hr />
    {@render item("settings")}
  </div>
</nav>

<style>
  .rail {
    width: var(--rail-w);
    background: var(--surface);
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 30px 0 16px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 37px;
  }
  .item {
    position: relative;
    height: 22px;
    width: 100%;
    display: grid;
    place-items: center;
    color: var(--icon);
  }
  .item:hover {
    color: var(--teal);
  }
  .item.active {
    color: var(--teal);
  }
  .item.active::before {
    content: "";
    position: absolute;
    left: 6px;
    top: 5px;
    width: 2px;
    height: 12px;
    background: var(--teal);
  }
  .badge {
    position: absolute;
    top: -2px;
    left: 34px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--danger);
  }
  .bottom hr {
    border: 0;
    border-top: 1px solid var(--divider);
    margin: 0 10px 16px 14px;
  }
</style>
