<script lang="ts">
  // A window that shows one tab on its own: a tab moved out of the main window, or a pop-up such
  // as a server's process monitor. What to show comes from the window's address.
  import { platformInfo, windowCloseSelf, type PlatformInfo } from "./lib/backend";
  import { decodePayload } from "./lib/layout/detach";
  import PaneTree from "./lib/layout/PaneTree.svelte";
  import { Workspace } from "./lib/layout/workspace.svelte";
  import { loadSettings, settings } from "./lib/settings.svelte";

  const payload = decodePayload(new URLSearchParams(location.search).get("window") ?? "");
  const workspace = new Workspace();
  let platform = $state<PlatformInfo | null>(null);
  let failure = $state(payload ? "" : "This window was opened without anything to show.");

  if (payload) {
    workspace.attach = payload.attach;
    const tab = workspace.newTab(null, payload.title);
    tab.root = payload.root;
    tab.active = payload.active;
  }

  $effect(() => {
    Promise.all([platformInfo(), loadSettings().catch(() => {})]).then(
      ([info]) => (platform = info),
      (err: { message?: string }) => (failure = err.message ?? "Couldn't start."),
    );
  });

  $effect(() => {
    document.documentElement.dataset.theme = settings.theme;
  });

  // When the last pane is closed there is nothing left to show.
  $effect(() => {
    if (payload && workspace.tabs.length === 0) void windowCloseSelf().catch(() => {});
  });
</script>

{#if failure}
  <p class="failure">{failure}</p>
{:else if platform}
  <main>
    {#each workspace.tabs as tab (tab.id)}
      <PaneTree
        node={tab.root}
        tabId={tab.id}
        activePane={tab.active}
        visible={true}
        crowded={workspace.paneCount(tab) > 1}
        {platform}
        {workspace}
      />
    {/each}
  </main>
{/if}

<style>
  main {
    position: absolute;
    inset: 0;
    background: var(--bg-0);
  }
  .failure {
    margin: 24px;
    color: var(--text-muted);
  }
</style>
