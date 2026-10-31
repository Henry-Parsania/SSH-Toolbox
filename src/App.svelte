<script lang="ts">
  import { listShells, platformInfo, type PlatformInfo, type Shell } from "./lib/backend";
  import TerminalView from "./lib/terminal/TerminalView.svelte";

  let platform = $state<PlatformInfo | null>(null);
  let shell = $state<Shell | null>(null);
  let failure = $state<string | null>(null);

  // TEMPORARY, until the pane tree and tabs arrive: one local shell fills the window.
  $effect(() => {
    Promise.all([platformInfo(), listShells()]).then(
      ([info, shells]) => {
        platform = info;
        shell = shells[0] ?? null;
        if (!shell) failure = "No shell was found on this computer.";
      },
      (err: { message?: string }) => (failure = err.message ?? "Couldn't start."),
    );
  });
</script>

{#if failure}
  <p class="error">{failure}</p>
{:else if platform && shell}
  <TerminalView spec={{ type: "local", shell_id: shell.id }} {platform} />
{/if}

<style>
  :global(html, body, #app) {
    height: 100%;
    margin: 0;
    background: #1e1e1e;
    overflow: hidden;
  }
  .error {
    color: #f48771;
    font-family: system-ui, sans-serif;
    padding: 2rem;
  }
</style>
