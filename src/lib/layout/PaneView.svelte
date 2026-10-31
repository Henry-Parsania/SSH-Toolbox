<script lang="ts">
  import type { PlatformInfo, TerminalStatus } from "../backend";
  import TerminalView from "../terminal/TerminalView.svelte";
  import type { Leaf } from "./panes";
  import type { Workspace } from "./workspace.svelte";

  interface Props {
    node: Leaf;
    tabId: string;
    platform: PlatformInfo;
    workspace: Workspace;
    active: boolean;
    visible: boolean;
  }

  let { node, tabId, platform, workspace, active, visible }: Props = $props();

  // Only this pane is affected when its session ends.
  let ended = $state<string | null>(null);

  function onstatus(status: TerminalStatus) {
    if (status.type === "exited") {
      ended = status.code === null ? "Session ended." : `Session ended (code ${status.code}).`;
    } else if (status.type === "failed") {
      ended = status.error.message;
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="pane"
  class:active
  onfocusin={() => workspace.focusPane(tabId, node.pane_id)}
  onpointerdown={() => workspace.focusPane(tabId, node.pane_id)}
>
  {#if node.spec}
    <TerminalView spec={node.spec} {platform} focused={active && visible}
      searchSignal={workspace.searchTarget === node.pane_id ? workspace.searchCount : 0}
      {onstatus}
    />
  {/if}
  {#if ended}
    <div class="ended">
      <span>{ended}</span>
      <button onclick={() => workspace.closePane(tabId, node.pane_id)}>Close pane</button>
    </div>
  {/if}
</div>

<style>
  .pane {
    position: relative;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    border-top: 1px solid transparent;
  }
  .pane.active {
    border-top-color: #007acc;
  }
  .ended {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 10px;
    background: #3c3c3c;
    color: #d4d4d4;
    font: 13px system-ui, sans-serif;
  }
</style>
