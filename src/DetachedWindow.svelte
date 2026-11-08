<script lang="ts">
  // A window that shows one tab on its own: a tab moved out of the main window, or a pop-up such
  // as a server's process monitor. What to show comes from the window's address.
  import { platformInfo, windowCloseSelf, type PlatformInfo } from "./lib/backend";
  import { decodePayload, encodePayload } from "./lib/layout/detach";
  import { otherWindowsAt, sendTabTo } from "./lib/backend/window";
  import PaneTree from "./lib/layout/PaneTree.svelte";
  import { Workspace } from "./lib/layout/workspace.svelte";
  import { loadSettings, settings } from "./lib/settings.svelte";
  import { applyTheme, currentTheme } from "./lib/ui/themes.svelte";

  const payload = decodePayload(new URLSearchParams(location.search).get("window") ?? "");
  const workspace = new Workspace();
  let platform = $state<PlatformInfo | null>(null);
  let failure = $state(payload ? "" : "This window was opened without anything to show.");

  if (payload) {
    workspace.attach = payload.attach;
    const tab = workspace.newTab(null, payload.title);
    tab.root = payload.root;
    tab.active = payload.active;
  }

  // Putting the tab back: drag the bar at the top onto the main window, or press the button.
  let dragOver = $state(false);
  async function sendBack(only: boolean, screenX = 0, screenY = 0) {
    const tab = workspace.tabs[0];
    if (!tab) return;
    if (!only && !(await otherWindowsAt(screenX, screenY).catch(() => [] as string[])).includes("main")) return;
    const moved = workspace.detachTab(tab.id);
    if (moved) await sendTabTo("main", encodePayload(moved)).catch(() => {});
  }
  function startBack(e: PointerEvent) {
    if (e.button !== 0) return;
    dragOver = true;
    const stop = (ev: PointerEvent) => {
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      dragOver = false;
      if (ev.type === "pointerup") void sendBack(false, ev.screenX, ev.screenY);
    };
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  }

  $effect(() => {
    Promise.all([platformInfo(), loadSettings().catch(() => {})]).then(
      ([info]) => (platform = info),
      (err: { message?: string }) => (failure = err.message ?? "Couldn't start."),
    );
  });

  $effect(() => {
    applyTheme(currentTheme());
  });

  // When the last pane is closed there is nothing left to show.
  $effect(() => {
    if (payload && workspace.tabs.length === 0) void windowCloseSelf().catch(() => {});
  });
</script>

{#if failure}
  <p class="failure">{failure}</p>
{:else if platform}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header class="back" class:over={dragOver} onpointerdown={startBack}>
    <span class="grab" title="Drag this bar onto the main window to put the tab back there">{workspace.tabs[0]?.title ?? ""}</span>
    <button class="btn small" onpointerdown={(e) => e.stopPropagation()} onclick={() => sendBack(true)}>Move back to the main window</button>
  </header>
  <main>
    {#each workspace.tabs as tab (tab.id)}
      <PaneTree
        node={tab.root}
        tabId={tab.id}
        activePane={tab.active}
        visible={true}
        crowded={workspace.paneCount(tab) > 1}
        {platform}
        {workspace}
      />
    {/each}
  </main>
{/if}

<style>
  .back {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 30px;
    padding: 0 8px;
    color: var(--text-muted);
    font-size: 12px;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    cursor: grab;
    user-select: none;
  }
  .back.over {
    cursor: grabbing;
    border-bottom-color: var(--accent);
  }
  .grab {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  main {
    position: absolute;
    inset: 30px 0 0 0;
    background: var(--bg-0);
  }
  .failure {
    margin: 24px;
    color: var(--text-muted);
  }
</style>
