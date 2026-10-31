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
    platform: PlatformInfo;
    workspace: Workspace;
  }

  let { node, path = [], tabId, activePane, visible, platform, workspace }: Props = $props();

  let container: HTMLDivElement | undefined = $state();

  function startDrag(e: PointerEvent, divider: number) {
    if (node.kind !== "split" || !container) return;
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const horizontal = node.direction === "row";
    const total = horizontal ? container.clientWidth : container.clientHeight;
    let last = horizontal ? e.clientX : e.clientY;

    const move = (ev: PointerEvent) => {
      const now = horizontal ? ev.clientX : ev.clientY;
      if (total > 0) workspace.resizeDivider(tabId, path, divider, (now - last) / total);
      last = now;
    };
    const stop = () => {
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
          {platform}
          {workspace}
        />
      </div>
      {#if i < node.children.length - 1}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="divider {node.direction}"
          onpointerdown={(e) => startDrag(e, i)}
        ></div>
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
  .divider {
    flex: none;
    background: #333;
    touch-action: none;
  }
  .divider.row {
    width: 5px;
    cursor: col-resize;
  }
  .divider.column {
    height: 5px;
    cursor: row-resize;
  }
  .divider:hover {
    background: #007acc;
  }
</style>
