<script lang="ts">
  import { listShells, platformInfo, type PlatformInfo, type Shell } from "./lib/backend";
  import PaneTree from "./lib/layout/PaneTree.svelte";
  import TabBar from "./lib/layout/TabBar.svelte";
  import { shortcutFor } from "./lib/layout/shortcuts";
  import { Workspace } from "./lib/layout/workspace.svelte";

  let platform = $state<PlatformInfo | null>(null);
  let shells = $state<Shell[]>([]);
  let failure = $state<string | null>(null);
  const workspace = new Workspace();

  $effect(() => {
    Promise.all([platformInfo(), listShells()]).then(
      ([info, found]) => {
        if (found.length === 0) {
          failure = "No shell was found on this computer.";
          return;
        }
        platform = info;
        shells = found;
        newTab();
      },
      (err: { message?: string }) => (failure = err.message ?? "Couldn't start."),
    );
  });

  // Until the shell menu arrives (task 1.11), new tabs use the first shell.
  function newTab() {
    const shell = shells[0];
    if (shell) workspace.newTab({ type: "local", shell_id: shell.id }, shell.name);
  }

  // Runs before the terminal sees the key, so shortcuts never reach the program in the pane.
  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      const action = shortcutFor(e);
      if (!action) return;
      e.preventDefault();
      e.stopPropagation();
      switch (action.type) {
        case "new-tab":
          newTab();
          break;
        case "close-pane":
          workspace.closeActivePane();
          break;
        case "next-tab":
          workspace.stepTab(1);
          break;
        case "previous-tab":
          workspace.stepTab(-1);
          break;
        case "split":
          workspace.split(action.direction);
          break;
        case "move-focus":
          workspace.moveFocus(action.move);
          break;
        // Search and typing into all panes arrive in tasks 1.13 and 1.15.
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });
</script>

{#if failure}
  <p class="error">{failure}</p>
{:else if platform}
  <div class="app">
    <TabBar {workspace} onnew={newTab} />
    <div class="tabs">
      {#each workspace.tabs as tab (tab.id)}
        <!-- Hidden tabs stay mounted, so their terminals keep running. -->
        <div class="tab-body" class:hidden={tab.id !== workspace.activeTabId}>
          <PaneTree
            node={tab.root}
            tabId={tab.id}
            activePane={tab.active}
            visible={tab.id === workspace.activeTabId}
            {platform}
            {workspace}
          />
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  :global(html, body, #app) {
    height: 100%;
    margin: 0;
    background: #1e1e1e;
    overflow: hidden;
  }
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .tabs {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .tab-body {
    position: absolute;
    inset: 0;
  }
  .tab-body.hidden {
    display: none;
  }
  .error {
    color: #f48771;
    font-family: system-ui, sans-serif;
    padding: 2rem;
  }
</style>
