<script lang="ts">
  // A classic menu bar, so every feature can be found without remembering a shortcut.

  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { appVersion } from "../backend";

  export interface MenuItem {
    label: string;
    /** Shown on the right, such as "Ctrl+Shift+T". */
    hint?: string;
    /** Shows a tick when set. */
    checked?: boolean;
    disabled?: boolean;
    onselect: () => void;
  }
  export type Menu = { label: string; items: (MenuItem | "separator")[] };

  interface Props {
    menus: Menu[];
  }

  let { menus }: Props = $props();

  // The window has no title bar of its own: this bar is dragged to move it, and the buttons at the
  // right minimize, maximize and close it.
  const win = getCurrentWindow();
  let maximized = $state(false);
  let version = $state("");
  void appVersion().then((v) => (version = v)).catch(() => {});
  void win.isMaximized().then((m) => (maximized = m)).catch(() => {});
  $effect(() => {
    const stop = win.onResized(() => void win.isMaximized().then((m) => (maximized = m)));
    return () => void stop.then((off) => off());
  });

  let open = $state<number | null>(null);
  let bar: HTMLElement | undefined = $state();

  function choose(item: MenuItem) {
    open = null;
    if (!item.disabled) item.onselect();
  }

  // A click outside closes the menu, and so does Escape. Alt opens the first menu.
  $effect(() => {
    if (open === null) return;
    const away = (e: PointerEvent) => {
      if (bar && !bar.contains(e.target as Node)) open = null;
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") open = null;
      else if (e.key === "ArrowRight") open = ((open ?? 0) + 1) % menus.length;
      else if (e.key === "ArrowLeft") open = ((open ?? 0) + menus.length - 1) % menus.length;
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", key);
    };
  });
</script>

<nav class="menubar" bind:this={bar} aria-label="Menu">
  {#each menus as menu, i (menu.label)}
    <div class="mb-menu">
      <button
        class="mb-top"
        class:mb-on={open === i}
        aria-haspopup="menu"
        aria-expanded={open === i}
        onclick={() => (open = open === i ? null : i)}
        onpointerenter={() => {
          if (open !== null) open = i;
        }}>{menu.label}</button
      >
      {#if open === i}
        <div class="mb-drop" role="menu">
          {#each menu.items as item, j (j)}
            {#if item === "separator"}
              <div class="mb-rule" role="separator"></div>
            {:else}
              <button
                class="mb-item"
                role="menuitem"
                disabled={item.disabled}
                onclick={() => choose(item)}
              >
                <span class="mb-tick">{item.checked ? "✓" : ""}</span>
                <span class="mb-text">{item.label}</span>
                {#if item.hint}<span class="mb-hint">{item.hint}</span>{/if}
              </button>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  {/each}
  <div class="drag" data-tauri-drag-region title={version}>{version ? `SSH Toolbox ${version}` : ""}</div>
  <div class="wc">
    <button class="wc-btn" aria-label="Minimize" title="Minimize" onclick={() => void win.minimize()}>
      <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 5h8" stroke="currentColor" stroke-width="1" fill="none" /></svg>
    </button>
    <button
      class="wc-btn"
      aria-label={maximized ? "Restore" : "Maximize"}
      title={maximized ? "Restore" : "Maximize"}
      onclick={() => void win.toggleMaximize()}
    >
      {#if maximized}
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M2.5 2.5h6v6h-6zM4 2.5V1h5.5v5.5H8" stroke="currentColor" stroke-width="1" fill="none" /></svg>
      {:else}
        <svg width="10" height="10" viewBox="0 0 10 10"><rect x="1.5" y="1.5" width="7" height="7" stroke="currentColor" stroke-width="1" fill="none" /></svg>
      {/if}
    </button>
    <button class="wc-btn wc-close" aria-label="Close" title="Close" onclick={() => void win.close()}>
      <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 1l8 8M9 1L1 9" stroke="currentColor" stroke-width="1" fill="none" /></svg>
    </button>
  </div>
</nav>

<style>
  .menubar {
    display: flex;
    flex: none;
    align-items: center;
    height: 28px;
    padding: 0 6px;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    font-size: 12.5px;
    user-select: none;
  }
  .mb-menu {
    position: relative;
  }
  .drag {
    flex: 1;
    align-self: stretch;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 40px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .wc {
    display: flex;
    align-self: stretch;
    margin-right: -6px;
  }
  .wc-btn {
    display: grid;
    place-items: center;
    width: 46px;
    min-height: 0;
    padding: 0;
    color: var(--text);
    background: transparent;
    border: 0;
    border-radius: 0;
    box-shadow: none;
    cursor: default;
  }
  .wc-btn:hover {
    background: var(--bg-2);
  }
  .wc-close:hover {
    color: #fff;
    background: #e5484d;
  }
  .mb-top {
    height: 24px;
    padding: 0 10px;
    color: var(--text);
    background: transparent;
    border: 0;
    border-radius: 5px;
    box-shadow: none;
    min-height: 0;
    font: inherit;
    cursor: pointer;
  }
  .mb-top:hover,
  .mb-top.mb-on {
    background: var(--bg-2);
  }
  .mb-drop {
    position: absolute;
    top: 26px;
    left: 0;
    z-index: 300;
    min-width: 250px;
    padding: 4px;
    background: var(--bg-1);
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    box-shadow: var(--shadow-small, 0 6px 20px rgba(0, 0, 0, 0.35));
  }
  .mb-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 10px 5px 6px;
    color: var(--text);
    background: transparent;
    border: 0;
    border-radius: 5px;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .mb-item:hover:not(:disabled) {
    background: var(--bg-2);
  }
  .mb-item:disabled {
    color: var(--text-muted);
    cursor: default;
    opacity: 0.6;
  }
  .mb-tick {
    width: 14px;
    color: var(--accent);
    text-align: center;
  }
  .mb-text {
    flex: 1;
    white-space: nowrap;
  }
  .mb-hint {
    margin-left: 24px;
    color: var(--text-muted);
    font-size: 11.5px;
  }
  .mb-rule {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
</style>
