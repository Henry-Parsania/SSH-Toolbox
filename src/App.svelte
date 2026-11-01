<script lang="ts">
  import "./lib/ui/theme.css";
  import {
    layoutGet,
    layoutSet,
    listShells,
    listWslDistros,
    platformInfo,
    type PlatformInfo,
    type SavedSession,
    type Shell,
  } from "./lib/backend";
  import PaneTree from "./lib/layout/PaneTree.svelte";
  import TabBar, { type NewTabOption } from "./lib/layout/TabBar.svelte";
  import { leaves } from "./lib/layout/panes";
  import { shortcutFor } from "./lib/layout/shortcuts";
  import { Workspace } from "./lib/layout/workspace.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import { Library } from "./lib/sessions/library.svelte";
  import FilesPanel from "./lib/files/FilesPanel.svelte";
  import Sidebar from "./lib/sessions/Sidebar.svelte";
  import { loadSettings, settings } from "./lib/settings.svelte";
  import Icon from "./lib/ui/Icon.svelte";

  let platform = $state<PlatformInfo | null>(null);
  let shells = $state<Shell[]>([]);
  let distros = $state<string[]>([]);
  let showSettings = $state(false);
  let sidebarOpen = $state(true);
  let filesOpen = $state(false);
  // Widths of the two side panels, which can be dragged. They are kept for next time.
  const saved = (key: string, fallback: number) => {
    const n = Number(localStorage.getItem(key));
    return n >= 180 && n <= 1200 ? n : fallback;
  };
  let sidebarWidth = $state(saved("sidebar-width", 264));
  let filesWidth = $state(saved("files-width", 460));

  function resizeSide(e: PointerEvent, side: "sidebar" | "files") {
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const start = e.clientX;
    const from = side === "sidebar" ? sidebarWidth : filesWidth;
    const move = (ev: PointerEvent) => {
      const delta = ev.clientX - start;
      const wanted = Math.round(side === "sidebar" ? from + delta : from - delta);
      const width = Math.min(Math.max(wanted, 200), Math.max(240, window.innerWidth - 360));
      if (side === "sidebar") sidebarWidth = width;
      else filesWidth = width;
    };
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      localStorage.setItem(side === "sidebar" ? "sidebar-width" : "files-width", String(side === "sidebar" ? sidebarWidth : filesWidth));
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  }
  // Counts up each time something asks the sidebar to start a new session.
  let newSessionSignal = $state(0);
  const library = new Library();

  // The server whose files the panel shows: the one in the active pane, if it is an SSH session.
  const activeSession = $derived.by(() => {
    const tab = workspace.activeTab;
    const leaf = tab ? leaves(tab.root).find((l) => l.pane_id === tab.active) : undefined;
    if (leaf?.spec?.type === "ssh") {
      return { kind: "ssh" as const, id: leaf.spec.saved_session_id, name: leaf.title };
    }
    if (leaf?.spec?.type === "wsl") return { kind: "wsl" as const, distro: leaf.spec.distro, name: leaf.title };
    return null;
  });

  // The colour theme applies to the whole window, not only to the terminal.
  $effect(() => {
    document.documentElement.dataset.theme = settings.theme;
  });

  // Only SSH sessions can be opened so far. Serial and FTP arrive in phases 3 and 4.
  function openSaved(session: SavedSession) {
    if (session.type === "ssh") {
      workspace.newTab({ type: "ssh", saved_session_id: session.id }, session.name);
      // The server's files appear beside the terminal.
      filesOpen = true;
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

  function newSession() {
    sidebarOpen = true;
    newSessionSignal++;
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
  <div class="failure">
    <div class="card">
      <h2>SSH Toolbox couldn't start</h2>
      <p>{failure}</p>
    </div>
  </div>
{:else if platform}
  <div class="app">
    <TabBar
      {workspace}
      {options}
      {sidebarOpen}
      onnew={newTab}
      onsettings={() => (showSettings = true)}
      onsidebar={() => (sidebarOpen = !sidebarOpen)}
      {filesOpen}
      onfiles={() => (filesOpen = !filesOpen)}
    />
    <div class="body">
      {#if sidebarOpen}
        <div class="side" style:width="{sidebarWidth}px">
          <Sidebar {library} onopen={openSaved} {newSessionSignal} />
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="grip" onpointerdown={(e) => resizeSide(e, "sidebar")}></div>
      {/if}
      <main class="tabs">
        {#each workspace.tabs as tab (tab.id)}
          <!-- Hidden tabs stay mounted, so their terminals keep running. -->
          <div class="tab-body" class:hidden={tab.id !== workspace.activeTabId}>
            <PaneTree
              node={tab.root}
              tabId={tab.id}
              activePane={tab.active}
              visible={tab.id === workspace.activeTabId}
              crowded={workspace.paneCount(tab) > 1}
              {platform}
              {workspace}
            />
          </div>
        {/each}

        {#if workspace.tabs.length === 0}
          <div class="welcome">
            <div class="mark"><Icon name="terminal" size={34} /></div>
            <h1>SSH Toolbox</h1>
            <p class="sub">Terminals, SSH and files in one place.</p>
            <div class="actions">
              <button class="btn primary big" onclick={() => newTab()}>
                <Icon name="terminal" size={16} /> New terminal
              </button>
              <button class="btn big" onclick={newSession}>
                <Icon name="server" size={16} /> Add an SSH session
              </button>
            </div>
            <dl class="shortcuts">
              <div><dt><span class="kbd">Ctrl</span> <span class="kbd">Shift</span> <span class="kbd">T</span></dt><dd>New tab</dd></div>
              <div><dt><span class="kbd">Ctrl</span> <span class="kbd">Shift</span> <span class="kbd">E</span></dt><dd>Split side by side</dd></div>
              <div><dt><span class="kbd">Ctrl</span> <span class="kbd">Shift</span> <span class="kbd">F</span></dt><dd>Search the output</dd></div>
            </dl>
          </div>
        {/if}
      </main>
      {#if filesOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="grip" onpointerdown={(e) => resizeSide(e, "files")}></div>
        <div class="side" style:width="{filesWidth}px">
          <FilesPanel source={activeSession} onclose={() => (filesOpen = false)} />
        </div>
      {/if}
    </div>
  </div>
  {#if showSettings}
    <SettingsDialog {platform} {shells} onclose={() => (showSettings = false)} />
  {/if}
{/if}

<style>
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
  .side {
    display: flex;
    flex: none;
    min-width: 0;
    min-height: 0;
  }
  /* A thin strip between a side panel and the terminals that can be dragged. */
  .grip {
    flex: none;
    width: 7px;
    margin: 0 -3px;
    z-index: 6;
    cursor: col-resize;
    touch-action: none;
    background: transparent;
    transition: background 0.12s var(--ease);
  }
  .grip:hover {
    background: var(--accent-soft);
  }
  .tabs {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    background: var(--bg-0);
  }
  .tab-body {
    position: absolute;
    inset: 0;
  }
  .tab-body.hidden {
    display: none;
  }

  .welcome {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 24px;
    text-align: center;
  }
  .mark {
    display: grid;
    place-items: center;
    width: 76px;
    height: 76px;
    margin-bottom: 18px;
    color: var(--on-accent);
    background: linear-gradient(135deg, var(--accent), #8a7bff);
    border-radius: 22px;
    box-shadow: 0 10px 30px #6aa2ff33;
  }
  h1 {
    margin: 0;
    font-size: 26px;
    font-weight: 650;
    letter-spacing: -0.02em;
  }
  .sub {
    margin: 6px 0 24px;
    color: var(--text-muted);
    font-size: 14px;
  }
  .actions {
    display: flex;
    gap: 10px;
  }
  .big {
    padding: 9px 18px;
    font-size: 14px;
  }
  .shortcuts {
    display: grid;
    gap: 8px;
    margin: 34px 0 0;
    color: var(--text-muted);
  }
  .shortcuts div {
    display: flex;
    justify-content: space-between;
    gap: 28px;
    min-width: 320px;
  }
  .shortcuts dt {
    display: flex;
    gap: 4px;
  }
  .shortcuts dd {
    margin: 0;
  }

  .failure {
    display: grid;
    place-items: center;
    height: 100%;
    padding: 24px;
  }
  .card {
    max-width: 460px;
    padding: 24px 28px;
    background: var(--bg-1);
    border: 1px solid var(--border-strong);
    border-left: 3px solid var(--danger);
    border-radius: var(--radius-l);
  }
  .card h2 {
    margin: 0 0 8px;
  }
  .card p {
    margin: 0;
    color: var(--text-muted);
  }
</style>
