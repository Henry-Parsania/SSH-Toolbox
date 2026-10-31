<script lang="ts">
  import { onMount } from "svelte";
  import {
    sshConfigImport,
    sshConfigPreview,
    type ImportPreviewItem,
    type ImportResult,
  } from "../backend";
  import type { Library } from "./library.svelte";

  interface Props {
    library: Library;
    onclose: () => void;
  }

  let { library, onclose }: Props = $props();

  let items = $state<ImportPreviewItem[] | null>(null);
  let chosen = $state<Record<string, boolean>>({});
  let error = $state("");
  let result = $state<ImportResult | null>(null);
  let busy = $state(false);

  const count = $derived(Object.values(chosen).filter(Boolean).length);

  onMount(async () => {
    try {
      const found = await sshConfigPreview();
      items = found;
      // Everything not already saved starts ticked.
      chosen = Object.fromEntries(found.filter((i) => !i.duplicate).map((i) => [i.alias, true]));
    } catch (err) {
      error = (err as { message?: string }).message ?? "Couldn't read your SSH config.";
    }
  });

  async function run() {
    busy = true;
    error = "";
    try {
      result = await sshConfigImport(Object.keys(chosen).filter((a) => chosen[a]));
      await library.load();
    } catch (err) {
      error = (err as { message?: string }).message ?? "The import didn't work.";
    } finally {
      busy = false;
    }
  }
</script>

<div class="backdrop">
  <div
    class="dialog"
    role="dialog"
    aria-label="Import from SSH config"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape") onclose();
      e.stopPropagation();
    }}
  >
    <h2>Import from ~/.ssh/config</h2>

    {#if result}
      <p class="ok">Added {result.added.length} session{result.added.length === 1 ? "" : "s"}.</p>
      {#if result.skipped.length}
        <p>Left out:</p>
        <ul>{#each result.skipped as reason (reason)}<li>{reason}</li>{/each}</ul>
      {/if}
      <div class="buttons"><button class="primary" onclick={onclose}>Done</button></div>
    {:else if error && !items}
      <p class="error">{error}</p>
      <div class="buttons"><button onclick={onclose}>Close</button></div>
    {:else if !items}
      <p>Reading…</p>
    {:else if items.length === 0}
      <p>No hosts were found. Only named hosts count: entries with * or ? in the name are skipped.</p>
      <div class="buttons"><button onclick={onclose}>Close</button></div>
    {:else}
      <p class="hint">Your config file isn't changed. Tick the hosts to add.</p>
      <div class="list">
        {#each items as item (item.alias)}
          <label class="row" class:dim={item.duplicate}>
            <input type="checkbox" bind:checked={chosen[item.alias]} disabled={item.duplicate} />
            <span class="name">{item.alias}</span>
            <span class="where">
              {item.user}@{item.hostname}{item.port === 22 ? "" : `:${item.port}`}
              {#if item.jump}· via {item.jump}{/if}
              {#if item.duplicate}· already saved{/if}
            </span>
          </label>
        {/each}
      </div>
      {#if error}<p class="error">{error}</p>{/if}
      <div class="buttons">
        <button onclick={onclose}>Cancel</button>
        <button class="primary" disabled={busy || count === 0} onclick={run}>
          Import {count}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #0008;
  }
  .dialog {
    width: min(520px, 92vw);
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    padding: 16px 20px;
    background: #252526;
    color: #d4d4d4;
    border: 1px solid #555;
    border-radius: 6px;
    font: 13px system-ui, sans-serif;
  }
  h2 {
    margin: 0 0 10px;
    font-size: 16px;
  }
  p {
    margin: 0 0 8px;
  }
  .hint {
    color: #9d9d9d;
  }
  .list {
    overflow: auto;
    min-height: 0;
    border: 1px solid #3c3c3c;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: baseline;
    padding: 4px 8px;
  }
  .row:hover {
    background: #2a2d2e;
  }
  .dim {
    color: #7a7a7a;
  }
  .name {
    font-weight: 600;
  }
  .where {
    color: #9d9d9d;
    font-size: 12px;
  }
  .ok {
    color: #89d185;
  }
  .error {
    color: #f48771;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
  button {
    padding: 5px 14px;
    font: inherit;
    color: inherit;
    background: #3c3c3c;
    border: 1px solid #555;
    border-radius: 3px;
    cursor: pointer;
  }
  button.primary {
    background: #0e639c;
    border-color: #0e639c;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
