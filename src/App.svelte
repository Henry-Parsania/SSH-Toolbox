<script lang="ts">
  import { layoutGet, layoutSet, type SavedSession, listShells, listWslDistros, platformInfo, type PlatformInfo, type Shell } from "./lib/backend";
  import PaneTree from "./lib/layout/PaneTree.svelte";
  import TabBar, { type NewTabOption } from "./lib/layout/TabBar.svelte";
  import { shortcutFor } from "./lib/layout/shortcuts";
  import { Workspace } from "./lib/layout/workspace.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import { Library } from "./lib/sessions/library.svelte";
  import Sidebar from "./lib/sessions/Sidebar.svelte";
  import { loadSettings, settings } from "./lib/settings.svelte";

  let platform = $state<PlatformInfo | null>(null);
  let shells = $state<Shell[]>([]);
  let distros = $state<string[]>([]);
  let showSettings = $state(false);
  let sidebarOpen = $state(true);
  const library = new Library();

  // Only SSH sessions can be opened so far. Serial and FTP arrive in phases 3 and 4.
  function openSaved(session: SavedSession) {
    if (session.type === "ssh") {
      workspace.newTab({ type: "ssh", saved_session_id: session.id }, session.name);
    }
  }
  let restored = false;
  const options = $derived<NewTabOption[]>([
    ...[...shells]
      .sort((a, b) => Number(b.id === settings.default_shell) - Number(a.id === settings.default_shell))
      .map((s) => ({ label: s.name, spec: { type: "local" as const, shell_id: s.id } })),
    ...distros.map((d) => ({ label: `WSL: ${d}`, spec: { type: "wsl" as const, distro: d } })),
  ]);
  let failure = $state<string | null>(null);
  const workspace = new Workspace();

  $effect(() => {
    Promise.all([platformInfo(), listShells(), loadSettings().catch(() => {})]).then(
      async ([info, found]) => {
        if (found.length === 0) {
          failure = "No shell was found on this computer.";
          return;
        }
        platform = info;
        shells = found;
        // Bring back the last layout if asked to. Its panes start fresh sessions.
        const saved = settings.restore_layout ? await layoutGet().catch(() => null) : null;
        if (saved) workspace.restore(saved);
        else newTab();
        restored = true;
        void library.load().catch(() => {});
        // The WSL entries are left out when WSL isn't there.
        if (info.wsl_available) listWslDistros().then((d) => (distros = d), () => {});
      },
      (err: { message?: string }) => (failure = err.message ?? "Couldn't start."),
    );
  });

  // Without a choice, a new tab uses the first shell.
  function newTab(option: NewTabOption | undefined = options[0]) {
    if (option) workspace.newTab(option.spec, option.label);
  }

  // Save the layout shortly after it changes, but not before the old one has been read.
  $effect(() => {
    const layout = workspace.toLayout();
    if (!restored) return;
    const timer = setTimeout(() => void layoutSet(layout).catch(() => {}), 500);
    return () => clearTimeout(timer);
  });

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
        case "search":
          workspace.requestSearch();
          break;
        case "broadcast":
          workspace.broadcast = !workspace.broadcast;
          break;
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
    <TabBar
      {workspace}
      {options}
      onnew={newTab}
      onsettings={() => (showSettings = true)}
      onsidebar={() => (sidebarOpen = !sidebarOpen)}
    />
    <div class="body">
      {#if sidebarOpen}
        <Sidebar {library} onopen={openSaved} />
      {/if}
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
  </div>
  {#if showSettings}
    <SettingsDialog {platform} {shells} onclose={() => (showSettings = false)} />
  {/if}
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
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
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
