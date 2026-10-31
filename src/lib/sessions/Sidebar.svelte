<script lang="ts">
  import type { SavedSession } from "../backend";
  import type { Library } from "./library.svelte";
  import SessionEditor from "./SessionEditor.svelte";
  import { buildTree, newId, type SshSession, type TreeItem } from "./tree";

  interface Props {
    library: Library;
    /** Open a saved session in a new tab. */
    onopen: (session: SavedSession) => void;
  }

  let { library, onopen }: Props = $props();

  let search = $state("");
  let collapsed = $state<Record<string, boolean>>({});
  let editor = $state<{ editing?: SshSession; folder?: string } | null>(null);
  let confirm = $state<{ kind: "session" | "folder"; id: string; name: string } | null>(null);
  let renaming = $state<{ id: string; name: string } | null>(null);
  let message = $state("");

  const tree = $derived(buildTree(library.file, search));

  function describe(session: SavedSession): string {
    if (session.type === "ssh") return `${session.user}@${session.host}`;
    if (session.type === "serial") return session.port;
    return session.host;
  }

  async function run(action: () => Promise<void>) {
    message = "";
    try {
      await action();
    } catch (err) {
      message = (err as { message?: string }).message ?? "That didn't work.";
    }
  }

  function addFolder(parent: string) {
    renaming = { id: `new:${parent}`, name: "" };
  }

  async function finishRename(save: boolean) {
    const target = renaming;
    renaming = null;
    if (!target || !save || !target.name.trim()) return;
    const name = target.name.trim();
    await run(() => {
      if (target.id.startsWith("new:")) {
        const folder = { id: newId(), name, parent: target.id.slice(4) };
        return library.save({ ...library.file, folder: [...library.file.folder, folder] });
      }
      return library.save({
        ...library.file,
        folder: library.file.folder.map((f) => (f.id === target.id ? { ...f, name } : f)),
      });
    });
  }

  async function doDelete() {
    const target = confirm;
    confirm = null;
    if (!target) return;
    await run(() => (target.kind === "session" ? library.deleteSession(target.id) : library.deleteFolder(target.id)));
  }
</script>

{#snippet row(item: TreeItem, depth: number)}
  {#if item.kind === "folder"}
    <div class="item folder" style:padding-left="{8 + depth * 14}px">
      <button
        class="label"
        onclick={() => (collapsed[item.folder.id] = !collapsed[item.folder.id])}
        ondblclick={() => (renaming = { id: item.folder.id, name: item.folder.name })}
      >
        {collapsed[item.folder.id] && !search ? "▸" : "▾"} {item.folder.name}
      </button>
      <span class="tools">
        <button title="New session here" onclick={() => (editor = { folder: item.folder.id })}>+</button>
        <button title="New folder inside" onclick={() => addFolder(item.folder.id)}>▤</button>
        <button
          title="Delete folder"
          onclick={() => (confirm = { kind: "folder", id: item.folder.id, name: item.folder.name })}>×</button
        >
      </span>
    </div>
    {#if renaming?.id === item.folder.id}
      {@render renameBox(depth + 1)}
    {/if}
    {#if !collapsed[item.folder.id] || search}
      {#each item.children as child (child.kind === "folder" ? child.folder.id : child.session.id)}
        {@render row(child, depth + 1)}
      {/each}
    {/if}
  {:else}
    <div class="item" style:padding-left="{8 + depth * 14}px">
      <button class="label" title={describe(item.session)} ondblclick={() => onopen(item.session)}>
        {item.session.name}
        <small>{describe(item.session)}</small>
      </button>
      <span class="tools">
        <button title="Connect in a new tab" onclick={() => onopen(item.session)}>▶</button>
        {#if item.session.type === "ssh"}
          <button title="Edit" onclick={() => (editor = { editing: item.session as SshSession })}>✎</button>
        {/if}
        <button
          title="Delete"
          onclick={() => (confirm = { kind: "session", id: item.session.id, name: item.session.name })}>×</button
        >
      </span>
    </div>
  {/if}
{/snippet}

{#snippet renameBox(depth: number)}
  <div class="item" style:padding-left="{8 + depth * 14}px">
    <!-- svelte-ignore a11y_autofocus -->
    <input
      class="rename"
      autofocus
      placeholder="Folder name"
      bind:value={renaming!.name}
      onblur={() => finishRename(true)}
      onkeydown={(e) => {
        if (e.key === "Enter") finishRename(true);
        else if (e.key === "Escape") finishRename(false);
        e.stopPropagation();
      }}
    />
  </div>
{/snippet}

<aside class="sidebar">
  <div class="head">
    <input class="search" type="search" placeholder="Search sessions" bind:value={search} />
    <button title="New SSH session" onclick={() => (editor = {})}>+</button>
    <button title="New folder" onclick={() => addFolder("")}>▤</button>
  </div>

  {#if library.problem}
    <div class="problem">
      <p>{library.problem}</p>
      <p>The file hasn't been changed. You can fix it by hand, or start again (the old file is kept as sessions.toml.bad).</p>
      <button onclick={() => run(() => library.startAgain())}>Start again</button>
    </div>
  {/if}

  {#if message}<p class="message">{message}</p>{/if}

  <div class="list">
    {#if renaming?.id.startsWith("new:") && renaming.id === "new:"}
      {@render renameBox(0)}
    {/if}
    {#each tree as item (item.kind === "folder" ? item.folder.id : item.session.id)}
      {@render row(item, 0)}
    {/each}
    {#if tree.length === 0 && !library.problem}
      <p class="empty">{search ? "Nothing matches." : "No saved sessions yet. Use + to add one."}</p>
    {/if}
  </div>
</aside>

{#if editor}
  <SessionEditor {library} editing={editor.editing} folder={editor.folder} onclose={() => (editor = null)} />
{/if}

{#if confirm}
  <div class="backdrop">
    <div class="dialog" role="alertdialog" aria-label="Delete">
      <p>
        Delete {confirm.kind === "folder" ? "the folder" : "the session"} <b>{confirm.name}</b>?
        {#if confirm.kind === "folder"}What's inside moves up one level.{:else}Its saved password is removed too.{/if}
      </p>
      <div class="buttons">
        <!-- svelte-ignore a11y_autofocus -->
        <button autofocus onclick={() => (confirm = null)}>Cancel</button>
        <button class="danger" onclick={doDelete}>Delete</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 250px;
    flex: none;
    background: #252526;
    color: #d4d4d4;
    font: 13px system-ui, sans-serif;
    border-right: 1px solid #1a1a1a;
    min-height: 0;
  }
  .head {
    display: flex;
    gap: 4px;
    padding: 6px;
  }
  .search {
    flex: 1;
    min-width: 0;
    padding: 3px 6px;
    font: inherit;
    color: inherit;
    background: #3c3c3c;
    border: 1px solid #555;
  }
  .list {
    flex: 1;
    overflow: auto;
    padding-bottom: 8px;
  }
  .item {
    display: flex;
    align-items: center;
    padding-right: 4px;
  }
  .item:hover {
    background: #2a2d2e;
  }
  .label {
    flex: 1;
    min-width: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .label small {
    margin-left: 6px;
    color: #8a8a8a;
  }
  .tools {
    display: none;
    gap: 2px;
  }
  .item:hover .tools {
    display: flex;
  }
  button {
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
    padding: 3px 6px;
    border-radius: 3px;
  }
  button:hover {
    background: #4448;
  }
  .rename {
    width: 100%;
    padding: 2px 4px;
    font: inherit;
    color: inherit;
    background: #3c3c3c;
    border: 1px solid #007acc;
  }
  .empty,
  .message {
    margin: 8px;
    color: #9d9d9d;
  }
  .message {
    color: #f48771;
  }
  .problem {
    margin: 6px;
    padding: 8px;
    background: #5a1d1d;
    border-radius: 4px;
  }
  .problem p {
    margin: 0 0 6px;
  }
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
    width: min(380px, 90vw);
    padding: 14px 18px;
    background: #252526;
    color: #d4d4d4;
    border: 1px solid #555;
    border-radius: 6px;
    font: 13px system-ui, sans-serif;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .buttons button {
    padding: 5px 14px;
    background: #3c3c3c;
    border: 1px solid #555;
  }
  .buttons .danger {
    background: #a1260d;
    border-color: #a1260d;
  }
</style>
