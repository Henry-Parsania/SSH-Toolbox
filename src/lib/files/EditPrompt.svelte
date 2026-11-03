<script lang="ts">
  // Asks whether to send a saved file back to the server. It shows wherever the app is, because
  // the file can be saved long after the file panel was closed.

  import { edits } from "./edits.svelte";

  const question = $derived(edits.questions[0]);
  const item = $derived(question ? edits.items.find((i) => i.id === question.id) : undefined);
</script>

{#if question && item}
  <div class="modal-backdrop">
    <div
      class="modal"
      role="alertdialog"
      aria-label="Send the saved file"
      tabindex="-1"
      onkeydown={(e) => {
        if (e.key === "Escape") void edits.answer(question.id, "later");
        e.stopPropagation();
      }}
    >
      {#if question.kind === "send"}
        <h2>Send {item.name} to the server?</h2>
        <p>
          You saved <strong>{item.name}</strong>. The server still has the old version of
          <code>{item.remotePath}</code>.
        </p>
        <div class="modal-actions">
          <button class="btn" onclick={() => edits.answer(question.id, "later")}>Not now</button>
          <button class="btn" onclick={() => edits.answer(question.id, "always")}>Always send automatically</button>
          <!-- svelte-ignore a11y_autofocus -->
          <button class="btn primary" autofocus onclick={() => edits.answer(question.id, "send")}>Send</button>
        </div>
      {:else}
        <h2>The server's file has changed</h2>
        <p>
          <code>{item.remotePath}</code> was changed on the server after you opened it. Sending your version
          replaces those changes.
        </p>
        <div class="modal-actions">
          <button class="btn" autofocus onclick={() => edits.answer(question.id, "later")}>Don't send</button>
          <button class="btn danger" onclick={() => edits.answer(question.id, "send")}>Replace the server's file</button>
        </div>
      {/if}
    </div>
  </div>
{/if}
