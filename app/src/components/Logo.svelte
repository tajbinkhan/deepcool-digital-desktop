<script lang="ts">
  // DeepCool wordmark and mark, rebuilt as vector shapes from the official logo
  // (source units; 32 px strokes, 43 px outer and 10 px inner corner radius).
  let { height = 24 }: { height?: number } = $props();
</script>

<svg
  class="logo"
  viewBox="91 92 1818 367"
  height={height}
  width={(height * 1818) / 367}
  role="img"
  aria-label="DeepCool"
>
  <g fill="#0c0808" fill-rule="evenodd">
    <!-- D -->
    <path d="M91 184H238a43 43 0 0 1 43 43V324a43 43 0 0 1-43 43H91ZM123 216V334H228a20 20 0 0 0 20-20V236a20 20 0 0 0-20-20Z" />
    <!-- E -->
    <path d="M346 216H444V249H346a10 10 0 0 0-10 10V275H441V308H336V324a10 10 0 0 0 10 10H444V367H346a43 43 0 0 1-43-43V259a43 43 0 0 1 43-43Z" />
    <path d="M510 216H608V249H510a10 10 0 0 0-10 10V275H605V308H500V324a10 10 0 0 0 10 10H608V367H510a43 43 0 0 1-43-43V259a43 43 0 0 1 43-43Z" />
    <!-- P -->
    <path d="M631 216H742a43 43 0 0 1 43 43V291a43 43 0 0 1-43 43H664V367H631ZM664 249V301H742a10 10 0 0 0 10-10V259a10 10 0 0 0-10-10Z" />
    <!-- C -->
    <path d="M851 184H981V216H852a12 12 0 0 0-12 12V322a12 12 0 0 0 12 12H981V367H851a43 43 0 0 1-43-43V227a43 43 0 0 1 43-43Z" />
    <!-- O O -->
    <path d="M1047 216H1119a43 43 0 0 1 43 43V324a43 43 0 0 1-43 43H1047a43 43 0 0 1-43-43V259a43 43 0 0 1 43-43ZM1047 249a10 10 0 0 0-10 10V324a10 10 0 0 0 10 10H1119a10 10 0 0 0 10-10V259a10 10 0 0 0-10-10Z" />
    <path d="M1227 216H1299a43 43 0 0 1 43 43V324a43 43 0 0 1-43 43H1227a43 43 0 0 1-43-43V259a43 43 0 0 1 43-43ZM1227 249a10 10 0 0 0-10 10V324a10 10 0 0 0 10 10H1299a10 10 0 0 0 10-10V259a10 10 0 0 0-10-10Z" />
    <!-- L -->
    <path d="M1364 216H1397V324a10 10 0 0 0 10 10H1499V367H1407a43 43 0 0 1-43-43Z" />
  </g>
  <g fill="#198686">
    <path d="M1725 92H1771V229H1680V275H1771V321H1634V183H1725Z" />
    <path d="M1771 321H1909V367H1817V459H1771Z" />
  </g>
</svg>

<style>
  .logo {
    display: block;
  }
</style>
