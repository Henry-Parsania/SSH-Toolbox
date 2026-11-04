<script lang="ts">
  import type { PlatformInfo, TerminalStatus } from "../backend";
  import { settings } from "../settings.svelte";
  import { nextRetryDelay } from "./autoReconnect";
  import { serialSendBreak, serialSetControl } from "../backend/serial";
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

  // Auto-reconnect: only for an SSH session that was working and then lost its connection. A
  // session that never connected (wrong password, refused) or that ended on purpose is left alone.
  let wasConnected = false;
  let retries = 0;
  let retryIn = $state<number | null>(null);
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let countdown: ReturnType<typeof setInterval> | undefined;

  function stopRetry() {
    clearTimeout(retryTimer);
    clearInterval(countdown);
    retryIn = null;
  }
  $effect(() => stopRetry);

  // The serial lines are shown as the user last set them. Devices start with both low.
  let dtr = $state(false);
  let rts = $state(false);
  let lineError = $state("");
  const sessionId = $derived(workspace.sessionIds[node.pane_id]);

  async function line(change: () => Promise<void>) {
    lineError = "";
    try {
      await change();
    } catch (err) {
      lineError = (err as { message?: string }).message ?? "That didn't work.";
    }
  }

  function reconnect() {
    stopRetry();
    dtr = false;
    rts = false;
    ended = null;
    attempt++;
  }

  const canReconnect = $derived(node.spec?.type === "ssh" || node.spec?.type === "serial");

  // Like MobaXterm: once the session has ended, R starts it again and C closes the pane. While it
  // is still connecting, Esc gives up (a letter could be the start of something the user is typing
  // ahead). Only the pane that has the keyboard listens, and typing in a
  // box (a password question, a search) is left alone.
  $effect(() => {
    if (!active || !visible || (!ended && !connecting)) return;
    const onkey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const target = e.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable]") && !target.closest(".xterm")) return;
      if (document.querySelector(".modal-backdrop")) return;
      const key = e.key.toLowerCase();
      if (ended && key === "r" && canReconnect) reconnect();
      else if (ended && key === "c") workspace.closePane(tabId, node.pane_id);
      else if (!ended && e.key === "Escape") workspace.closePane(tabId, node.pane_id);
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("keydown", onkey, true);
    return () => window.removeEventListener("keydown", onkey, true);
  });

  function onstatus(status: TerminalStatus) {
    connecting = status.type === "connecting";
    if (status.type === "connected") {
      wasConnected = true;
      retries = 0;
    }
    if (status.type === "exited") {
      ended = status.code === null ? "Session ended." : `Session ended (code ${status.code}).`;
    } else if (status.type === "failed") {
      ended = status.error.message;
      const delay = nextRetryDelay({
        enabled: settings.auto_reconnect,
        isSsh: node.spec?.type === "ssh" && !node.spec.monitor,
        wasConnected,
        lost: status.error.code === "connection_lost",
        retries,
      });
      if (delay !== null) {
        retries++;
        retryIn = delay;
        countdown = setInterval(() => (retryIn = Math.max(0, (retryIn ?? 1) - 1)), 1000);
        retryTimer = setTimeout(reconnect, delay * 1000);
      }
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
        saveSignal={workspace.saveTarget === node.pane_id ? workspace.saveCount : 0}
        attachId={workspace.attach[node.pane_id] ?? ""}
        {tabId}
        paneId={node.pane_id}
        broadcasting={workspace.broadcast}
        oncwd={(path) => (workspace.cwd[node.pane_id] = path)}
        onsession={(id) => {
          if (id) workspace.sessionIds[node.pane_id] = id;
          else delete workspace.sessionIds[node.pane_id];
        }}
        {onstatus}
      />
    {/key}
    {#if connecting && !ended}
      <div class="connecting"><span class="spinner"></span> Connecting… <span class="hint">Press Esc to cancel</span></div>
    {/if}
  {/if}
  {#if node.spec?.type === "serial" && sessionId && !ended}
    <div class="lines" role="toolbar" aria-label="Serial lines">
      <button
        class="chip"
        class:on={dtr}
        aria-pressed={dtr}
        title="Data Terminal Ready line"
        onclick={() => line(async () => { await serialSetControl(sessionId, { dtr: !dtr }); dtr = !dtr; })}
      >DTR</button>
      <button
        class="chip"
        class:on={rts}
        aria-pressed={rts}
        title="Request To Send line"
        onclick={() => line(async () => { await serialSetControl(sessionId, { rts: !rts }); rts = !rts; })}
      >RTS</button>
      <button class="chip" title="Send a break signal" onclick={() => line(() => serialSendBreak(sessionId))}>
        Break
      </button>
      {#if lineError}<span class="line-error">{lineError}</span>{/if}
    </div>
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
      <span class="text">{ended}{#if retryIn !== null} Reconnecting in {retryIn} s (press R to reconnect now, or C to cancel).{/if}</span>
      <span class="actions">
        {#if node.spec?.type === "ssh" || node.spec?.type === "serial"}
          <button class="btn small primary" title="Press R" onclick={reconnect}>
            <Icon name="refresh" size={13} /> Reconnect <span class="key">R</span>
          </button>
        {/if}
        <button class="btn small" title="Press C" onclick={() => workspace.closePane(tabId, node.pane_id)}>
          Close <span class="key">C</span>
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
  .lines {
    position: absolute;
    top: 6px;
    right: 44px;
    z-index: 6;
    display: flex;
    align-items: center;
    gap: 4px;
    opacity: 0.85;
  }
  .lines:hover {
    opacity: 1;
  }
  .chip {
    padding: 2px 9px;
    color: var(--text-muted);
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }
  .chip.on {
    color: var(--on-accent);
    background: var(--accent);
    border-color: var(--accent);
  }
  .line-error {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--danger);
    font-size: 11px;
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
  .key {
    margin-left: 4px;
    padding: 0 5px;
    font: 11px var(--mono);
    border: 1px solid currentColor;
    border-radius: 4px;
    opacity: 0.7;
  }
  .hint {
    margin-left: 8px;
    color: var(--text-faint);
  }
  .actions {
    display: flex;
    gap: 6px;
    flex: none;
  }
</style>
