<script lang="ts">
  // Saved commands. Pick one and send it to the pane you are working in, to every pane of the
  // tab, or to every open pane. Snippets are stored as plain text, so no passwords in them.

  import { onMount } from "svelte";
  import { snippetsGet, snippetsSet, type Snippet } from "../backend";
  import { sendToEverything, sendToPane, sendToTab } from "../terminal/broadcast";
  import { snippetBytes } from "./send";

  interface Props {
    /** The tab and pane that "this pane" means. */
    tabId: string;
    paneId: string;
    onclose: () => void;
  }

  let { tabId, paneId, onclose }: Props = $props();

  type Target = "pane" | "tab" | "all";
  let list = $state<Snippet[]>([]);
  let target = $state<Target>("pane");
  let editing = $state<Snippet | null>(null);
  let isNew = $state(false);
  let message = $state("");
  let error = $state("");

  onMount(() => {
    snippetsGet().then(
      (file) => (list = file.snippet),
      () => (error = "Couldn't read the saved snippets."),
    );
  });

  async function persist(next: Snippet[]) {
    error = "";
    try {
      await snippetsSet(next);
      list = next;
      return true;
    } catch (e) {
      error = (e as { message?: string }).message ?? "Couldn't save the snippets.";
      return false;
    }
  }

  function add() {
    editing = { id: crypto.randomUUID(), name: "", text: "", run: true };
    isNew = true;
  }

  async function saveEdit() {
    if (!editing) return;
    const item = { ...editing, name: editing.name.trim() };
    const next = isNew ? [...list, item] : list.map((s) => (s.id === item.id ? item : s));
    if (await persist(next)) editing = null;
  }

  async function remove(id: string) {
    await persist(list.filter((s) => s.id !== id));
  }

  function send(snippet: Snippet) {
    const bytes = snippetBytes(snippet);
    const count =
      target === "pane" ? Number(sendToPane(tabId, paneId, bytes)) : target === "tab" ? sendToTab(tabId, bytes) : sendToEverything(bytes);
    message = count === 0 ? "There is no open terminal to send it to." : `Sent to ${count} ${count === 1 ? "pane" : "panes"}.`;
  }
</script>

<div class="modal-backdrop">
  <div
    class="modal wide"
    role="dialog"
    aria-label="Snippets"
    tabindex="-1"
    onkeydown={(e) => {
      if (e.key === "Escape" && !editing) onclose();
      e.stopPropagation();
    }}
  >
    <h2>Snippets</h2>

    {#if editing}
      <label class="field">Name <input type="text" bind:value={editing.name} maxlength="100" /></label>
      <label class="field">
        Text to send
        <textarea rows="6" bind:value={editing.text} spellcheck="false"></textarea>
        <small>Each line is typed followed by Enter. Don't put passwords here: snippets are stored as plain text.</small>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={editing.run} /> Press Enter at the end, so it runs straight away
      </label>
      {#if error}<p class="message error">{error}</p>{/if}
      <div class="modal-actions">
        <button class="btn" onclick={() => ((editing = null), (error = ""))}>Cancel</button>
        <button class="btn primary" disabled={!editing.name.trim() || !editing.text} onclick={saveEdit}>Save</button>
      </div>
    {:else}
      <div class="target">
        Send to
        <label class="check"><input type="radio" bind:group={target} value="pane" /> this pane</label>
        <label class="check"><input type="radio" bind:group={target} value="tab" /> all panes of this tab</label>
        <label class="check"><input type="radio" bind:group={target} value="all" /> every open pane</label>
      </div>

      {#if list.length === 0}
        <p class="empty">No snippets yet. Add one for a command you type often.</p>
      {/if}
      <ul class="list">
        {#each list as s (s.id)}
          <li>
            <div class="info">
              <strong>{s.name}</strong>
              <code>{s.text.split("\n")[0]}{s.text.includes("\n") ? " …" : ""}</code>
            </div>
            <button class="btn small primary" onclick={() => send(s)}>Send</button>
            <button class="btn small" onclick={() => ((editing = { ...s }), (isNew = false))}>Edit</button>
            <button class="btn small" onclick={() => remove(s.id)}>Delete</button>
          </li>
        {/each}
      </ul>
      {#if message}<p class="message ok">{message}</p>{/if}
      {#if error}<p class="message error">{error}</p>{/if}
      <div class="modal-actions">
        <button class="btn" onclick={add}>New snippet</button>
        <button class="btn primary" onclick={onclose}>Close</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .target {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    margin-bottom: 10px;
  }
  .list {
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: 320px;
    overflow: auto;
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
  .info code {
    overflow: hidden;
    color: var(--text-faint);
    font: 12px var(--mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    color: var(--text-faint);
  }
  textarea {
    font: 13px var(--mono);
    resize: vertical;
  }
</style>
