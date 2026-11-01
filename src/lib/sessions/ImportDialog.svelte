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

<div class="modal-backdrop">
  <div
    class="modal wide"
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
      <p class="message ok">Added {result.added.length} session{result.added.length === 1 ? "" : "s"}.</p>
      {#if result.skipped.length}
        <p>Left out:</p>
        <ul>{#each result.skipped as reason (reason)}<li>{reason}</li>{/each}</ul>
      {/if}
      <div class="modal-actions"><button class="btn primary" onclick={onclose}>Done</button></div>
    {:else if error && !items}
      <p class="message error">{error}</p>
      <div class="modal-actions"><button class="btn" onclick={onclose}>Close</button></div>
    {:else if !items}
      <p>Reading…</p>
    {:else if items.length === 0}
      <p>No hosts were found. Only named hosts count: entries with * or ? in the name are skipped.</p>
      <div class="modal-actions"><button class="btn" onclick={onclose}>Close</button></div>
    {:else}
      <p class="lead">Your config file isn't changed. Tick the hosts to add.</p>
      <div class="list">
        {#each items as item (item.alias)}
          <label class="item" class:dim={item.duplicate}>
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
      {#if error}<p class="message error">{error}</p>{/if}
      <div class="modal-actions">
        <button class="btn" onclick={onclose}>Cancel</button>
        <button class="btn primary" disabled={busy || count === 0} onclick={run}>
          Import {count}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .list {
    max-height: 340px;
    overflow: auto;
    margin-bottom: 4px;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .item {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin: 0;
    padding: 7px 12px;
    border-bottom: 1px solid var(--border);
    cursor: pointer;
  }
  .item:last-child {
    border-bottom: none;
  }
  .item:hover {
    background: var(--bg-hover);
  }
  .item.dim {
    opacity: 0.55;
  }
  .name {
    font-weight: 600;
  }
  .where {
    color: var(--text-faint);
    font-size: 12px;
  }
  ul {
    margin: 0 0 8px;
    padding-left: 20px;
    color: var(--text-muted);
  }
</style>
