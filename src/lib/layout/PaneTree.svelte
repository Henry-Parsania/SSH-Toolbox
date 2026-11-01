<script lang="ts">
  import type { PlatformInfo } from "../backend";
  import PaneView from "./PaneView.svelte";
  import PaneTree from "./PaneTree.svelte";
  import type { PaneNode, SplitPath } from "./panes";
  import type { Workspace } from "./workspace.svelte";

  interface Props {
    node: PaneNode;
    path?: SplitPath;
    tabId: string;
    activePane: string;
    visible: boolean;
    crowded: boolean;
    platform: PlatformInfo;
    workspace: Workspace;
  }

  let { node, path = [], tabId, activePane, visible, crowded, platform, workspace }: Props = $props();

  let container: HTMLDivElement | undefined = $state();
  let draggingDivider = $state<number | null>(null);

  function startDrag(e: PointerEvent, divider: number) {
    if (node.kind !== "split" || !container) return;
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const horizontal = node.direction === "row";
    const total = horizontal ? container.clientWidth : container.clientHeight;
    let last = horizontal ? e.clientX : e.clientY;
    draggingDivider = divider;

    const move = (ev: PointerEvent) => {
      const now = horizontal ? ev.clientX : ev.clientY;
      if (total > 0) workspace.resizeDivider(tabId, path, divider, (now - last) / total);
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

{#if node.kind === "leaf"}
  <PaneView
    {node}
    {tabId}
    {platform}
    {workspace}
    {crowded}
    active={node.pane_id === activePane}
    {visible}
  />
{:else}
  <div class="split {node.direction}" bind:this={container}>
    {#each node.children as child, i (child.kind === "leaf" ? child.pane_id : `split-${i}`)}
      <div class="child" style:flex="{node.sizes[i]} 1 0">
        <PaneTree
          node={child}
          path={[...path, i]}
          {tabId}
          {activePane}
          {visible}
          {crowded}
          {platform}
          {workspace}
        />
      </div>
      {#if i < node.children.length - 1}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="divider {node.direction}"
          class:dragging={draggingDivider === i}
          onpointerdown={(e) => startDrag(e, i)}
        >
          <span class="line"></span>
        </div>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .split {
    display: flex;
    width: 100%;
    height: 100%;
  }
  .split.row {
    flex-direction: row;
  }
  .split.column {
    flex-direction: column;
  }
  .child {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  /* A thin line you can see, inside a wider area you can grab. */
  .divider {
    position: relative;
    flex: none;
    z-index: 5;
    touch-action: none;
  }
  .divider.row {
    width: 7px;
    margin: 0 -3px;
    cursor: col-resize;
  }
  .divider.column {
    height: 7px;
    margin: -3px 0;
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
