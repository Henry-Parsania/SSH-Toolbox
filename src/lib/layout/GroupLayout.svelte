<script lang="ts">
  // The window's panes. Each pane shows one tab, chosen from the tab bar: click a pane, then click
  // a tab. Every tab is drawn once, in a flat list, and moved to the place of the pane that shows
  // it, so switching or splitting never restarts a running terminal.

  import type { Snippet } from "svelte";
  import type { PlatformInfo } from "../backend";
  import Icon from "../ui/Icon.svelte";
  import PaneTree from "./PaneTree.svelte";
  import { placeAll, type DividerPlace } from "./panes";
  import type { Tab, Workspace } from "./workspace.svelte";

  interface Props {
    workspace: Workspace;
    platform: PlatformInfo;
    /** Draws a tab that is a panel (files, Docker, the agent). */
    view: Snippet<[Tab]>;
    /** Starts a new terminal in a pane. */
    onnew: () => void;
  }

  let { workspace, platform, view, onnew }: Props = $props();

  const placed = $derived(placeAll(workspace.groups));
  const rects = $derived(new Map(placed.panes.map((p) => [p.leaf.pane_id, p.rect])));
  const crowded = $derived(placed.panes.length > 1);
  const HEADER = 26;

  const pct = (share: number) => `${share * 100}%`;
  const tabOf = (groupId: string) => workspace.tabs.find((t) => t.id === workspace.groupTab[groupId]);

  let stage: HTMLDivElement | undefined = $state();
  let draggingDivider = $state<DividerPlace | null>(null);

  function startDrag(e: PointerEvent, divider: DividerPlace) {
    if (!stage) return;
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const horizontal = divider.direction === "row";
    const { left, right, top, bottom } = divider.split;
    const total = horizontal ? (right - left) * stage.clientWidth : (bottom - top) * stage.clientHeight;
    let last = horizontal ? e.clientX : e.clientY;
    draggingDivider = divider;
    const move = (ev: PointerEvent) => {
      const now = horizontal ? ev.clientX : ev.clientY;
      if (total > 0) workspace.resizeGroupDivider(divider.path, divider.index, (now - last) / total);
      last = now;
    };
    const stop = () => {
      draggingDivider = null;
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  }
</script>

<div class="stage" bind:this={stage}>
  <!-- The panes: a thin title strip and, when nothing is chosen yet, a hint. -->
  {#each placed.panes as { leaf, rect } (leaf.pane_id)}
    {@const shown = tabOf(leaf.pane_id)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="slot"
      class:active={crowded && leaf.pane_id === workspace.activeGroup}
      style:left={pct(rect.left)}
      style:top={pct(rect.top)}
      style:width={pct(rect.right - rect.left)}
      style:height={pct(rect.bottom - rect.top)}
      onpointerdown={() => workspace.focusGroup(leaf.pane_id)}
    >
      {#if crowded}
        <header class="strip" style:height="{HEADER}px">
          <span class="which">{shown ? shown.title : "Empty pane"}</span>
          <button
            class="icon-btn small"
            aria-label="Close this pane"
            title="Close this pane (the tab stays open in the tab bar)"
            onclick={(e) => {
              e.stopPropagation();
              workspace.closeGroup(leaf.pane_id);
            }}
          >
            <Icon name="close" size={13} />
          </button>
        </header>
      {/if}
      {#if !shown}
        <div class="empty">
          <p>Choose a tab from the bar above to show it here.</p>
          {#if workspace.hiddenTabs.length > 0}
            <div class="choices">
              {#each workspace.hiddenTabs as tab (tab.id)}
                <button class="btn small" onclick={() => workspace.assign(leaf.pane_id, tab.id)}>{tab.title}</button>
              {/each}
            </div>
          {/if}
          <button class="btn" onclick={() => { workspace.focusGroup(leaf.pane_id); onnew(); }}>
            <Icon name="terminal" size={14} /> New terminal here
          </button>
        </div>
      {/if}
    </div>
  {/each}

  <!-- The tabs, each placed over the pane that shows it. Hidden ones stay mounted. -->
  {#each workspace.tabs as tab (tab.id)}
    {@const group = workspace.groupOf(tab.id)}
    {@const rect = group ? rects.get(group) : undefined}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="body"
      class:hidden={!rect}
      style:left={rect ? pct(rect.left) : "0"}
      style:width={rect ? pct(rect.right - rect.left) : "0"}
      style:top={rect ? (crowded ? `calc(${pct(rect.top)} + ${HEADER}px)` : pct(rect.top)) : "0"}
      style:height={rect ? (crowded ? `calc(${pct(rect.bottom - rect.top)} - ${HEADER}px)` : pct(rect.bottom - rect.top)) : "0"}
      onpointerdowncapture={() => group && workspace.focusGroup(group)}
    >
      {#if tab.view}
        {@render view(tab)}
      {:else}
        <PaneTree
          node={tab.root}
          tabId={tab.id}
          activePane={group === workspace.activeGroup ? tab.active : ""}
          visible={!!rect}
          crowded={workspace.paneCount(tab) > 1}
          {platform}
          {workspace}
        />
      {/if}
    </div>
  {/each}

  {#each placed.dividers as divider (divider.path.join(".") + ":" + divider.index)}
    {@const row = divider.direction === "row"}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="divider {divider.direction}"
      class:dragging={draggingDivider?.path.join(".") === divider.path.join(".") && draggingDivider?.index === divider.index}
      style:left={row ? pct(divider.at) : pct(divider.split.left)}
      style:top={row ? pct(divider.split.top) : pct(divider.at)}
      style:width={row ? undefined : pct(divider.split.right - divider.split.left)}
      style:height={row ? pct(divider.split.bottom - divider.split.top) : undefined}
      onpointerdown={(e) => startDrag(e, divider)}
    >
      <span class="line"></span>
    </div>
  {/each}
</div>

<style>
  .stage {
    position: absolute;
    inset: 0;
  }
  .slot {
    position: absolute;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .slot.active {
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .strip {
    z-index: 2;
    display: flex;
    flex: none;
    align-items: center;
    gap: 6px;
    padding: 0 4px 0 10px;
    color: var(--text-muted);
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    user-select: none;
  }
  .slot.active .strip {
    color: var(--text);
    background: var(--bg-2);
  }
  .which {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 16px;
    color: var(--text-muted);
    text-align: center;
  }
  .empty p {
    margin: 0;
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    justify-content: center;
  }
  .body {
    position: absolute;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .body.hidden {
    display: none;
  }
  /* A thin line you can see, inside a wider area you can grab. */
  .divider {
    position: absolute;
    z-index: 5;
    touch-action: none;
  }
  .divider.row {
    width: 7px;
    margin-left: -3px;
    cursor: col-resize;
  }
  .divider.column {
    height: 7px;
    margin-top: -3px;
    cursor: row-resize;
  }
  .line {
    position: absolute;
    background: var(--border-strong, var(--border));
    transition:
      background 0.12s var(--ease),
      transform 0.12s var(--ease);
  }
  .divider.row .line {
    left: 3px;
    top: 0;
    bottom: 0;
    width: 1px;
  }
  .divider.column .line {
    top: 3px;
    left: 0;
    right: 0;
    height: 1px;
  }
  .divider:hover .line,
  .divider.dragging .line {
    background: var(--accent);
  }
</style>
