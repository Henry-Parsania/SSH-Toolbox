<script lang="ts">
  import { untrack } from "svelte";
  import "./lib/ui/theme.css";
  import {
    layoutGet,
    layoutSet,
    listShells,
    listWslDistros,
    platformInfo,
    windowOpen,
    type PlatformInfo,
    type SavedSession,
    type Shell,
  } from "./lib/backend";
  import PaneTree from "./lib/layout/PaneTree.svelte";
  import TabBar, { type NewTabOption } from "./lib/layout/TabBar.svelte";
  import { encodePayload } from "./lib/layout/detach";
  import { leaves, newLeaf } from "./lib/layout/panes";
  import { shortcutFor } from "./lib/layout/shortcuts";
  import { Workspace } from "./lib/layout/workspace.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import { Library } from "./lib/sessions/library.svelte";
  import HelpDialog from "./lib/ui/HelpDialog.svelte";
  import MenuBar, { type Menu } from "./lib/ui/MenuBar.svelte";
  import DockerPanel from "./lib/docker/DockerPanel.svelte";
  import FilesPanel from "./lib/files/FilesPanel.svelte";
  import Sidebar from "./lib/sessions/Sidebar.svelte";
  import { Reachability } from "./lib/status/reach.svelte";
  import StatusBar from "./lib/status/StatusBar.svelte";
  import { loadSettings, settings, saveSettings } from "./lib/settings.svelte";
  import Icon from "./lib/ui/Icon.svelte";

  let platform = $state<PlatformInfo | null>(null);
  let shells = $state<Shell[]>([]);
  let distros = $state<string[]>([]);
  let showSettings = $state(false);
  let help = $state<"features" | "shortcuts" | "about" | null>(null);
  // Menu commands the sessions list carries out. The number makes a repeated command count.
  let sidebarCommand = $state<{ name: "ssh" | "ftp" | "serial" | "import"; n: number } | null>(null);
  function sessionCommand(name: "ssh" | "ftp" | "serial" | "import") {
    sidebarOpen = true;
    sidebarCommand = { name, n: (sidebarCommand?.n ?? 0) + 1 };
  }
  // Which panels are showing is kept for next time, like their widths.
  const savedOpen = (key: string, fallback: boolean) => {
    const value = localStorage.getItem(key);
    return value === null ? fallback : value === "1";
  };
  let sidebarOpen = $state(savedOpen("sidebar-open", true));
  let filesOpen = $state(savedOpen("files-open", false));
  // Docker lives under the sessions list.
  let dockerOpen = $state(savedOpen("docker-open", false));
  $effect(() => {
    localStorage.setItem("sidebar-open", sidebarOpen ? "1" : "0");
    localStorage.setItem("files-open", filesOpen ? "1" : "0");
    localStorage.setItem("docker-open", dockerOpen ? "1" : "0");
  });
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
  // The Docker section's height can be dragged, and is kept for next time.
  let dockHeight = $state(
    (() => {
      const n = Number(localStorage.getItem("docker-height"));
      return n >= 140 && n <= 2000 ? n : 320;
    })(),
  );
  function resizeDock(e: PointerEvent) {
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const startY = e.clientY;
    const start = dockHeight;
    const move = (ev: PointerEvent) => {
      // Dragging up makes it taller. The sessions list keeps at least 120 px.
      const most = Math.max(140, window.innerHeight - 28 - 120 - 60);
      dockHeight = Math.min(most, Math.max(140, start + (startY - ev.clientY)));
    };
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      localStorage.setItem("docker-height", String(Math.round(dockHeight)));
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  }
  // Counts up each time something asks the sidebar to start a new session.
  let newSessionSignal = $state(0);
  const library = new Library();
  const reach = new Reachability();

  // Check which saved servers answer: at start, whenever the list changes, and then on the
  // interval from the settings (0 means only at start and on changes).
  const listKey = $derived(
    library.file.session
      .map((s) => (s.type === "ssh" ? `${s.id}@${s.host}:${s.port}:${s.jump}` : s.type === "ftp" ? `${s.id}@${s.host}:${s.port}` : ""))
      .join("|"),
  );
  $effect(() => {
    if (!settings.reachability_enabled) {
      reach.clear();
      return;
    }
    void listKey;
    const every = settings.reachability_interval_secs;
    const sessions = $state.snapshot(library.file.session);
    const soon = setTimeout(() => void reach.run(sessions), 300);
    const timer = every > 0 ? setInterval(() => void reach.run(sessions), every * 1000) : undefined;
    return () => {
      clearTimeout(soon);
      if (timer) clearInterval(timer);
    };
  });

  // The footer shows the server of the active SSH pane, and this computer for anything else.
  const footerTarget = $derived.by(() => {
    const tab = workspace.activeTab;
    const leaf = tab ? leaves(tab.root).find((l) => l.pane_id === tab.active) : undefined;
    return leaf?.spec?.type === "ssh"
      ? { kind: "ssh" as const, id: leaf.spec.saved_session_id, name: leaf.title }
      : { kind: "local" as const };
  });

  // An FTP session has no terminal. Opening one shows its files until another pane is chosen.
  let ftpOpened = $state<{ kind: "ftp"; id: string; name: string } | null>(null);

  // The server whose files the panel shows: the one in the active pane, if it is an SSH session.
  const activeSession = $derived.by(() => {
    const tab = workspace.activeTab;
    const leaf = tab ? leaves(tab.root).find((l) => l.pane_id === tab.active) : undefined;
    if (leaf?.spec?.type === "ssh") {
      return { kind: "ssh" as const, id: leaf.spec.saved_session_id, name: leaf.title };
    }
    if (ftpOpened) return ftpOpened;
    if (leaf?.spec?.type === "wsl") return { kind: "wsl" as const, distro: leaf.spec.distro, name: leaf.title };
    return null;
  });

  // The SSH server in the active pane, whose Docker the left panel shows.
  const activeSshSession = $derived.by(() => {
    const tab = workspace.activeTab;
    const leaf = tab ? leaves(tab.root).find((l) => l.pane_id === tab.active) : undefined;
    if (leaf?.spec?.type !== "ssh" || workspace.waiting[leaf.pane_id]) return null;
    return { id: leaf.spec.saved_session_id, name: leaf.title };
  });

  // The backend session of the active pane, so the file panel can send it a folder to go to.
  const activeTerminalSession = $derived.by(() => {
    const tab = workspace.activeTab;
    return tab ? (workspace.sessionIds[tab.active] ?? "") : "";
  });

  // The folder the active pane's shell reported, for the file panel to follow.
  const followedFolder = $derived.by(() => {
    const tab = workspace.activeTab;
    return tab ? (workspace.cwd[tab.active] ?? "") : "";
  });

  // The colour theme applies to the whole window, not only to the terminal.
  $effect(() => {
    document.documentElement.dataset.theme = settings.theme;
  });

  // A pop-up window with the server's process monitor (btop, htop or top) instead of a shell.
  function openMonitor(session: SavedSession) {
    if (session.type !== "ssh") return;
    const title = `${session.name} · top`;
    const pane = newLeaf({ type: "ssh", saved_session_id: session.id, monitor: true }, title);
    void windowOpen(title, encodePayload({ title, root: pane, attach: {}, active: pane.pane_id })).catch(() => {});
  }

  // Moves the active tab into a window of its own. Its programs keep running; the new window
  // starts with an empty screen.
  function detachActiveTab() {
    const tab = workspace.activeTab;
    if (!tab) return;
    const payload = workspace.detachTab(tab.id);
    if (payload) void windowOpen(payload.title, encodePayload(payload)).catch(() => {});
  }

  // Only SSH sessions can be opened so far. Serial and FTP arrive in phases 3 and 4.
  function openSaved(session: SavedSession) {
    if (session.type === "ssh") {
      workspace.newTab({ type: "ssh", saved_session_id: session.id }, session.name);
      // The server's files appear beside the terminal.
      filesOpen = true;
      ftpOpened = null;
    } else if (session.type === "serial") {
      workspace.newTab({ type: "serial", saved_session_id: session.id }, session.name);
    } else if (session.type === "ftp") {
      ftpOpened = { kind: "ftp", id: session.id, name: session.name };
      filesOpen = true;
    }
  }
  // Choosing another tab or pane goes back to following the terminal.
  $effect(() => {
    void workspace.activeTabId;
    void workspace.activeTab?.active;
    untrack(() => (ftpOpened = null));
  });
  let restored = false;
  const options = $derived<NewTabOption[]>([
    ...[...shells]
      .sort((a, b) => Number(b.id === settings.default_shell) - Number(a.id === settings.default_shell))
      .map((s) => ({ label: s.name, spec: { type: "local" as const, shell_id: s.id } })),
    ...distros.map((d) => ({ label: `WSL: ${d}`, spec: { type: "wsl" as const, distro: d } })),
  ]);
  let failure = $state<string | null>(null);

  const workspace = new Workspace();

  const menus = $derived<Menu[]>([
    {
      label: "File",
      items: [
        { label: "New terminal tab", hint: "Ctrl+Shift+T", onselect: () => newTab() },
        "separator",
        { label: "New SSH session…", onselect: () => sessionCommand("ssh") },
        { label: "New FTP session…", onselect: () => sessionCommand("ftp") },
        { label: "New serial session…", onselect: () => sessionCommand("serial") },
        { label: "Import from ~/.ssh/config…", onselect: () => sessionCommand("import") },
        "separator",
        { label: "Settings…", onselect: () => (showSettings = true) },
      ],
    },
    {
      label: "View",
      items: [
        { label: "Sessions list", checked: sidebarOpen, onselect: () => (sidebarOpen = !sidebarOpen) },
        { label: "Files panel", checked: filesOpen, onselect: () => (filesOpen = !filesOpen) },
        {
          label: "Docker panel",
          checked: dockerOpen,
          onselect: () => {
            dockerOpen = !dockerOpen;
            if (dockerOpen) sidebarOpen = true;
          },
        },
        {
          label: "Footer (CPU, memory, disk)",
          checked: settings.stats_enabled,
          onselect: () => void saveSettings({ ...settings, stats_enabled: !settings.stats_enabled }).catch(() => {}),
        },
        "separator",
        { label: "Next tab", hint: "Ctrl+Tab", onselect: () => workspace.stepTab(1) },
        { label: "Previous tab", hint: "Ctrl+Shift+Tab", onselect: () => workspace.stepTab(-1) },
      ],
    },
    {
      label: "Terminal",
      items: [
        { label: "Split side by side", hint: "Ctrl+Shift+E", onselect: () => workspace.split("row") },
        { label: "Split top and bottom", hint: "Ctrl+Shift+O", onselect: () => workspace.split("column") },
        { label: "Close pane", hint: "Ctrl+Shift+W", onselect: () => workspace.closeActivePane() },
        { label: "Move this tab to its own window", disabled: workspace.tabs.length === 0, onselect: detachActiveTab },
        "separator",
        { label: "Search the output", hint: "Ctrl+Shift+F", onselect: () => workspace.requestSearch() },
        { label: "Save the output to Downloads", hint: "Ctrl+Shift+S", onselect: () => workspace.requestSave() },
        {
          label: "Type into all panes of the tab",
          hint: "Ctrl+Shift+B",
          checked: workspace.broadcast,
          onselect: () => (workspace.broadcast = !workspace.broadcast),
        },
      ],
    },
    {
      label: "Help",
      items: [
        { label: "What it can do", onselect: () => (help = "features") },
        { label: "Keyboard shortcuts", onselect: () => (help = "shortcuts") },
        "separator",
        { label: "About SSH Toolbox", onselect: () => (help = "about") },
      ],
    },
  ]);

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
    <MenuBar {menus} />
    <div class="body">
      {#if sidebarOpen}
        <div class="side column" style:width="{sidebarWidth}px">
          <div class="top-part"><Sidebar {library} onopen={openSaved} onmonitor={openMonitor} {newSessionSignal} command={sidebarCommand} {reach} /></div>
          {#if dockerOpen}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="grip-h" onpointerdown={resizeDock}></div>
            <div class="dock" style:height="{dockHeight}px">
              <DockerPanel
                {library}
                active={activeSshSession}
                onopen={(spec, title) => workspace.newTab(spec, title)}
                onclose={() => (dockerOpen = false)}
              />
            </div>
          {/if}
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="grip" onpointerdown={(e) => resizeSide(e, "sidebar")}></div>
      {/if}
      <div class="center">
      <TabBar
      {workspace}
      {options}
      {sidebarOpen}
      onnew={newTab}
      onsettings={() => (showSettings = true)}
      onsidebar={() => (sidebarOpen = !sidebarOpen)}
      {filesOpen}
      onfiles={() => (filesOpen = !filesOpen)}
      {dockerOpen}
      ondocker={() => {
        dockerOpen = !dockerOpen;
        // Docker lives under the sessions list, so that has to be showing.
        if (dockerOpen) sidebarOpen = true;
      }}
    />
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
              <div><dt><span class="kbd">Ctrl</span> <span class="kbd">Shift</span> <span class="kbd">S</span></dt><dd>Save the output to Downloads</dd></div>
            </dl>
          </div>
        {/if}
      </main>
      {#if settings.stats_enabled}
        <StatusBar target={footerTarget} />
      {/if}
      </div>
      {#if filesOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="grip" onpointerdown={(e) => resizeSide(e, "files")}></div>
        <div class="side" style:width="{filesWidth}px">
          <FilesPanel
            source={activeSession}
            follow={followedFolder}
            terminalSession={activeTerminalSession}
            onclose={() => (filesOpen = false)} />
        </div>
      {/if}
    </div>
  </div>
  {#if help}
    <HelpDialog page={help} onclose={() => (help = null)} />
  {/if}
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
  .side.column {
    flex-direction: column;
  }
  .top-part {
    display: flex;
    flex: 1;
    min-height: 120px;
    min-width: 0;
  }
  .dock {
    display: flex;
    flex: none;
    min-height: 140px;
    min-width: 0;
  }
  /* A thin strip between the sessions and Docker that can be dragged up and down. */
  .grip-h {
    flex: none;
    height: 7px;
    margin: -3px 0;
    z-index: 6;
    cursor: row-resize;
  }
  .grip-h:hover {
    background: var(--accent);
    opacity: 0.4;
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
  .center {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
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
