<script lang="ts">
  import type { SessionSpec } from "../backend";
  import { fitMenu } from "../ui/fitMenu";
  import Icon, { type IconName } from "../ui/Icon.svelte";
  import { leaves } from "./panes";
  import { readableOn, sessionColors } from "../sessions/colors.svelte";
  import type { Tab, Workspace } from "./workspace.svelte";

  export interface NewTabOption {
    label: string;
    spec: SessionSpec;
  }

  interface Props {
    workspace: Workspace;
    /** What the menu offers: local shells, and WSL distributions when WSL is there. */
    options: NewTabOption[];
    onnew: (option?: NewTabOption) => void;
    onsettings: () => void;
    onsidebar: () => void;
    sidebarOpen: boolean;
    filesOpen: boolean;
    onfiles: () => void;
    dockerOpen: boolean;
    ondocker: () => void;
    agentOpen: boolean;
    onagent: () => void;
    /** Moves a tab into a window of its own. */
    ondetach: (tabId: string) => void;
  }

  let { workspace, options, onnew, onsettings, onsidebar, sidebarOpen, filesOpen, onfiles, dockerOpen, ondocker, agentOpen, onagent, ondetach }: Props = $props();
  let menuOpen = $state(false);
  // The right-click menu on a tab.
  let tabMenu = $state<{ x: number; y: number; id: string } | null>(null);

  /** Runs an action from the tab menu, then closes it. The action goes first because it reads the menu. */
  function act(action: () => void) {
    action();
    tabMenu = null;
  }

  let editing = $state<string | null>(null);
  let draft = $state("");
  let strip: HTMLDivElement | undefined = $state();
  let dragging = $state<string | null>(null);

  /** The colour chosen for the SSH session a tab was opened from, if any. */
  function colorOf(tab: Tab): string | undefined {
    const spec = leaves(tab.root)[0]?.spec;
    return spec?.type === "ssh" ? sessionColors.get(spec.saved_session_id) : undefined;
  }

  function startRename(id: string, title: string) {
    editing = id;
    draft = title;
  }

  function finishRename(save: boolean) {
    if (editing && save) workspace.renameTab(editing, draft);
    editing = null;
  }

  /** The icon says what kind of session the tab holds. */
  function iconFor(tab: Tab): IconName {
    const spec = leaves(tab.root)[0]?.spec;
    return spec?.type === "ssh" ? "server" : spec?.type === "serial" ? "serial" : "terminal";
  }

  // Reordering uses pointer events. HTML drag and drop is switched off by Tauri's file drop.
  function startDrag(e: PointerEvent, id: string) {
    if (e.button !== 0 || editing || !strip) return;
    const startX = e.clientX;
    let moved = false;
    const bar = strip;

    const move = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientX - startX) < 5) return;
      moved = true;
      dragging = id;
      const tabs = Array.from(bar.querySelectorAll<HTMLElement>("[data-tab]"));
      const from = workspace.tabs.findIndex((t) => t.id === id);
      let to = tabs.findIndex((el) => {
        const r = el.getBoundingClientRect();
        return ev.clientX >= r.left && ev.clientX < r.right;
      });
      if (to < 0) to = ev.clientX < tabs[0].getBoundingClientRect().left ? 0 : tabs.length - 1;
      workspace.moveTab(from, to);
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      dragging = null;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  }
</script>

<svelte:window onpointerdown={() => (tabMenu = null)} onblur={() => (tabMenu = null)} onkeydown={(e) => e.key === "Escape" && (tabMenu = null)} />

{#if tabMenu}
  {@const target = tabMenu}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <ul class="menu ctx" role="menu" use:fitMenu={{ x: target.x, y: target.y }} onpointerdown={(e) => e.stopPropagation()} oncontextmenu={(e) => e.preventDefault()}>
    <li><button onclick={() => act(() => workspace.duplicateTab(target.id, "row"))}><Icon name="split-h" size={14} /> Duplicate to the right <span class="hintkey">(side by side)</span></button></li>
    <li><button onclick={() => act(() => workspace.duplicateTab(target.id, "column"))}><Icon name="split-v" size={14} /> Duplicate below <span class="hintkey">(top and bottom)</span></button></li>
    <li class="sep" role="separator"></li>
    <li><button onclick={() => act(() => { const t = workspace.tabs.find((x) => x.id === target.id); if (t) startRename(t.id, t.title); })}><Icon name="edit" size={14} /> Rename</button></li>
    <li><button onclick={() => act(() => ondetach(target.id))}><Icon name="up" size={14} /> Move to its own window</button></li>
    <li class="sep" role="separator"></li>
    <li><button onclick={() => act(() => workspace.closeTab(target.id))}><Icon name="close" size={14} /> Close tab</button></li>
  </ul>
{/if}

<div class="bar">
  <button
    class="icon-btn lead"
    class:on={sidebarOpen}
    aria-label="Show or hide saved sessions"
    title="Saved sessions"
    onclick={onsidebar}
  >
    <Icon name="sidebar" size={17} />
  </button>

  <div class="tabs" bind:this={strip} role="tablist">
    {#each workspace.tabs as tab (tab.id)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="tab"
        class:active={tab.id === workspace.activeTabId}
        class:dragging={tab.id === dragging}
        class:elsewhere={tab.id !== workspace.activeTabId && workspace.groupOf(tab.id) !== null}
        class:colored={!!colorOf(tab)}
        style={colorOf(tab) ? `--tab-color: ${colorOf(tab)}; --tab-text: ${readableOn(colorOf(tab)!)}` : undefined}
        data-tab={tab.id}
        role="tab"
        tabindex="-1"
        aria-selected={tab.id === workspace.activeTabId}
        onpointerdown={(e) => {
          // Only the main button chooses the tab. The right button just opens the menu.
          if (e.button !== 0) return;
          workspace.selectTab(tab.id);
          startDrag(e, tab.id);
        }}
        oncontextmenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          tabMenu = { x: e.clientX, y: e.clientY, id: tab.id };
        }}
        ondblclick={() => startRename(tab.id, tab.title)}
        onauxclick={(e) => {
          if (e.button === 1) workspace.closeTab(tab.id);
        }}
      >
        <span class="kind"><Icon name={iconFor(tab)} size={14} /></span>
        {#if tab.id !== workspace.activeTabId && workspace.groupOf(tab.id) !== null}
          <span class="shown" title="Showing in another pane"></span>
        {/if}
        {#if workspace.finished[tab.id]}
          <span class="done" title="Output has stopped: a long command may have finished">✓</span>
        {/if}
        {#if editing === tab.id}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="rename"
            autofocus
            bind:value={draft}
            onfocus={(e) => e.currentTarget.select()}
            onblur={() => finishRename(true)}
            onkeydown={(e) => {
              if (e.key === "Enter") finishRename(true);
              else if (e.key === "Escape") finishRename(false);
              e.stopPropagation();
            }}
            onpointerdown={(e) => e.stopPropagation()}
          />
        {:else}
          <span class="title">{tab.title}</span>
        {/if}
        <button
          class="close"
          aria-label="Close tab"
          title="Close tab"
          onpointerdown={(e) => e.stopPropagation()}
          onclick={() => workspace.closeTab(tab.id)}
        >
          <Icon name="close" size={12} />
        </button>
      </div>
    {/each}
  </div>

  <div class="adders">
    <button class="icon-btn" aria-label="New tab" title="New tab (Ctrl+Shift+T)" onclick={() => onnew()}>
      <Icon name="plus" size={16} />
    </button>
    <div class="menu-wrap">
      <button
        class="icon-btn small"
        aria-label="Choose a shell"
        title="Choose a shell"
        onclick={() => (menuOpen = !menuOpen)}
      >
        <Icon name="chevron-down" size={14} />
      </button>
      {#if menuOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="backdrop" onpointerdown={() => (menuOpen = false)}></div>
        <ul class="menu drop">
          {#each options as option (option.label)}
            <li>
              <button
                onclick={() => {
                  menuOpen = false;
                  onnew(option);
                }}
              >
                <Icon name="terminal" size={14} />
                {option.label}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>

  <div class="spacer">
    {#if workspace.broadcast}
      <span class="badge" title="Ctrl+Shift+B to stop">
        <Icon name="broadcast" size={13} /> Typing into all panes
      </span>
    {/if}
  </div>

  <div class="tools">
    <button
      class="icon-btn"
      aria-label="Split"
      title="Split the window into another pane (Ctrl+Shift+E). Then click the pane and choose what it shows."
      onclick={() => workspace.split("row")}
    >
      <Icon name="split-h" size={16} />
    </button>
    <button
      class="icon-btn"
      class:on={filesOpen}
      aria-label="Files"
      title="Files: browse and transfer"
      onclick={onfiles}
    >
      <Icon name="folder" size={16} />
    </button>
    <button
      class="icon-btn"
      class:on={dockerOpen}
      aria-label="Docker"
      title="Docker: containers, logs and shells"
      onclick={ondocker}
    >
      <Icon name="box" size={16} />
    </button>
    <button
      class="icon-btn"
      class:on={agentOpen}
      aria-label="Agent"
      title="Agent: an assistant that can look at the server and help"
      onclick={onagent}
    >
      <Icon name="sparkle" size={16} />
    </button>
    <span class="sep"></span>
    <button class="icon-btn" aria-label="Settings" title="Settings" onclick={onsettings}>
      <Icon name="settings" size={17} />
    </button>
  </div>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 42px;
    padding: 0 8px 0 6px;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    user-select: none;
  }
  .lead.on,
  .tools .on {
    color: var(--accent);
  }
  .tabs {
    display: flex;
    align-items: flex-end;
    align-self: stretch;
    gap: 2px;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    padding-top: 6px;
    margin-left: 4px;
  }
  .tabs::-webkit-scrollbar {
    height: 0;
  }
  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 7px;
    max-width: 220px;
    height: 100%;
    padding: 0 6px 0 11px;
    color: var(--text-muted);
    border-radius: var(--radius) var(--radius) 0 0;
    white-space: nowrap;
    cursor: default;
    transition:
      background 0.12s var(--ease),
      color 0.12s var(--ease);
  }
  .tab:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .tab.active {
    color: var(--text);
    background: var(--bg-0);
    box-shadow:
      inset 0 2px 0 var(--accent),
      -1px 0 0 var(--border),
      1px 0 0 var(--border);
  }
  /* Joins the active tab to the terminal below it. */
  .tab.active::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 2px;
    background: var(--bg-0);
  }
  /* A session's own colour replaces the theme's look for its tab. */
  .tab.colored {
    background: var(--tab-color);
    color: var(--tab-text);
    opacity: 0.8;
  }
  .tab.colored.active {
    opacity: 1;
    background: var(--tab-color);
    box-shadow:
      inset 0 2px 0 var(--tab-text),
      -1px 0 0 var(--border),
      1px 0 0 var(--border);
  }
  .tab.colored.active::after {
    background: var(--tab-color);
  }
  .tab.colored .kind,
  .tab.colored.active .kind {
    color: var(--tab-text);
  }
  .tab.dragging {
    opacity: 0.65;
  }
  .kind {
    display: grid;
    color: var(--text-faint);
  }
  .tab.active .kind {
    color: var(--accent);
  }
  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 500;
  }
  .rename {
    width: 9em;
    padding: 2px 6px;
  }
  .close {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    color: var(--text-faint);
    background: none;
    border: none;
    border-radius: 5px;
    opacity: 0;
    cursor: pointer;
    transition: opacity 0.1s;
  }
  .tab:hover .close,
  .tab.active .close {
    opacity: 1;
  }
  .close:hover {
    color: var(--text);
    background: var(--bg-active);
  }
  .adders {
    display: flex;
    align-items: center;
    gap: 0;
    margin-left: 2px;
  }
  .spacer {
    flex: 1;
    display: flex;
    justify-content: center;
    min-width: 0;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    color: var(--warning);
    background: var(--warning-soft);
    border: 1px solid #f0b45a44;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 500;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .sep {
    width: 1px;
    height: 18px;
    margin: 0 4px;
    background: var(--border-strong);
  }
  .menu-wrap {
    position: relative;
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
  }
  .drop {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 11;
  }
  .ctx {
    position: fixed;
    z-index: 60;
    min-width: 230px;
  }
  .ctx li {
    list-style: none;
  }
  .ctx .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
  .hintkey {
    color: var(--text-faint);
    font-size: 11px;
  }
  .done {
    color: var(--accent);
    font-size: 12px;
    font-weight: 700;
  }
  .shown {
    flex: none;
    width: 6px;
    height: 6px;
    background: var(--accent);
    border-radius: 50%;
    opacity: 0.8;
  }
</style>
