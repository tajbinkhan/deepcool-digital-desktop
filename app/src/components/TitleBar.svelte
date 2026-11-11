<script lang="ts">
  // Logo strip under the system's own title bar (the window keeps native
  // decorations, so buttons, shadow and resizing follow the desktop).
  import Logo from "./Logo.svelte";
</script>

<header class="strip">
  <Logo height={20} />
</header>

<style>
  .strip {
    height: 45px;
    flex: none;
    display: flex;
    align-items: center;
    padding-left: 20px;
    border-bottom: 1px solid #f1f1f1;
    background: var(--surface);
  }
</style>
