<script lang="ts">
  // The servers whose keys were accepted (the known_hosts file that OpenSSH uses too). Removing
  // one makes the next connection ask again, which is what to do after a server was reinstalled.

  import { onMount } from "svelte";
  import { knownHostsList, knownHostsRemove, type KnownHost } from "../backend";

  interface Props {
    onclose: () => void;
  }

  let { onclose }: Props = $props();

  let entries = $state<KnownHost[]>([]);
  let filter = $state("");
  let confirming = $state<KnownHost | null>(null);
  let error = $state("");
  let loaded = $state(false);

  const shown = $derived(
    entries.filter((e) => `${e.hosts} ${e.algorithm} ${e.fingerprint}`.toLowerCase().includes(filter.trim().toLowerCase())),
  );

  async function load() {
    try {
      entries = await knownHostsList();
      error = "";
    } catch (e) {
      error = (e as { message?: string }).message ?? "Couldn't read the known hosts.";
    } finally {
      loaded = true;
    }
  }
  onMount(() => void load());

  async function remove(entry: KnownHost) {
    confirming = null;
    try {
      await knownHostsRemove(entry.line, entry.fingerprint);
    } catch (e) {
      error = (e as { message?: string }).message ?? "Couldn't remove that entry.";
    }
    await load();
  }
</script>

<div class="modal-backdrop">
  <div
    class="modal wide"
    role="dialog"
    aria-label="Trusted servers"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape" && !confirming) onclose();
      e.stopPropagation();
    }}
  >
    <h2>Trusted servers</h2>
    <p class="lead">
      Servers whose keys you accepted. If a server was reinstalled and the app warns that its key
      changed, remove its entry here, then connect again and check the new fingerprint.
    </p>
    <input type="text" placeholder="Filter" bind:value={filter} spellcheck="false" />

    {#if error}<p class="message error">{error}</p>{/if}
    {#if loaded && entries.length === 0 && !error}<p class="empty">No trusted servers yet.</p>{/if}
    <ul class="list">
      {#each shown as e (e.line)}
        <li>
          <div class="info">
            <strong>{e.hosts}</strong>
            <code>{e.algorithm} {e.fingerprint}{e.marker ? " " + e.marker : ""}</code>
          </div>
          <button class="btn small" onclick={() => (confirming = e)}>Remove</button>
        </li>
      {/each}
    </ul>

    {#if confirming}
      <div class="confirm" role="alert">
        <p>Stop trusting <strong>{confirming.hosts}</strong>? The next connection to it will ask you again. A copy of the file is kept as known_hosts.bak.</p>
        <div class="modal-actions">
          <button class="btn" onclick={() => (confirming = null)}>Cancel</button>
          <button class="btn primary" onclick={() => confirming && remove(confirming)}>Remove</button>
        </div>
      </div>
    {/if}

    <div class="modal-actions"><button class="btn primary" onclick={onclose}>Close</button></div>
  </div>
</div>

<style>
  .list {
    max-height: 340px;
    margin: 10px 0 0;
    padding: 0;
    overflow: auto;
    list-style: none;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 0;
    border-bottom: 1px solid var(--border);
  }
  .info {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }
  .info strong {
    overflow-wrap: anywhere;
  }
  .info code {
    color: var(--text-faint);
    font: 11px var(--mono);
    overflow-wrap: anywhere;
  }
  .empty {
    color: var(--text-faint);
  }
  .confirm {
    margin-top: 12px;
    padding: 10px 12px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
</style>
