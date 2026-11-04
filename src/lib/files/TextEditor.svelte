<script lang="ts">
  // A small editor for text files, on the server or on this computer. Save puts the text back
  // (and asks first if the file changed meanwhile); Show changes compares it with what was opened.

  import { onMount } from "svelte";
  import { filesReadText, filesWriteText, type TextFile } from "../backend/files";
  import { changedHunks, diffLines, hasChanges } from "./diff";

  interface Props {
    /** The open file backend. */
    fileId: string;
    path: string;
    onclose: () => void;
  }

  let { fileId, path, onclose }: Props = $props();

  let loaded = $state<TextFile | null>(null);
  let original = $state("");
  let text = $state("");
  let opened = $state<number | null>(null);
  let error = $state("");
  let saving = $state(false);
  let showDiff = $state(false);
  let conflict = $state(false);
  let leaving = $state(false);

  const dirty = $derived(loaded !== null && text !== original);
  const diff = $derived(showDiff ? diffLines(original, text) : []);
  const hunks = $derived(showDiff ? changedHunks(diff) : []);
  const name = $derived(path.split(/[\\/]/).pop() ?? path);

  onMount(() => {
    filesReadText(fileId, path).then(
      (file) => {
        loaded = file;
        original = text = file.text;
        opened = file.modified;
      },
      (e) => (error = (e as { message?: string }).message ?? "Couldn't open that file."),
    );
  });

  async function save(force = false) {
    if (!loaded || saving) return;
    saving = true;
    error = "";
    try {
      const outcome = await filesWriteText(fileId, path, text, loaded.crlf, opened, force);
      if (outcome.result === "remote_changed") {
        conflict = true;
      } else {
        original = text;
        opened = outcome.modified;
        conflict = false;
      }
    } catch (e) {
      error = (e as { message?: string }).message ?? "Couldn't save the file.";
    } finally {
      saving = false;
    }
  }

  function close() {
    if (dirty) leaving = true;
    else onclose();
  }

  function keydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void save();
    } else if (e.key === "Escape" && !leaving && !conflict) {
      close();
    }
    e.stopPropagation();
  }

  // Tab types a tab character instead of leaving the box.
  function tab(e: KeyboardEvent) {
    if (e.key !== "Tab" || e.ctrlKey || e.altKey || e.metaKey) return;
    e.preventDefault();
    const box = e.currentTarget as HTMLTextAreaElement;
    const { selectionStart: from, selectionEnd: to } = box;
    text = text.slice(0, from) + "\t" + text.slice(to);
    queueMicrotask(() => (box.selectionStart = box.selectionEnd = from + 1));
  }
</script>

<div class="modal-backdrop">
  <div class="modal editor" role="dialog" aria-label="Edit {name}" tabindex="-1" onkeydown={keydown}>
    <h2>{name}{dirty ? " •" : ""}</h2>
    <p class="path">{path}</p>

    {#if error && !loaded}
      <p class="message error">{error}</p>
      <div class="modal-actions"><button class="btn primary" onclick={onclose}>Close</button></div>
    {:else if !loaded}
      <p>Opening…</p>
    {:else}
      {#if showDiff}
        <div class="diff" aria-label="Changes">
          {#if !hasChanges(diff)}
            <p class="none">No changes yet.</p>
          {/if}
          {#each hunks as h, i (i)}
            {#if h.type === "gap"}
              <div class="gap">… {h.hidden} unchanged {h.hidden === 1 ? "line" : "lines"} …</div>
            {:else}
              <div class="line {h.type}"><span class="sign">{h.type === "add" ? "+" : h.type === "del" ? "−" : " "}</span>{h.text}</div>
            {/if}
          {/each}
        </div>
      {:else}
        <!-- svelte-ignore a11y_autofocus -->
        <textarea bind:value={text} onkeydown={tab} spellcheck="false" autofocus wrap="off" aria-label="File text"></textarea>
      {/if}

      {#if conflict}
        <div class="notice" role="alert">
          <p>This file was changed by someone or something else after you opened it. Saving now would replace their change.</p>
          <div class="modal-actions">
            <button class="btn" onclick={() => (conflict = false)}>Keep editing</button>
            <button class="btn primary" onclick={() => save(true)}>Replace it anyway</button>
          </div>
        </div>
      {/if}
      {#if leaving}
        <div class="notice" role="alert">
          <p>You have changes that aren't saved.</p>
          <div class="modal-actions">
            <button class="btn" onclick={() => (leaving = false)}>Keep editing</button>
            <button class="btn" onclick={onclose}>Discard them</button>
            <button class="btn primary" onclick={async () => { await save(); if (!dirty) onclose(); else leaving = false; }}>Save and close</button>
          </div>
        </div>
      {/if}
      {#if error}<p class="message error">{error}</p>{/if}

      <div class="modal-actions">
        <span class="status">{loaded.crlf ? "Windows line ends" : "Unix line ends"} · {text.length.toLocaleString()} characters</span>
        <button class="btn" onclick={() => (showDiff = !showDiff)}>{showDiff ? "Back to editing" : "Show changes"}</button>
        <button class="btn" onclick={close}>Close</button>
        <button class="btn primary" disabled={!dirty || saving} onclick={() => save()}>Save</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .editor {
    width: min(960px, 94vw);
    max-width: none;
  }
  .path {
    margin: 0 0 8px;
    color: var(--text-faint);
    font: 12px var(--mono);
    overflow-wrap: anywhere;
  }
  textarea,
  .diff {
    width: 100%;
    height: min(60vh, 560px);
    padding: 10px 12px;
    color: var(--text);
    font: 13px/1.5 var(--mono);
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    resize: none;
    tab-size: 4;
  }
  .diff {
    overflow: auto;
    white-space: pre;
  }
  .line.add {
    color: #7ee787;
    background: rgba(46, 160, 67, 0.15);
  }
  .line.del {
    color: #ffa198;
    background: rgba(248, 81, 73, 0.15);
  }
  .sign {
    display: inline-block;
    width: 1.5em;
    user-select: none;
  }
  .gap,
  .none {
    color: var(--text-faint);
  }
  .notice {
    margin-top: 10px;
    padding: 10px 12px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .status {
    margin-right: auto;
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
