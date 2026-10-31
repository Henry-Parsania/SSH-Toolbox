<script lang="ts">
  import { platformInfo, type PlatformInfo } from "./lib/backend";

  let info = $state<PlatformInfo | null>(null);
  let failure = $state<string | null>(null);

  $effect(() => {
    platformInfo().then(
      (result) => (info = result),
      (err: { message?: string }) => (failure = err.message ?? "Couldn't read system details."),
    );
  });
</script>

<main>
  <h1>SSH Toolbox</h1>
  {#if failure}
    <p class="error">{failure}</p>
  {:else if info}
    <p>Running on <strong>{info.os}</strong></p>
  {:else}
    <p>Loading…</p>
  {/if}
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    padding: 2rem;
  }
  .error {
    color: #b00020;
  }
</style>
