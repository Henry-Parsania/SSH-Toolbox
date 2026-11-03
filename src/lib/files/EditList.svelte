<script lang="ts">
  // The server files open on this computer, and whether their saves are sent back.

  import Icon from "../ui/Icon.svelte";
  import { edits, type EditItem } from "./edits.svelte";

  function label(item: EditItem): string {
    switch (item.state) {
      case "sending":
        return "Sending to the server…";
      case "sent":
        return item.sentAt ? `Sent to the server at ${new Date(item.sentAt).toLocaleTimeString()}` : "Sent to the server";
      case "waiting":
        return item.message;
      case "failed":
        return item.message;
      default:
        return item.message || "Open here. Save it in your editor and it goes back to the server.";
    }
  }
</script>

{#if edits.items.length > 0}
  <section class="edits" aria-label="Files open on this computer">
    <header>
      <span class="title">Editing here</span>
      <label class="auto" title="When on, a saved file is sent to the server straight away">
        <input
          type="checkbox"
          checked={edits.mode === "auto"}
          onchange={(e) => edits.setMode(e.currentTarget.checked ? "auto" : "ask")}
        />
        Send saves automatically
      </label>
    </header>
    <ul>
      {#each edits.items as item (item.id)}
        <li class={item.state}>
          <div class="text">
            <span class="name" title={item.remotePath}>{item.name}</span>
            <span class="state">{label(item)}</span>
          </div>
          {#if item.state === "waiting" || item.state === "failed" || item.state === "watching" || item.state === "sent"}
            <button class="btn small" title="Send the saved file to the server now" onclick={() => edits.sendNow(item.id)}>
              <Icon name="up" size={12} /> Send now
            </button>
          {/if}
          <button class="icon-btn small" title="Open it again" onclick={() => edits.reopen(item.id, false)}>
            <Icon name="edit" size={13} />
          </button>
          <button
            class="icon-btn small"
            title="Stop watching and delete the copy on this computer"
            onclick={() => edits.stop(item.id)}
          >
            <Icon name="close" size={13} />
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .edits {
    border-top: 1px solid var(--border);
    background: var(--bg-1);
    max-height: 30%;
    overflow-y: auto;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px 4px 12px;
  }
  .title {
    flex: 1;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .auto {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: 12px;
  }
  ul {
    margin: 0;
    padding: 0 10px 8px;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0;
  }
  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .state {
    overflow: hidden;
    color: var(--text-faint);
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  li.failed .state {
    color: var(--danger, #d9534f);
  }
  li.sent .state {
    color: var(--ok, #3fb950);
  }
</style>
