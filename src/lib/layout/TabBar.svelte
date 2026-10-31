<script lang="ts">
  import type { SessionSpec } from "../backend";
  import type { Workspace } from "./workspace.svelte";

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
  }

  let { workspace, options, onnew, onsettings }: Props = $props();
  let menuOpen = $state(false);

  let editing = $state<string | null>(null);
  let draft = $state("");
  let strip: HTMLDivElement | undefined = $state();
  let dragging = $state<string | null>(null);

  function startRename(id: string, title: string) {
    editing = id;
    draft = title;
  }

  function finishRename(save: boolean) {
    if (editing && save) workspace.renameTab(editing, draft);
    editing = null;
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

<div class="bar">
  <div class="tabs" bind:this={strip}>
    {#each workspace.tabs as tab (tab.id)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="tab"
        class:active={tab.id === workspace.activeTabId}
        class:dragging={tab.id === dragging}
        data-tab={tab.id}
        role="tab"
        tabindex="-1"
        aria-selected={tab.id === workspace.activeTabId}
        onpointerdown={(e) => {
          workspace.selectTab(tab.id);
          startDrag(e, tab.id);
        }}
        ondblclick={() => startRename(tab.id, tab.title)}
        onauxclick={(e) => {
          if (e.button === 1) workspace.closeTab(tab.id);
        }}
      >
        {#if editing === tab.id}
          <!-- svelte-ignore a11y_autofocus -->
          <input
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
          onpointerdown={(e) => e.stopPropagation()}
          onclick={() => workspace.closeTab(tab.id)}>×</button
        >
      </div>
    {/each}
  </div>
  <button class="new" aria-label="New tab" title="New tab (Ctrl+Shift+T)" onclick={() => onnew()}>+</button>
  <div class="menu-wrap">
    <button class="new" aria-label="Choose a shell" onclick={() => (menuOpen = !menuOpen)}>▾</button>
    {#if menuOpen}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="backdrop" onpointerdown={() => (menuOpen = false)}></div>
      <ul class="menu">
        {#each options as option (option.label)}
          <li>
            <button
              onclick={() => {
                menuOpen = false;
                onnew(option);
              }}>{option.label}</button
            >
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  <div class="spacer"></div>
  <button class="new" aria-label="Settings" title="Settings" onclick={onsettings}>⚙</button>
</div>

<style>
  .bar {
    display: flex;
    align-items: stretch;
    background: #252526;
    font: 13px system-ui, sans-serif;
    user-select: none;
  }
  .tabs {
    display: flex;
    min-width: 0;
    overflow-x: auto;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px 6px 12px;
    color: #9d9d9d;
    background: #2d2d2d;
    border-right: 1px solid #1e1e1e;
    white-space: nowrap;
  }
  .tab.active {
    color: #fff;
    background: #1e1e1e;
  }
  .tab.dragging {
    opacity: 0.7;
  }
  .title {
    max-width: 16em;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  input {
    width: 10em;
    font: inherit;
    background: #3c3c3c;
    color: #fff;
    border: 1px solid #007acc;
  }
  button {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    font-size: 15px;
    line-height: 1;
    padding: 2px 6px;
    border-radius: 3px;
  }
  button:hover {
    background: #4448;
  }
  .new {
    color: #ccc;
    padding: 0 12px;
  }
  .spacer {
    flex: 1;
  }
  .menu-wrap {
    position: relative;
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
  }
  .menu {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 11;
    margin: 0;
    padding: 4px 0;
    list-style: none;
    min-width: 12em;
    background: #252526;
    border: 1px solid #555;
    box-shadow: 0 4px 12px #0008;
  }
  .menu button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 6px 14px;
    font-size: 13px;
    border-radius: 0;
    color: #d4d4d4;
  }
</style>
