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
  // Bumped by Reconnect, which opens a fresh session in the same pane.
  let attempt = $state(0);

  function reconnect() {
    ended = null;
    attempt++;
  }

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
  {#if node.spec && workspace.waiting[node.pane_id]}
    <div class="waiting">
      <p>{node.title}</p>
      <p class="hint">This session was open when the app closed. It doesn't connect by itself.</p>
      <button onclick={() => workspace.connect(node.pane_id)}>Connect</button>
    </div>
  {:else if node.spec}
    {#key attempt}
    <TerminalView spec={node.spec} {platform} focused={active && visible}
      searchSignal={workspace.searchTarget === node.pane_id ? workspace.searchCount : 0}
      {tabId}
      paneId={node.pane_id}
      broadcasting={workspace.broadcast}
      {onstatus}
    />
    {/key}
  {/if}
  {#if ended}
    <div class="ended">
      <span>{ended}</span>
      <span class="actions">
        {#if node.spec?.type === "ssh"}<button onclick={reconnect}>Reconnect</button>{/if}
        <button onclick={() => workspace.closePane(tabId, node.pane_id)}>Close pane</button>
      </span>
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
  .waiting {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    height: 100%;
    background: #1e1e1e;
    color: #d4d4d4;
    font: 14px system-ui, sans-serif;
  }
  .waiting p {
    margin: 0;
  }
  .waiting .hint {
    color: #8a8a8a;
    font-size: 12px;
  }
  .waiting button {
    margin-top: 6px;
    padding: 5px 18px;
    font: inherit;
    color: #fff;
    background: #0e639c;
    border: none;
    border-radius: 3px;
    cursor: pointer;
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
