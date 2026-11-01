<script lang="ts">
  import type { PlatformInfo, TerminalStatus } from "../backend";
  import TerminalView from "../terminal/TerminalView.svelte";
  import Icon from "../ui/Icon.svelte";
  import type { Leaf } from "./panes";
  import type { Workspace } from "./workspace.svelte";

  interface Props {
    node: Leaf;
    tabId: string;
    platform: PlatformInfo;
    workspace: Workspace;
    active: boolean;
    visible: boolean;
    /** More than one pane is on screen, so the active one is marked. */
    crowded: boolean;
  }

  let { node, tabId, platform, workspace, active, visible, crowded }: Props = $props();

  // Only this pane is affected when its session ends.
  let ended = $state<string | null>(null);
  // Bumped by Reconnect, which opens a fresh session in the same pane.
  let attempt = $state(0);
  let connecting = $state(false);

  function reconnect() {
    ended = null;
    attempt++;
  }

  function onstatus(status: TerminalStatus) {
    connecting = status.type === "connecting";
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
  class:marked={crowded && active}
  onfocusin={() => workspace.focusPane(tabId, node.pane_id)}
  onpointerdown={() => workspace.focusPane(tabId, node.pane_id)}
>
  {#if node.spec && workspace.waiting[node.pane_id]}
    <div class="waiting">
      <div class="badge"><Icon name="server" size={26} /></div>
      <h3>{node.title}</h3>
      <p>This session was open when the app closed.<br />It doesn't connect by itself.</p>
      <button class="btn primary" onclick={() => workspace.connect(node.pane_id)}>
        <Icon name="play" size={14} /> Connect
      </button>
    </div>
  {:else if node.spec}
    {#key attempt}
      <TerminalView
        spec={node.spec}
        {platform}
        focused={active && visible}
        searchSignal={workspace.searchTarget === node.pane_id ? workspace.searchCount : 0}
        {tabId}
        paneId={node.pane_id}
        broadcasting={workspace.broadcast}
        oncwd={(path) => (workspace.cwd[node.pane_id] = path)}
        {onstatus}
      />
    {/key}
    {#if connecting && !ended}
      <div class="connecting"><span class="spinner"></span> Connecting…</div>
    {/if}
  {/if}
  {#if crowded}
    <button
      class="close-pane"
      aria-label="Close this pane"
      title="Close this pane (Ctrl+Shift+W)"
      onclick={() => workspace.closePane(tabId, node.pane_id)}
    >
      <Icon name="close" size={12} />
    </button>
  {/if}
  {#if ended}
    <div class="ended" role="alert">
      <span class="text">{ended}</span>
      <span class="actions">
        {#if node.spec?.type === "ssh"}
          <button class="btn small primary" onclick={reconnect}>
            <Icon name="refresh" size={13} /> Reconnect
          </button>
        {/if}
        <button class="btn small" onclick={() => workspace.closePane(tabId, node.pane_id)}>
          Close pane
        </button>
      </span>
    </div>
  {/if}
</div>

<style>
  .pane {
    position: relative;
    width: 100%;
    height: 100%;
    background: var(--bg-0);
  }
  /* The pane that has the keyboard, when there are several. */
  .pane.marked::after {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    box-shadow: inset 0 0 0 1px var(--accent);
    opacity: 0.8;
    z-index: 3;
  }
  .close-pane {
    position: absolute;
    top: 6px;
    right: 14px;
    z-index: 6;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    color: var(--text-muted);
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    opacity: 0;
    cursor: pointer;
    transition: opacity 0.12s;
  }
  .pane:hover .close-pane,
  .close-pane:focus-visible {
    opacity: 1;
  }
  .close-pane:hover {
    color: var(--text);
    background: var(--bg-active);
  }
  .waiting {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    width: 100%;
    height: 100%;
    padding: 24px;
    text-align: center;
  }
  .waiting .badge {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    color: var(--accent);
    background: var(--accent-soft);
    border-radius: 16px;
  }
  .waiting h3 {
    margin: 6px 0 0;
    font-size: 16px;
    font-weight: 600;
  }
  .waiting p {
    margin: 0 0 8px;
    color: var(--text-muted);
  }
  .connecting {
    position: absolute;
    top: 10px;
    right: 14px;
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 12px;
    color: var(--text-muted);
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    font-size: 12px;
    box-shadow: var(--shadow-small);
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .ended {
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: 10px;
    z-index: 4;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 9px 12px;
    color: var(--text);
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-left: 3px solid var(--warning);
    border-radius: var(--radius);
    box-shadow: var(--shadow-small);
  }
  .actions {
    display: flex;
    gap: 6px;
    flex: none;
  }
</style>
