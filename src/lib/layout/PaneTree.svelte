<script lang="ts">
  import type { PlatformInfo } from "../backend";
  import PaneView from "./PaneView.svelte";
  import { placeAll, type DividerPlace, type PaneNode } from "./panes";
  import type { Workspace } from "./workspace.svelte";

  interface Props {
    node: PaneNode;
    tabId: string;
    activePane: string;
    visible: boolean;
    crowded: boolean;
    platform: PlatformInfo;
    workspace: Workspace;
  }

  let { node, tabId, activePane, visible, crowded, platform, workspace }: Props = $props();

  // Every pane is drawn in one flat list, keyed by its id. Splitting then changes only where a
  // pane sits, so the terminals already running are never rebuilt.
  const placed = $derived(placeAll(node));

  let container: HTMLDivElement | undefined = $state();
  let draggingDivider = $state<DividerPlace | null>(null);

  const pct = (share: number) => `${share * 100}%`;

  function startDrag(e: PointerEvent, divider: DividerPlace) {
    if (!container) return;
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const horizontal = divider.direction === "row";
    const { left, right, top, bottom } = divider.split;
    const total = horizontal
      ? (right - left) * container.clientWidth
      : (bottom - top) * container.clientHeight;
    let last = horizontal ? e.clientX : e.clientY;
    draggingDivider = divider;

    const move = (ev: PointerEvent) => {
      const now = horizontal ? ev.clientX : ev.clientY;
      if (total > 0) workspace.resizeDivider(tabId, divider.path, divider.index, (now - last) / total);
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

<div class="tree" bind:this={container}>
  {#each placed.panes as { leaf, rect } (leaf.pane_id)}
    <div
      class="cell"
      style:left={pct(rect.left)}
      style:top={pct(rect.top)}
      style:width={pct(rect.right - rect.left)}
      style:height={pct(rect.bottom - rect.top)}
    >
      <PaneView
        node={leaf}
        {tabId}
        {platform}
        {workspace}
        {crowded}
        active={leaf.pane_id === activePane}
        {visible}
      />
    </div>
  {/each}

  {#each placed.dividers as divider (divider.path.join(".") + ":" + divider.index)}
    {@const row = divider.direction === "row"}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="divider {divider.direction}"
      class:dragging={draggingDivider?.path.join(".") === divider.path.join(".") &&
        draggingDivider?.index === divider.index}
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
  .tree {
    position: relative;
    width: 100%;
    height: 100%;
  }
  .cell {
    position: absolute;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
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
    background: var(--border);
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
  .divider.row:hover .line,
  .divider.row.dragging .line {
    transform: scaleX(3);
  }
  .divider.column:hover .line,
  .divider.column.dragging .line {
    transform: scaleY(3);
  }
</style>
